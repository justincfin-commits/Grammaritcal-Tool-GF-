//! Renders outcomes as text and as deterministic CBOR.
//!
//! This module holds behavior only. Every function takes data and returns a
//! `String` or a `Vec<u8>`. Nothing here prints; the binary does that.
//!
//! Every error variant gets one plain sentence. The matches are exhaustive
//! on purpose, so that adding a variant without a sentence fails to compile.

use std::fmt::Write;

use crate::analyze::{
    AnalysisOutcome, GrammarOutcome, GrammarSummary, Reading, SentenceOutcome, SentenceResult,
};
use crate::concrete::lintype::{FieldPath, ParamState, PathStep, ValueShape};
use crate::dcbor::{diagnostic, encode, Cbor};
use crate::error::{
    CompileError, CompileStage, ConcreteError, GfError, LexError, LowerError, ParseError,
    ReadError, SourcePos, TypeError, UnsupportedReason, WriteError,
};
use crate::mltt::term::{Binder, ConstName, Term};
use crate::newtype::{DeBruijnIndex, UniverseLevel};
use crate::parse::chart::Rejection;

/// The names the printer hands out to binders written as `_`.
const FRESH_NAMES: [&str; 6] = ["x", "y", "z", "u", "v", "w"];

/// Picks a readable name for one binder at one depth.
fn binder_name(binder: &Binder, depth: usize, taken: &[String]) -> String {
    let wanted = if binder.0 .0 == "_" {
        let base = FRESH_NAMES
            .get(depth % FRESH_NAMES.len())
            .copied()
            .unwrap_or("x");
        let round = depth / FRESH_NAMES.len();
        if round == 0 {
            base.to_string()
        } else {
            format!("{base}{round}")
        }
    } else {
        binder.0 .0.clone()
    };
    let mut candidate = wanted.clone();
    let mut tick: usize = 0;
    while taken.contains(&candidate) {
        tick = tick.saturating_add(1);
        candidate = format!("{wanted}{tick}");
    }
    candidate
}

/// Answers whether a term needs brackets when it stands as an argument.
fn is_atom(term: &Term) -> bool {
    matches!(term, Term::Var(_) | Term::Const(_) | Term::Universe(_) | Term::Pair { .. })
}

/// Prints a universe as the dialect writes it.
fn universe(level: UniverseLevel) -> String {
    if level.0 == 0 {
        "Type".to_string()
    } else {
        format!("Type{}", level.0)
    }
}

/// Prints one term with named binders.
fn write_term(term: &Term, names: &mut Vec<String>, out: &mut String) {
    match term {
        Term::Var(DeBruijnIndex(index)) => {
            let position = names.len().checked_sub(1).and_then(|top| top.checked_sub(*index));
            match position.and_then(|p| names.get(p)) {
                Some(name) => out.push_str(name),
                None => {
                    let _ = write!(out, "#{index}");
                }
            }
        }
        Term::Const(ConstName::Category(name)) => out.push_str(&name.0),
        Term::Const(ConstName::Function(name)) => out.push_str(&name.0),
        Term::Universe(level) => out.push_str(&universe(*level)),
        Term::Pi {
            binder,
            domain,
            codomain,
        } => {
            let name = binder_name(binder, names.len(), names);
            if binder.0 .0 == "_" {
                write_argument(domain, names, out);
                out.push_str(" -> ");
            } else {
                out.push('(');
                out.push_str(&name);
                out.push_str(" : ");
                write_term(domain, names, out);
                out.push_str(") -> ");
            }
            names.push(name);
            write_term(codomain, names, out);
            names.pop();
        }
        Term::Lam { binder, body } => {
            let name = binder_name(binder, names.len(), names);
            out.push('\\');
            out.push_str(&name);
            out.push_str(" -> ");
            names.push(name);
            write_term(body, names, out);
            names.pop();
        }
        Term::App { function, argument } => {
            write_term(function, names, out);
            out.push(' ');
            write_argument(argument, names, out);
        }
        Term::Sigma {
            binder,
            first,
            second,
        } => {
            let name = binder_name(binder, names.len(), names);
            out.push('(');
            out.push_str(&name);
            out.push_str(" : ");
            write_term(first, names, out);
            out.push_str(") ** ");
            names.push(name);
            write_term(second, names, out);
            names.pop();
        }
        Term::Pair { first, second } => {
            out.push('<');
            write_term(first, names, out);
            out.push_str(", ");
            write_term(second, names, out);
            out.push('>');
        }
        Term::First(inner) => {
            out.push_str("fst ");
            write_argument(inner, names, out);
        }
        Term::Second(inner) => {
            out.push_str("snd ");
            write_argument(inner, names, out);
        }
        Term::Id { ty, left, right } => {
            out.push_str("Id ");
            write_argument(ty, names, out);
            out.push(' ');
            write_argument(left, names, out);
            out.push(' ');
            write_argument(right, names, out);
        }
        Term::Refl { ty, point } => {
            out.push_str("refl ");
            write_argument(ty, names, out);
            out.push(' ');
            write_argument(point, names, out);
        }
        Term::J {
            level,
            ty,
            left,
            motive,
            base,
            right,
            proof,
        } => {
            let _ = write!(out, "J {} ", level.0);
            for part in [ty, left, motive, base, right, proof] {
                write_argument(part, names, out);
                out.push(' ');
            }
            out.pop();
        }
    }
}

/// Prints a term that stands as an argument, with brackets when needed.
fn write_argument(term: &Term, names: &mut Vec<String>, out: &mut String) {
    if is_atom(term) {
        write_term(term, names, out);
    } else {
        out.push('(');
        write_term(term, names, out);
        out.push(')');
    }
}

/// Prints a closed term as the dialect writes it.
pub fn render_term(term: &Term) -> String {
    let mut out = String::new();
    let mut names: Vec<String> = Vec::new();
    write_term(term, &mut names, &mut out);
    out
}

/// Prints a type. Types and terms share one syntax.
pub fn render_type(ty: &Term) -> String {
    render_term(ty)
}

/// Prints a source position.
fn position(pos: SourcePos) -> String {
    format!("line {}, column {}", pos.line.0, pos.column.0)
}

/// Prints a field path such as `.s!Sg`.
pub fn render_path(path: &FieldPath) -> String {
    let mut out = String::new();
    for step in &path.0 {
        match step {
            PathStep::Field(name) => {
                out.push('.');
                out.push_str(&name.0);
            }
            PathStep::Row(value) => {
                out.push('!');
                out.push_str(&value.0);
            }
        }
    }
    if out.is_empty() {
        out.push_str("(whole)");
    }
    out
}

/// Prints a parameter state such as `{.n = Sg}`.
pub fn render_state(state: &ParamState) -> String {
    if state.0.is_empty() {
        return "{}".to_string();
    }
    let parts: Vec<String> = state
        .0
        .iter()
        .map(|(path, value)| format!("{} = {}", render_path(path), value.0))
        .collect();
    format!("{{{}}}", parts.join(", "))
}

fn shape(found: ValueShape) -> &'static str {
    match found {
        ValueShape::Text => "a word list",
        ValueShape::Parameter => "a parameter value",
        ValueShape::Record => "a record",
        ValueShape::Table => "a table",
        ValueShape::Function => "a helper that still waits for arguments",
        ValueShape::Binder => "a function argument that was never applied",
    }
}

/// Prints a lexing failure.
pub fn render_lex_error(error: &LexError) -> String {
    match error {
        LexError::UnexpectedCharacter { pos, found } => {
            format!("the grammar file holds the character {found:?} at {}, which no token may contain", position(*pos))
        }
        LexError::UnterminatedString { pos } => {
            format!("the quotation mark at {} is never closed", position(*pos))
        }
        LexError::UnterminatedComment { pos } => {
            format!("the block comment at {} is never closed", position(*pos))
        }
    }
}

/// Prints a parsing failure.
pub fn render_parse_error(error: &ParseError) -> String {
    match error {
        ParseError::Expected {
            pos,
            expected,
            found,
        } => format!("at {}, the parser wanted {expected} and found {found}", position(*pos)),
        ParseError::UnexpectedEnd { expected } => {
            format!("the grammar file ended while the parser still wanted {expected}")
        }
        ParseError::NestingTooDeep { pos } => {
            format!("the grammar file nests brackets too deeply at {}", position(*pos))
        }
        ParseError::AbstractNameMismatch {
            pos,
            expected,
            found,
        } => format!(
            "at {}, the concrete module names the abstract module {} but the abstract file declares {}",
            position(*pos),
            found.0,
            expected.0
        ),
        ParseError::DuplicateDeclaration { pos, name } => {
            format!("at {}, the name {name} is declared a second time", position(*pos))
        }
    }
}

/// Prints a lowering failure.
pub fn render_lower_error(error: &LowerError) -> String {
    match error {
        LowerError::UnknownIdentifier { name } => {
            format!("the abstract module names {name}, which it never declares")
        }
        LowerError::DuplicateDeclaration { name } => {
            format!("the abstract module declares {name} twice")
        }
        LowerError::UnknownStartCategory { name } => {
            format!("the start category {} is never declared", name.0)
        }
        LowerError::MissingStartCategory => {
            "no start category was given and the abstract module has no flags startcat line".to_string()
        }
        LowerError::NestingTooDeep => "a type in the abstract module nests too deeply".to_string(),
    }
}

/// Prints a type checking failure.
pub fn render_type_error(error: &TypeError) -> String {
    match error {
        TypeError::OutOfFuel => "the type checker reached its depth limit".to_string(),
        TypeError::UnboundVariable { index } => {
            format!("the term names a variable, index {}, that no binder introduces", index.0)
        }
        TypeError::UnknownConstant { name } => {
            let shown = match name {
                ConstName::Category(cat) => cat.0.clone(),
                ConstName::Function(fun) => fun.0.clone(),
            };
            format!("the term names the constant {shown}, which the grammar never declares")
        }
        TypeError::NotAFunction { found_type } => {
            format!("the term applies something of type {}, which is not a function", render_type(found_type))
        }
        TypeError::NotAPair { found_type } => {
            format!("the term projects from something of type {}, which is not a pair", render_type(found_type))
        }
        TypeError::NotAType { found_type } => {
            format!("a term of type {} stands where a type belongs", render_type(found_type))
        }
        TypeError::NotAnIdentity { found_type } => {
            format!("the term eliminates something of type {}, which is not an identity proof", render_type(found_type))
        }
        TypeError::BadMotive { found_type } => {
            format!("the motive has type {}, which is the wrong shape", render_type(found_type))
        }
        TypeError::Mismatch { expected, found } => {
            format!("expected type {}, found type {}", render_type(expected), render_type(found))
        }
        TypeError::CannotInfer { subject } => {
            format!("the checker cannot read a type off {} without more information", render_term(subject))
        }
        TypeError::LambdaAgainstNonFunction { expected } => {
            format!("a function stands where a value of type {} belongs", render_type(expected))
        }
        TypeError::PairAgainstNonPairType { expected } => {
            format!("a pair stands where a value of type {} belongs", render_type(expected))
        }
        TypeError::AppliedNonFunction => "evaluation applied a value that is not a function".to_string(),
        TypeError::ProjectedNonPair => "evaluation projected from a value that is not a pair".to_string(),
        TypeError::EliminatedNonProof => {
            "evaluation eliminated a value that is not an identity proof".to_string()
        }
    }
}

/// Prints a linearization evaluation failure.
pub fn render_concrete_error(error: &ConcreteError) -> String {
    match error {
        ConcreteError::OutOfFuel => "the rule evaluator reached its depth limit".to_string(),
        ConcreteError::UnknownIdentifier { name } => {
            format!("the rule names {name}, which the concrete module never declares")
        }
        ConcreteError::NotARecord { found } => {
            format!("the rule projects a field from {}, which is not a record", shape(*found))
        }
        ConcreteError::MissingField { field, available } => {
            let names: Vec<&str> = available.iter().map(|f| f.0.as_str()).collect();
            format!("the record has no field {}; it has {}", field.0, names.join(", "))
        }
        ConcreteError::NotATable { found } => {
            format!("the rule selects a row from {}, which is not a table", shape(*found))
        }
        ConcreteError::MissingRow { value } => format!("the table has no row for {}", value.0),
        ConcreteError::NotAParameter { found } => {
            format!("the rule selects with {}, which is not a parameter value", shape(*found))
        }
        ConcreteError::NotAString { found } => {
            format!("the rule joins {}, which is not a word list", shape(*found))
        }
        ConcreteError::NotAFunction { found } => {
            format!("the rule applies {}, which is not a function", shape(*found))
        }
        ConcreteError::WrongArity {
            name,
            expected,
            found,
        } => format!("the helper {} takes {expected} arguments and received {found}", name.0),
        ConcreteError::UnknownParamValue { value } => {
            format!("the pattern names {}, which no parameter type declares", value.0)
        }
        ConcreteError::NoBranchMatches { value } => {
            format!("no branch covers the parameter value {}", value.0)
        }
        ConcreteError::DuplicateField { field } => {
            format!("the record repeats the field {}", field.0)
        }
        ConcreteError::TableNeedsNamedRow => {
            "the table holds no named row, so its parameter type is unknown".to_string()
        }
        ConcreteError::UnknownParamTypeName { name } => {
            format!("the parameter type {} is never declared", name.0)
        }
        ConcreteError::ShapeMismatch { found } => {
            format!("the value is {}, which does not match the declared linearization type", shape(*found))
        }
        ConcreteError::BinderNotApplied { arg } => {
            format!("argument {} is a function, and the rule never applies it to a record", arg.0)
        }
        ConcreteError::GapNotEmpty { arg } => format!(
            "argument {} is a function, and the record it is applied to prints words; the record that names the missing phrase must hold empty strings only",
            arg.0
        ),
    }
}

fn reason(reason: &UnsupportedReason) -> &'static str {
    match reason {
        UnsupportedReason::NestedFunction => "a function whose argument or result is itself a function",
        UnsupportedReason::NotACategory => "neither a category nor a function into a category",
        UnsupportedReason::Universe => "a universe, which no word sequence can name",
        UnsupportedReason::NotSupportedByParser => "a pair type or an identity type",
    }
}

fn stage(stage: CompileStage) -> &'static str {
    match stage {
        CompileStage::ArgumentTypes => "walking the declared type of a constructor",
        CompileStage::ResultValue => "walking the value a rule produced",
        CompileStage::Pruning => "pruning the compiled grammar",
    }
}

/// Prints a compile failure.
pub fn render_compile_error(error: &CompileError) -> String {
    match error {
        CompileError::MissingLinCat { cat } => {
            format!("the concrete module declares no lincat for the category {}", cat.0)
        }
        CompileError::MissingLin { fun } => {
            format!("the concrete module declares no lin for the constructor {}", fun.0)
        }
        CompileError::LinArityMismatch {
            fun,
            expected,
            found,
        } => format!(
            "the rule for {} binds {found} names but the constructor takes {expected} arguments",
            fun.0
        ),
        CompileError::UnknownParamType { name } => {
            format!("the parameter type {} is never declared", name.0)
        }
        CompileError::DuplicateParamValue { value } => {
            format!("two parameter types declare the value {}", value.0)
        }
        CompileError::ArgumentDropped { fun, arg, path } => format!(
            "in the rule for {}, output field {} never mentions argument {}, so the parser could not recover it",
            fun.0,
            render_path(path),
            arg.0
        ),
        CompileError::ArgumentRepeated { fun, arg, path } => format!(
            "in the rule for {}, output field {} mentions argument {} more than once, which no context-free production can express",
            fun.0,
            render_path(path),
            arg.0
        ),
        CompileError::UnsupportedArgumentType { fun, arg, reason: why } => format!(
            "argument {} of {} has a type the parser cannot compile: {}",
            arg.0,
            fun.0,
            reason(why)
        ),
        CompileError::UnsupportedResultType { fun } => {
            format!("the result type of {} is not a category", fun.0)
        }
        CompileError::ResultShapeMismatch { fun, cat, found } => format!(
            "the rule for {} produces {}, which does not match the lincat of {}",
            fun.0,
            shape(*found),
            cat.0
        ),
        CompileError::StateExplosion { cat, count, limit } => format!(
            "the category {} carries {count} parameter states, more than the limit of {}",
            cat.0, limit.0
        ),
        CompileError::Evaluation { fun, error } => {
            format!("the rule for {} failed: {}", fun.0, render_concrete_error(error))
        }
        CompileError::TooManyProductions { count, limit } => {
            format!("the compiled grammar holds {count} productions, more than the limit of {}", limit.0)
        }
        CompileError::DuplicateLinCat { cat } => {
            format!("the concrete module declares two lincats for {}", cat.0)
        }
        CompileError::DuplicateLin { fun } => {
            format!("the concrete module declares two rules for {}", fun.0)
        }
        CompileError::LinForUnknownFunction { fun } => {
            format!("the concrete module has a rule for {}, which the abstract module never declares", fun.0)
        }
        CompileError::LinCatForUnknownCategory { cat } => {
            format!("the concrete module has a lincat for {}, which the abstract module never declares", cat.0)
        }
        CompileError::CategoryPrintsNothing { cat } => {
            format!("no part of the category {} prints words, so no sentence can carry it", cat.0)
        }
        CompileError::RuleStateExplosion { fun, count, limit } => format!(
            "the rule for {} fixes {count} combinations of argument states, more than the limit of {}",
            fun.0, limit.0
        ),
        CompileError::TooManyGaps { count, limit } => {
            format!("the grammar asks for {count} distinct missing phrases, more than the limit of {}", limit.0)
        }
        CompileError::OutOfFuel { stage: which } => {
            format!("the compiler reached its depth limit while {}", stage(*which))
        }
        CompileError::UnitCycle { cat, state, path } => format!(
            "the nonterminal {} {} {} derives itself with no words in between, so some sentence would have unboundedly many trees",
            cat.0,
            render_state(state),
            render_path(path)
        ),
        CompileError::NullableUnstable => {
            "the compiler could not settle which nonterminals derive nothing; this is a defect in the tool".to_string()
        }
    }
}

/// Prints a reading failure.
pub fn render_read_error(error: &ReadError) -> String {
    match error {
        ReadError::EmptyInput => "the input holds no sentence".to_string(),
        ReadError::SentenceTooLong {
            sentence,
            length,
            limit,
        } => format!(
            "sentence {} holds {length} words, more than the limit of {}",
            sentence.0, limit.0
        ),
        ReadError::NoParse => "no rule of the grammar matches any word of the sentence".to_string(),
        ReadError::NoParseAt { position, expected } => {
            let words: Vec<&str> = expected.iter().map(|t| t.0.as_str()).collect();
            if words.is_empty() {
                format!("the grammar covers the first {} words and then nothing more", position.0)
            } else {
                format!(
                    "the grammar covers the first {} words and then wanted one of: {}",
                    position.0,
                    words.join(", ")
                )
            }
        }
        ReadError::UnknownWord { token } => {
            format!("the word {:?} appears in no rule of the grammar", token.0)
        }
        ReadError::UnknownWordCaseOnly {
            token,
            grammar_spelling,
        } => format!(
            "the word {:?} appears in the grammar only as {:?}; capitals carry meaning here",
            token.0, grammar_spelling.0
        ),
        ReadError::TreeLimitReached { returned } => format!(
            "the tree limit stopped the search before a well typed tree was found ({returned} returned); raise the limit"
        ),
        ReadError::ChartTooLarge { position, limit } => format!(
            "the chart at word {} grew past {} items",
            position.0, limit.0
        ),
        ReadError::TooManySplits { production, limit } => format!(
            "production {} splits the sentence in more than {} ways",
            production.0, limit.0
        ),
        ReadError::OutOfFuel => "the tree rebuilder reached its depth limit".to_string(),
        ReadError::WrongTraceCount { fun, arg, found } => format!(
            "argument {} of {} received a phrase with {found} places for its bound variable instead of one; this is a defect in the tool",
            arg.0, fun.0
        ),
        ReadError::TraceOutsideBinder { cat, state } => format!(
            "a bound variable stands in a complete phrase of {} {}; this is a defect in the tool",
            cat.0,
            render_state(state)
        ),
        ReadError::BrokenDerivation => {
            "a derivation names something the compiled grammar does not hold; this is a defect in the tool".to_string()
        }
    }
}

/// Prints a printing failure.
pub fn render_write_error(error: &WriteError) -> String {
    match error {
        WriteError::UnknownConstructor { fun } => {
            format!("the tree applies {}, which the grammar never declares", fun.0)
        }
        WriteError::NotAConstructorTree => "the tree is not an application of a constructor".to_string(),
        WriteError::WrongArity {
            fun,
            expected,
            found,
        } => format!("the tree gives {} {found} arguments instead of {expected}", fun.0),
        WriteError::Evaluation { fun, error } => {
            format!("printing {} failed: {}", fun.0, render_concrete_error(error))
        }
        WriteError::NoSuchState { cat, state } => {
            format!("the printed {} carries the state {}, which was not asked for", cat.0, render_state(state))
        }
        WriteError::LooseVariable { name } => {
            format!("the variable {} stands outside any binder", name.0)
        }
        WriteError::OutOfFuel => "the printer reached its depth limit".to_string(),
    }
}

/// Prints any failure.
pub fn render_error(error: &GfError) -> String {
    match error {
        GfError::Lex(inner) => render_lex_error(inner),
        GfError::Parse(inner) => render_parse_error(inner),
        GfError::Lower(inner) => render_lower_error(inner),
        GfError::Type(inner) => render_type_error(inner),
        GfError::Concrete(inner) => render_concrete_error(inner),
        GfError::Compile(inner) => render_compile_error(inner),
        GfError::Read(inner) => render_read_error(inner),
        GfError::Write(inner) => render_write_error(inner),
    }
}

/// Names the kind of a failure for the structured output.
fn error_kind(error: &GfError) -> &'static str {
    match error {
        GfError::Lex(_) => "lex",
        GfError::Parse(_) => "parse",
        GfError::Lower(_) => "lower",
        GfError::Type(_) => "type",
        GfError::Concrete(_) => "concrete",
        GfError::Compile(_) => "compile",
        GfError::Read(_) => "read",
        GfError::Write(_) => "write",
    }
}

fn write_summary(summary: &GrammarSummary, out: &mut String) {
    let _ = writeln!(
        out,
        "grammar {} / {}: start category {}, {} constants, {} nonterminals, {} productions ({} before gap threading), {} gaps, {} words",
        summary.abstract_name.0,
        summary.concrete_name.0,
        summary.start.0,
        summary.constants,
        summary.nonterminals,
        summary.productions,
        summary.base_productions,
        summary.gaps,
        summary.words
    );
}

fn write_sentence(sentence: &SentenceOutcome, out: &mut String) {
    let words: Vec<&str> = sentence.words.iter().map(|t| t.0.as_str()).collect();
    let _ = writeln!(out, "sentence {}: {}", sentence.index.0.saturating_add(1), words.join(" "));
    match &sentence.result {
        SentenceResult::Failed(error) => {
            let _ = writeln!(out, "  no reading: {}", render_read_error(error));
        }
        SentenceResult::Readings {
            accepted,
            rejected,
            rejected_total,
            truncated,
            cycles_cut,
        } => {
            if accepted.is_empty() {
                let _ = writeln!(
                    out,
                    "  no reading: {rejected_total} candidate tree(s) parsed and every one failed to type check"
                );
            } else {
                let _ = writeln!(out, "  {} reading(s)", accepted.len());
            }
            for (position, reading) in accepted.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "  [{}] {}\n      : {}",
                    position.saturating_add(1),
                    render_term(&reading.tree),
                    render_type(&reading.ty)
                );
            }
            // When a reading exists, the subtrees refused on the way are
            // only noise. When none exists, they are the evidence.
            if accepted.is_empty() {
                for rejection in rejected {
                    let _ = writeln!(
                        out,
                        "  rejected {}\n      because {}",
                        render_term(&rejection.tree),
                        render_type_error(&rejection.error)
                    );
                }
                if *rejected_total > rejected.len() {
                    let _ = writeln!(
                        out,
                        "  ... and {} more rejected candidate(s)",
                        rejected_total.saturating_sub(rejected.len())
                    );
                }
            } else if *rejected_total > 0 {
                let _ = writeln!(
                    out,
                    "  ({rejected_total} candidate subtree(s) refused by the type checker on the way)"
                );
            }
            if *truncated {
                let _ = writeln!(out, "  note: the tree limit cut the search short");
            }
            if *cycles_cut > 0 {
                let _ = writeln!(out, "  note: {cycles_cut} derivation(s) were cut for repeating a span");
            }
        }
    }
}

/// Renders a whole outcome as a text report.
pub fn render_text(outcome: &AnalysisOutcome) -> String {
    let mut out = String::new();
    match &outcome.grammar {
        GrammarOutcome::Failed(failures) => {
            let _ = writeln!(out, "the grammar failed to load with {} problem(s):", failures.len());
            for failure in failures {
                let _ = writeln!(out, "  - {}", render_error(failure));
            }
            return out;
        }
        GrammarOutcome::Loaded(summary) => write_summary(summary, &mut out),
    }
    if let Some(failure) = &outcome.input_failure {
        let _ = writeln!(out, "the input could not be read: {}", render_read_error(failure));
        return out;
    }
    for sentence in &outcome.sentences {
        write_sentence(sentence, &mut out);
    }
    out
}

fn text(s: &str) -> Cbor {
    Cbor::Text(s.to_string())
}

fn count(n: usize) -> Cbor {
    Cbor::Uint(n as u64)
}

fn map(pairs: Vec<(&str, Cbor)>) -> Cbor {
    Cbor::Map(pairs.into_iter().map(|(key, value)| (text(key), value)).collect())
}

/// Turns a term into structured CBOR. Every node is a one-key map whose key
/// names the constructor.
fn term_cbor(term: &Term) -> Cbor {
    match term {
        Term::Var(index) => map(vec![("var", count(index.0))]),
        Term::Const(ConstName::Category(name)) => map(vec![("cat", text(&name.0))]),
        Term::Const(ConstName::Function(name)) => map(vec![("fun", text(&name.0))]),
        Term::Universe(level) => map(vec![("universe", Cbor::Uint(u64::from(level.0)))]),
        Term::Pi {
            binder,
            domain,
            codomain,
        } => map(vec![(
            "pi",
            Cbor::Array(vec![text(&binder.0 .0), term_cbor(domain), term_cbor(codomain)]),
        )]),
        Term::Lam { binder, body } => map(vec![(
            "lam",
            Cbor::Array(vec![text(&binder.0 .0), term_cbor(body)]),
        )]),
        Term::App { function, argument } => map(vec![(
            "app",
            Cbor::Array(vec![term_cbor(function), term_cbor(argument)]),
        )]),
        Term::Sigma {
            binder,
            first,
            second,
        } => map(vec![(
            "sigma",
            Cbor::Array(vec![text(&binder.0 .0), term_cbor(first), term_cbor(second)]),
        )]),
        Term::Pair { first, second } => map(vec![(
            "pair",
            Cbor::Array(vec![term_cbor(first), term_cbor(second)]),
        )]),
        Term::First(inner) => map(vec![("fst", term_cbor(inner))]),
        Term::Second(inner) => map(vec![("snd", term_cbor(inner))]),
        Term::Id { ty, left, right } => map(vec![(
            "id",
            Cbor::Array(vec![term_cbor(ty), term_cbor(left), term_cbor(right)]),
        )]),
        Term::Refl { ty, point } => map(vec![(
            "refl",
            Cbor::Array(vec![term_cbor(ty), term_cbor(point)]),
        )]),
        Term::J {
            level,
            ty,
            left,
            motive,
            base,
            right,
            proof,
        } => map(vec![(
            "j",
            Cbor::Array(vec![
                Cbor::Uint(u64::from(level.0)),
                term_cbor(ty),
                term_cbor(left),
                term_cbor(motive),
                term_cbor(base),
                term_cbor(right),
                term_cbor(proof),
            ]),
        )]),
    }
}

fn reading_cbor(reading: &Reading) -> Cbor {
    map(vec![
        ("tree", term_cbor(&reading.tree)),
        ("tree_text", text(&render_term(&reading.tree))),
        ("type", term_cbor(&reading.ty)),
        ("type_text", text(&render_type(&reading.ty))),
    ])
}

fn rejection_cbor(rejection: &Rejection) -> Cbor {
    map(vec![
        ("tree", term_cbor(&rejection.tree)),
        ("tree_text", text(&render_term(&rejection.tree))),
        ("error", text(&render_type_error(&rejection.error))),
    ])
}

fn error_cbor(error: &GfError) -> Cbor {
    map(vec![
        ("kind", text(error_kind(error))),
        ("message", text(&render_error(error))),
    ])
}

fn sentence_cbor(sentence: &SentenceOutcome) -> Cbor {
    let words: Vec<Cbor> = sentence.words.iter().map(|t| text(&t.0)).collect();
    let mut pairs: Vec<(&str, Cbor)> = vec![
        ("index", count(sentence.index.0)),
        ("words", Cbor::Array(words)),
        ("start", count(sentence.span.start.0)),
        ("end", count(sentence.span.end.0)),
    ];
    match &sentence.result {
        SentenceResult::Failed(error) => {
            pairs.push(("failure", text(&render_read_error(error))));
        }
        SentenceResult::Readings {
            accepted,
            rejected,
            rejected_total,
            truncated,
            cycles_cut,
        } => {
            pairs.push(("readings", Cbor::Array(accepted.iter().map(reading_cbor).collect())));
            pairs.push(("rejected", Cbor::Array(rejected.iter().map(rejection_cbor).collect())));
            pairs.push(("rejected_total", count(*rejected_total)));
            pairs.push(("truncated", Cbor::Bool(*truncated)));
            pairs.push(("cycles_cut", count(*cycles_cut)));
        }
    }
    map(pairs)
}

/// Turns a whole outcome into one CBOR item.
pub fn outcome_cbor(outcome: &AnalysisOutcome) -> Cbor {
    let grammar = match &outcome.grammar {
        GrammarOutcome::Failed(failures) => map(vec![(
            "failures",
            Cbor::Array(failures.iter().map(error_cbor).collect()),
        )]),
        GrammarOutcome::Loaded(summary) => map(vec![
            ("abstract", text(&summary.abstract_name.0)),
            ("concrete", text(&summary.concrete_name.0)),
            ("start", text(&summary.start.0)),
            ("constants", count(summary.constants)),
            ("nonterminals", count(summary.nonterminals)),
            ("productions", count(summary.productions)),
            ("base_productions", count(summary.base_productions)),
            ("gaps", count(summary.gaps)),
            ("words", count(summary.words)),
        ]),
    };
    let mut pairs: Vec<(&str, Cbor)> = vec![
        ("format", text("gf-mltt/1")),
        ("grammar", grammar),
        (
            "sentences",
            Cbor::Array(outcome.sentences.iter().map(sentence_cbor).collect()),
        ),
    ];
    if let Some(failure) = &outcome.input_failure {
        pairs.push(("input_failure", text(&render_read_error(failure))));
    }
    map(pairs)
}

/// Renders a whole outcome as deterministic CBOR bytes.
///
/// The encoder can fail only on a duplicate map key, and every map built here
/// uses distinct literal keys, so the failure branch returns an empty list.
pub fn render_dcbor(outcome: &AnalysisOutcome) -> Vec<u8> {
    encode(&outcome_cbor(outcome)).unwrap_or_default()
}

/// Renders a whole outcome in CBOR diagnostic notation.
pub fn render_diagnostic(outcome: &AnalysisOutcome) -> String {
    diagnostic(&outcome_cbor(outcome))
}

/// Renders one sentence's outcome as deterministic CBOR bytes, for the test
/// vector files the command line program writes.
pub fn render_sentence_dcbor(sentence: &SentenceOutcome) -> Vec<u8> {
    encode(&sentence_cbor(sentence)).unwrap_or_default()
}

/// Renders one sentence's outcome in CBOR diagnostic notation.
pub fn render_sentence_diagnostic(sentence: &SentenceOutcome) -> String {
    diagnostic(&sentence_cbor(sentence))
}
