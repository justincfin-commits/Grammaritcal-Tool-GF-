//! Tests for the tokenizer, the Earley parser, and the tree rebuilder.

use gf_mltt::compile::grammar::{CompileLimits, CompiledGrammar};
use gf_mltt::compile::productions::compile;
use gf_mltt::error::ReadError;
use gf_mltt::gf::lexer::lex;
use gf_mltt::gf::lower::{lower_abstract, AbstractGrammar};
use gf_mltt::gf::parser::{parse_abstract, parse_concrete};
use gf_mltt::mltt::term::{Binder, ConstName, Term};
use gf_mltt::newtype::{
    DeBruijnIndex, EvalFuel, FunName, GapLimit, InputText, ItemLimit, ProductionLimit,
    RejectionLimit, SentenceLimit, SourceText, SplitLimit, StateLimit, TreeLimit, VarName,
};
use gf_mltt::parse::chart::{Enumeration, TypeFilter};
use gf_mltt::parse::earley::{build_forest, build_tables, parse, resolve};
use gf_mltt::parse::forest::enumerate;
use gf_mltt::parse::input::{MatchKind, SentenceEnd};
use gf_mltt::parse::tokenize::tokenize;

const LOGIC: &str = include_str!("../grammars/Logic.gf");
const LOGIC_ENG: &str = include_str!("../grammars/LogicEng.gf");

fn limits() -> CompileLimits {
    CompileLimits {
        states_per_category: StateLimit(64),
        states_per_rule: StateLimit(256),
        productions: ProductionLimit(20_000),
        gaps: GapLimit(16),
        rule_fuel: EvalFuel(256),
        walk_fuel: EvalFuel(256),
        rounds: EvalFuel(1024),
    }
}

fn load(abstract_text: &str, concrete_text: &str) -> (AbstractGrammar, CompiledGrammar) {
    let abstract_module = parse_abstract(&lex(&SourceText(abstract_text.to_string())).unwrap()).unwrap();
    let concrete = parse_concrete(&lex(&SourceText(concrete_text.to_string())).unwrap()).unwrap();
    let grammar = lower_abstract(&abstract_module, None).unwrap();
    let compiled = compile(&grammar, &concrete, limits()).unwrap();
    (grammar, compiled)
}

fn words(text: &str) -> Vec<Vec<String>> {
    tokenize(&InputText(text.to_string()), SentenceLimit(64))
        .unwrap()
        .into_iter()
        .map(|s| s.tokens.into_iter().map(|t| t.surface.0).collect())
        .collect()
}

#[test]
fn the_tokenizer_splits_sentences_at_terminators_and_blank_lines() {
    assert_eq!(
        words("Every dog barks. John sleeps!\n\nFido barks"),
        vec![
            vec!["Every", "dog", "barks"],
            vec!["John", "sleeps"],
            vec!["Fido", "barks"],
        ]
    );
    let sentences = tokenize(&InputText("a b. c\n\nd".to_string()), SentenceLimit(64)).unwrap();
    let ends: Vec<SentenceEnd> = sentences.iter().map(|s| s.end).collect();
    assert_eq!(
        ends,
        vec![SentenceEnd::Terminator, SentenceEnd::Break, SentenceEnd::EndOfText]
    );
}

#[test]
fn the_tokenizer_strips_brackets_and_keeps_inner_full_stops() {
    assert_eq!(
        words("(\"the rock\", falls); 3.5 stays."),
        vec![vec!["the", "rock", "falls", "3.5", "stays"]]
    );
}

#[test]
fn the_tokenizer_refuses_empty_input_and_long_sentences() {
    assert_eq!(
        tokenize(&InputText("  \n\n ".to_string()), SentenceLimit(64)).unwrap_err(),
        ReadError::EmptyInput
    );
    let failure = tokenize(&InputText("a b c d".to_string()), SentenceLimit(3)).unwrap_err();
    assert!(matches!(failure, ReadError::SentenceTooLong { length: 4, .. }));
}

#[test]
fn a_capital_folds_to_a_grammar_word_but_a_small_word_never_folds_up() {
    let (_, compiled) = load(LOGIC, LOGIC_ENG);
    let tables = build_tables(&compiled);
    let sentence = &tokenize(&InputText("Every dog barks.".to_string()), SentenceLimit(64)).unwrap()[0];
    let resolved = resolve(&tables, &sentence.tokens).unwrap();
    let kinds: Vec<MatchKind> = resolved.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec![MatchKind::Folded, MatchKind::Exact, MatchKind::Exact]);

    let shouting = "abstract R = { flags startcat = S ; cat S ; fun must : S ; }";
    let shouting_eng = "concrete REng of R = { lincat S = { s : Str } ; lin must = { s = \"MUST\" } ; }";
    let (_, compiled) = load(shouting, shouting_eng);
    let tables = build_tables(&compiled);
    let ok = &tokenize(&InputText("MUST".to_string()), SentenceLimit(64)).unwrap()[0];
    assert_eq!(resolve(&tables, &ok.tokens).unwrap()[0].kind, MatchKind::Exact);
    let bad = &tokenize(&InputText("must".to_string()), SentenceLimit(64)).unwrap()[0];
    assert!(matches!(
        resolve(&tables, &bad.tokens).unwrap_err(),
        ReadError::UnknownWordCaseOnly { grammar_spelling, .. } if grammar_spelling.0 == "MUST"
    ));
    let unknown = &tokenize(&InputText("maybe".to_string()), SentenceLimit(64)).unwrap()[0];
    assert!(matches!(
        resolve(&tables, &unknown.tokens).unwrap_err(),
        ReadError::UnknownWord { token } if token.0 == "maybe"
    ));
}

fn read(
    grammar: &AbstractGrammar,
    compiled: &CompiledGrammar,
    text: &str,
    filter: bool,
) -> Result<Enumeration, ReadError> {
    let tables = build_tables(compiled);
    let sentence = &tokenize(&InputText(text.to_string()), SentenceLimit(64)).unwrap()[0];
    let resolved = resolve(&tables, &sentence.tokens)?;
    let chart = parse(&tables, &resolved, ItemLimit(50_000))?;
    let forest = build_forest(&tables, &chart, resolved.len())?;
    let type_filter = TypeFilter {
        signature: &grammar.signature,
        fuel: EvalFuel(4096),
        keep: RejectionLimit(8),
    };
    enumerate(
        &tables,
        &forest,
        TreeLimit(64),
        SplitLimit(256),
        EvalFuel(4096),
        if filter { Some(&type_filter) } else { None },
    )
}

fn fun(name: &str) -> Term {
    Term::Const(ConstName::Function(FunName(name.to_string())))
}

fn app(f: Term, x: Term) -> Term {
    Term::App {
        function: Box::new(f),
        argument: Box::new(x),
    }
}

#[test]
fn every_dog_barks_yields_two_candidates_and_the_filter_keeps_one() {
    let (grammar, compiled) = load(LOGIC, LOGIC_ENG);
    let raw = read(&grammar, &compiled, "every dog barks", false).unwrap();
    let body = app(fun("Barks"), Term::Var(DeBruijnIndex(0)));
    let lam = Term::Lam {
        binder: Binder(VarName("_".to_string())),
        body: Box::new(body),
    };
    let expected = |sort: &str| app(app(app(fun("Every"), fun(sort)), fun("dog")), lam.clone());
    assert_eq!(raw.terms.len(), 2, "{:#?}", raw.terms);
    assert!(raw.terms.contains(&expected("Animate")));
    assert!(raw.terms.contains(&expected("Thing")));
    assert!(!raw.truncated);

    let filtered = read(&grammar, &compiled, "every dog barks", true).unwrap();
    assert_eq!(filtered.terms, vec![expected("Animate")]);
    assert_eq!(filtered.rejected_total, 1);
}

#[test]
fn every_rock_barks_parses_and_then_every_candidate_is_refused() {
    let (grammar, compiled) = load(LOGIC, LOGIC_ENG);
    let filtered = read(&grammar, &compiled, "every rock barks", true).unwrap();
    assert!(filtered.terms.is_empty());
    assert_eq!(filtered.rejected_total, 2);
    assert!(filtered
        .rejected
        .iter()
        .all(|r| matches!(r.error, gf_mltt::error::TypeError::Mismatch { .. })));
}

#[test]
fn number_agreement_fails_before_the_type_checker_runs() {
    let (grammar, compiled) = load(LOGIC, LOGIC_ENG);
    let failure = read(&grammar, &compiled, "all dog barks", true).unwrap_err();
    assert!(matches!(failure, ReadError::NoParseAt { position, .. } if position.0 == 1), "{failure:?}");
    let ok = read(&grammar, &compiled, "all dogs bark", true).unwrap();
    assert_eq!(ok.terms.len(), 1);
}

#[test]
fn a_nullable_symbol_completed_late_is_still_seen() {
    // Two nullable symbols in a row before a word. Naive completion misses
    // the second one because it completes before the item that wants it exists.
    let abstract_text = "abstract N = { flags startcat = S ; cat S ; cat E ; fun e1, e2 : E ; fun w : E -> E -> S ; }";
    let concrete = "concrete NEng of N = { lincat S = { s : Str } ; lincat E = { s : Str } ; lin e1 = { s = [] } ; lin e2 = { s = [] } ; lin w a b = { s = a.s ++ b.s ++ \"w\" } ; }";
    let (grammar, compiled) = load(abstract_text, concrete);
    let found = read(&grammar, &compiled, "w", false).unwrap();
    assert_eq!(found.terms.len(), 4, "two choices for each empty argument");
}

#[test]
fn the_filter_does_not_change_the_well_typed_set() {
    let (grammar, compiled) = load(LOGIC, LOGIC_ENG);
    for text in [
        "every dog barks",
        "john barks and fido sleeps",
        "every dog barks and john sleeps",
        "it is not the case that the rock falls",
        "some cat sleeps",
    ] {
        let raw = read(&grammar, &compiled, text, false).unwrap();
        let kept: Vec<Term> = raw
            .terms
            .into_iter()
            .filter(|t| gf_mltt::mltt::check::infer_closed(&grammar.signature, t, EvalFuel(4096)).is_ok())
            .collect();
        let filtered = read(&grammar, &compiled, text, true).unwrap();
        assert_eq!(filtered.terms, kept, "{text}");
        assert!(!filtered.terms.is_empty(), "{text} should have a reading");
    }
}
