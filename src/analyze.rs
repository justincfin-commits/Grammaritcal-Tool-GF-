//! The pure top-level function that runs every stage.
//!
//! The data at the top of this module describes one request and one outcome.
//! The function `analyze` at the bottom takes a request and returns an
//! outcome. It reads no file, opens no socket, reads no clock, draws no
//! random number, and mutates no global state. Two calls on equal requests
//! return equal outcomes, and `tests/determinism.rs` checks that byte for
//! byte through the renderers.
//!
//! Five stages run inside. The GF parser reads the abstract and the concrete
//! grammar text. The lowering step turns `cat` and `fun` declarations into
//! constants of the type theory and checks the whole signature. The compiler
//! turns each `lin` rule into context-free productions. An Earley parser
//! reads each sentence into candidate trees. The dependent type checker
//! rejects every candidate that violates a type index and returns the
//! survivors with their normal types.

use crate::compile::grammar::CompileLimits;
use crate::compile::productions::compile;
use crate::error::{GfError, ParseError, ReadError};
use crate::gf::lexer::lex;
use crate::gf::lower::lower_abstract;
use crate::gf::parser::{parse_abstract, parse_concrete};
use crate::mltt::check::infer_closed;
use crate::mltt::signature::check_signature;
use crate::mltt::term::Term;
use crate::newtype::{
    CatName, EvalFuel, GapLimit, InputText, ItemLimit, ModuleName, ProductionLimit,
    RejectionLimit, SentenceIndex, SentenceLimit, SourceText, SplitLimit, StateLimit, Token,
    TreeLimit,
};
use crate::parse::chart::{Rejection, TypeFilter};
use crate::parse::earley::{build_forest, build_tables, parse, resolve};
use crate::parse::forest::enumerate;
use crate::parse::input::Span;
use crate::parse::tokenize::tokenize;

/// Every fuel counter and every cap the tool honors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The depth budget for the type checker and the normalizer.
    pub type_fuel: EvalFuel,
    /// The depth budget for evaluating one `lin` body.
    pub rule_fuel: EvalFuel,
    /// The depth budget for walking one type or one value in the compiler.
    pub walk_fuel: EvalFuel,
    /// The depth budget for rebuilding trees from the chart.
    pub forest_fuel: EvalFuel,
    /// How many rounds each fixed point in the compiler may run.
    pub rounds: EvalFuel,
    /// How many parameter states one category may carry.
    pub states_per_category: StateLimit,
    /// How many combinations of argument states one rule may fix.
    pub states_per_rule: StateLimit,
    /// How many productions the compiled grammar may hold.
    pub productions: ProductionLimit,
    /// How many distinct missing phrases the compiler may thread.
    pub gaps: GapLimit,
    /// How many words one sentence may hold.
    pub sentence_words: SentenceLimit,
    /// How many items one chart column may hold.
    pub chart_items: ItemLimit,
    /// How many ways of splitting one production the rebuilder follows.
    pub splits: SplitLimit,
    /// How many trees the rebuilder keeps per span.
    pub trees: TreeLimit,
    /// How many rejected subtrees a report shows for one sentence.
    pub rejections_shown: RejectionLimit,
}

/// The limits the command line program uses when the caller names none.
pub const DEFAULT_LIMITS: Limits = Limits {
    type_fuel: EvalFuel(4096),
    rule_fuel: EvalFuel(256),
    walk_fuel: EvalFuel(256),
    forest_fuel: EvalFuel(4096),
    rounds: EvalFuel(4096),
    states_per_category: StateLimit(64),
    states_per_rule: StateLimit(256),
    productions: ProductionLimit(20_000),
    gaps: GapLimit(16),
    sentence_words: SentenceLimit(64),
    chart_items: ItemLimit(50_000),
    splits: SplitLimit(256),
    trees: TreeLimit(64),
    rejections_shown: RejectionLimit(8),
};

/// Everything one call of `analyze` reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisRequest {
    /// The text of the abstract grammar file.
    pub abstract_source: SourceText,
    /// The text of the concrete grammar file.
    pub concrete_source: SourceText,
    /// The text to read.
    pub input: InputText,
    /// The category a sentence must produce. When absent, the abstract module
    /// must carry a `flags startcat` line.
    pub start: Option<CatName>,
    /// Every fuel counter and cap.
    pub limits: Limits,
}

/// What the tool learned about the grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct GrammarSummary {
    pub abstract_name: ModuleName,
    pub concrete_name: ModuleName,
    pub start: CatName,
    /// How many constants the signature declares.
    pub constants: usize,
    /// How many nonterminals the compiled grammar holds.
    pub nonterminals: usize,
    /// How many productions the compiled grammar holds after pruning.
    pub productions: usize,
    /// How many productions the compiler made before threading gaps.
    pub base_productions: usize,
    /// How many distinct missing phrases the compiler threaded.
    pub gaps: usize,
    /// How many distinct words the grammar prints.
    pub words: usize,
}

/// Whether the grammar loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrammarOutcome {
    /// The grammar loaded and compiled.
    Loaded(GrammarSummary),
    /// The grammar failed. Every failure the tool found is listed.
    Failed(Vec<GfError>),
}

/// One well typed tree, with its type in normal form.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Reading {
    pub tree: Term,
    pub ty: Term,
}

/// What one sentence produced.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SentenceResult {
    /// The sentence parsed. The accepted list holds every well typed tree.
    /// An empty accepted list with a non-zero rejected count means every
    /// candidate failed to type check.
    Readings {
        accepted: Vec<Reading>,
        /// The first few candidates the type checker refused.
        rejected: Vec<Rejection>,
        /// How many candidates the type checker refused in all.
        rejected_total: usize,
        /// True when the rebuilder stopped at the tree limit.
        truncated: bool,
        /// How many derivations the rebuilder cut for repeating a span.
        cycles_cut: usize,
    },
    /// The sentence did not reach the type checker.
    Failed(ReadError),
}

/// One sentence of the input, with what the tool made of it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct SentenceOutcome {
    pub index: SentenceIndex,
    /// The words as the input wrote them.
    pub words: Vec<Token>,
    pub span: Span,
    pub result: SentenceResult,
}

/// Everything one call of `analyze` returns.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct AnalysisOutcome {
    pub grammar: GrammarOutcome,
    /// A failure that stopped the tool before it split the input into
    /// sentences.
    pub input_failure: Option<ReadError>,
    pub sentences: Vec<SentenceOutcome>,
}

/// Runs every stage on one request.
pub fn analyze(request: &AnalysisRequest) -> AnalysisOutcome {
    let limits = request.limits;
    let failed = |failures: Vec<GfError>| AnalysisOutcome {
        grammar: GrammarOutcome::Failed(failures),
        input_failure: None,
        sentences: Vec::new(),
    };

    // Stage 1: read the grammar files.
    let abstract_tokens = match lex(&request.abstract_source) {
        Ok(tokens) => tokens,
        Err(failure) => return failed(vec![GfError::Lex(failure)]),
    };
    let abstract_module = match parse_abstract(&abstract_tokens) {
        Ok(module) => module,
        Err(failure) => return failed(vec![GfError::Parse(failure)]),
    };
    let concrete_tokens = match lex(&request.concrete_source) {
        Ok(tokens) => tokens,
        Err(failure) => return failed(vec![GfError::Lex(failure)]),
    };
    let concrete = match parse_concrete(&concrete_tokens) {
        Ok(module) => module,
        Err(failure) => return failed(vec![GfError::Parse(failure)]),
    };
    if concrete.abstract_name != abstract_module.name {
        let pos = concrete_tokens
            .get(3)
            .map(|lexeme| lexeme.pos)
            .unwrap_or(crate::error::SourcePos {
                line: crate::newtype::Line(1),
                column: crate::newtype::Column(1),
            });
        return failed(vec![GfError::Parse(ParseError::AbstractNameMismatch {
            pos,
            expected: abstract_module.name.clone(),
            found: concrete.abstract_name.clone(),
        })]);
    }

    // Stage 2: lower the declarations and check the signature.
    let grammar = match lower_abstract(&abstract_module, request.start.as_ref()) {
        Ok(grammar) => grammar,
        Err(failures) => return failed(failures.into_iter().map(GfError::Lower).collect()),
    };
    let signature_failures = check_signature(&grammar.signature, limits.type_fuel);
    if !signature_failures.is_empty() {
        return failed(
            signature_failures
                .into_iter()
                .map(|(_, failure)| GfError::Type(failure))
                .collect(),
        );
    }

    // Stage 3: compile the rules.
    let compile_limits = CompileLimits {
        states_per_category: limits.states_per_category,
        states_per_rule: limits.states_per_rule,
        productions: limits.productions,
        gaps: limits.gaps,
        rule_fuel: limits.rule_fuel,
        walk_fuel: limits.walk_fuel,
        rounds: limits.rounds,
    };
    let compiled = match compile(&grammar, &concrete, compile_limits) {
        Ok(compiled) => compiled,
        Err(failures) => return failed(failures.into_iter().map(GfError::Compile).collect()),
    };
    let tables = build_tables(&compiled);
    let summary = GrammarSummary {
        abstract_name: abstract_module.name.clone(),
        concrete_name: concrete.name.clone(),
        start: grammar.start.clone(),
        constants: grammar.signature.order.len(),
        nonterminals: compiled.nonterminals.len(),
        productions: compiled.productions.len(),
        base_productions: compiled.base_count,
        gaps: compiled.gap_count,
        words: compiled.words.len(),
    };

    // Stage 4 and 5: read each sentence and keep the well typed trees.
    let sentences = match tokenize(&request.input, limits.sentence_words) {
        Ok(sentences) => sentences,
        Err(failure) => {
            return AnalysisOutcome {
                grammar: GrammarOutcome::Loaded(summary),
                input_failure: Some(failure),
                sentences: Vec::new(),
            }
        }
    };
    let filter = TypeFilter {
        signature: &grammar.signature,
        fuel: limits.type_fuel,
        keep: limits.rejections_shown,
    };
    let mut outcomes: Vec<SentenceOutcome> = Vec::new();
    for sentence in &sentences {
        let words: Vec<Token> = sentence.tokens.iter().map(|t| t.surface.clone()).collect();
        let result = read_sentence(&tables, &filter, &sentence.tokens, limits);
        outcomes.push(SentenceOutcome {
            index: sentence.index,
            words,
            span: sentence.span,
            result,
        });
    }
    AnalysisOutcome {
        grammar: GrammarOutcome::Loaded(summary),
        input_failure: None,
        sentences: outcomes,
    }
}

/// Reads one sentence through the chart parser and the type checker.
fn read_sentence(
    tables: &crate::parse::chart::ParseTables,
    filter: &TypeFilter<'_>,
    tokens: &[crate::parse::input::InputToken],
    limits: Limits,
) -> SentenceResult {
    let resolved = match resolve(tables, tokens) {
        Ok(resolved) => resolved,
        Err(failure) => return SentenceResult::Failed(failure),
    };
    let chart = match parse(tables, &resolved, limits.chart_items) {
        Ok(chart) => chart,
        Err(failure) => return SentenceResult::Failed(failure),
    };
    let forest = match build_forest(tables, &chart, resolved.len()) {
        Ok(forest) => forest,
        Err(failure) => return SentenceResult::Failed(failure),
    };
    let found = match enumerate(
        tables,
        &forest,
        limits.trees,
        limits.splits,
        limits.forest_fuel,
        Some(filter),
    ) {
        Ok(found) => found,
        Err(failure) => return SentenceResult::Failed(failure),
    };
    // Stage 5 proper: every root candidate goes through `infer` once more,
    // and the type it gets back is the type the report shows.
    let mut accepted: Vec<Reading> = Vec::new();
    let mut rejected = found.rejected;
    let mut rejected_total = found.rejected_total;
    for tree in found.terms {
        match infer_closed(filter.signature, &tree, filter.fuel) {
            Ok(ty) => accepted.push(Reading { tree, ty }),
            Err(error) => {
                rejected_total = rejected_total.saturating_add(1);
                if rejected.len() < filter.keep.0 {
                    rejected.push(Rejection { tree, error });
                }
            }
        }
    }
    if accepted.is_empty() && found.truncated {
        return SentenceResult::Failed(ReadError::TreeLimitReached { returned: 0 });
    }
    SentenceResult::Readings {
        accepted,
        rejected,
        rejected_total,
        truncated: found.truncated,
        cycles_cut: found.cycles_cut,
    }
}
