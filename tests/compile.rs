//! Tests for the production compiler and the gap threading.

use gf_mltt::compile::grammar::{Build, CompileLimits, CompiledGrammar, Symbol};
use gf_mltt::compile::productions::compile;
use gf_mltt::error::{CompileError, ConcreteError, UnsupportedReason};
use gf_mltt::gf::lexer::lex;
use gf_mltt::gf::lower::{lower_abstract, AbstractGrammar};
use gf_mltt::gf::parser::{parse_abstract, parse_concrete};
use gf_mltt::newtype::{
    ArgIndex, CatName, EvalFuel, FunName, GapLimit, ProductionLimit, SourceText, StateLimit,
};

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

fn load(abstract_text: &str, concrete_text: &str) -> (AbstractGrammar, gf_mltt::gf::ast::ConcreteModule) {
    let abstract_module = parse_abstract(&lex(&SourceText(abstract_text.to_string())).unwrap()).unwrap();
    let concrete = parse_concrete(&lex(&SourceText(concrete_text.to_string())).unwrap()).unwrap();
    let grammar = lower_abstract(&abstract_module, None).unwrap();
    (grammar, concrete)
}

fn compile_text(abstract_text: &str, concrete_text: &str) -> Result<CompiledGrammar, Vec<CompileError>> {
    let (grammar, concrete) = load(abstract_text, concrete_text);
    compile(&grammar, &concrete, limits())
}

fn logic() -> CompiledGrammar {
    match compile_text(LOGIC, LOGIC_ENG) {
        Ok(grammar) => grammar,
        Err(failures) => panic!("LogicEng failed to compile: {failures:#?}"),
    }
}

fn fun(name: &str) -> FunName {
    FunName(name.to_string())
}

#[test]
fn logic_compiles_to_the_hand_counted_grammar() {
    let grammar = logic();
    assert_eq!(grammar.base_count, 25, "base productions before gap threading");
    assert_eq!(grammar.gap_count, 2, "one gap per number");
    assert_eq!(grammar.productions.len(), 41, "live productions after pruning");
    assert_eq!(grammar.start.len(), 1, "Prop has one state and one string field");
    let every: Vec<_> = grammar
        .productions
        .iter()
        .filter(|p| p.build == Build::Apply(fun("Every")))
        .collect();
    assert_eq!(every.len(), 1, "Every has one base production and no live gapped copy");
    let gap_sites = grammar
        .productions
        .iter()
        .filter(|p| p.build == Build::GapSite)
        .count();
    assert_eq!(gap_sites, 2);
}

#[test]
fn the_every_production_puts_the_gap_on_its_third_argument() {
    let grammar = logic();
    let every = grammar
        .productions
        .iter()
        .find(|p| p.build == Build::Apply(fun("Every")))
        .unwrap();
    let children: Vec<(ArgIndex, bool)> = every
        .rhs
        .iter()
        .filter_map(|symbol| match symbol {
            Symbol::Child { arg, nonterminal } => {
                let nt = &grammar.nonterminals[nonterminal.0];
                Some((*arg, nt.gap.is_some()))
            }
            Symbol::Word(_) => None,
        })
        .collect();
    assert_eq!(
        children,
        vec![(ArgIndex(0), false), (ArgIndex(1), false), (ArgIndex(2), true)]
    );
    let words: Vec<String> = every
        .rhs
        .iter()
        .filter_map(|symbol| match symbol {
            Symbol::Word(token) => Some(token.0.clone()),
            Symbol::Child { .. } => None,
        })
        .collect();
    assert_eq!(words, vec!["every".to_string()]);
}

#[test]
fn sort_derives_the_empty_string_twice() {
    let grammar = logic();
    let sort = CatName("Sort".to_string());
    let empties: Vec<&Build> = grammar
        .productions
        .iter()
        .filter(|p| grammar.nonterminals[p.lhs.0].cat == sort && p.rhs.is_empty())
        .map(|p| &p.build)
        .collect();
    assert_eq!(empties, vec![&Build::Apply(fun("Animate")), &Build::Apply(fun("Thing"))]);
    let sort_id = grammar
        .nonterminals
        .iter()
        .position(|nt| nt.cat == sort && nt.gap.is_none())
        .unwrap();
    assert!(grammar.nullable[sort_id], "Sort must be nullable");
}

const TINY_ABS: &str = "abstract T = { flags startcat = S ; cat S ; cat N ; fun n : N ; fun f : N -> S ; }";

#[test]
fn a_repeated_argument_is_refused() {
    let concrete = "concrete TEng of T = { lincat S = { s : Str } ; lincat N = { s : Str } ; lin n = { s = \"n\" } ; lin f x = { s = x.s ++ x.s } ; }";
    let failures = compile_text(TINY_ABS, concrete).unwrap_err();
    assert!(
        matches!(&failures[..], [CompileError::ArgumentRepeated { fun, arg, .. }] if fun.0 == "f" && arg.0 == 0),
        "{failures:?}"
    );
}

#[test]
fn a_dropped_argument_is_refused() {
    let concrete = "concrete TEng of T = { lincat S = { s : Str } ; lincat N = { s : Str } ; lin n = { s = \"n\" } ; lin f x = { s = \"only words\" } ; }";
    let failures = compile_text(TINY_ABS, concrete).unwrap_err();
    assert!(
        matches!(&failures[..], [CompileError::ArgumentDropped { fun, arg, .. }] if fun.0 == "f" && arg.0 == 0),
        "{failures:?}"
    );
}

#[test]
fn a_missing_rule_and_a_missing_lincat_are_named() {
    let concrete = "concrete TEng of T = { lincat S = { s : Str } ; lin n = { s = \"n\" } ; }";
    let failures = compile_text(TINY_ABS, concrete).unwrap_err();
    assert!(failures.contains(&CompileError::MissingLinCat { cat: CatName("N".to_string()) }));
    assert!(failures.contains(&CompileError::MissingLin { fun: fun("f") }));
}

#[test]
fn an_arity_mismatch_is_named() {
    let concrete = "concrete TEng of T = { lincat S = { s : Str } ; lincat N = { s : Str } ; lin n = { s = \"n\" } ; lin f x y = { s = x.s } ; }";
    let failures = compile_text(TINY_ABS, concrete).unwrap_err();
    assert_eq!(
        failures,
        vec![CompileError::LinArityMismatch { fun: fun("f"), expected: 1, found: 2 }]
    );
}

const HO_ABS: &str = "abstract H = { flags startcat = P ; cat P ; cat I ; fun i : I ; fun q : (I -> P) -> P ; fun v : I -> P ; }";

#[test]
fn a_gap_record_that_prints_words_is_refused_and_names_the_rule() {
    let concrete = "concrete HEng of H = { lincat P = { s : Str } ; lincat I = { s : Str } ; lin i = { s = \"i\" } ; lin v x = { s = x.s ++ \"v\" } ; lin q f = { s = \"q\" ++ (f { s = \"oops\" }).s } ; }";
    let failures = compile_text(HO_ABS, concrete).unwrap_err();
    assert_eq!(
        failures,
        vec![CompileError::Evaluation {
            fun: fun("q"),
            error: ConcreteError::GapNotEmpty { arg: ArgIndex(0) },
        }]
    );
}

#[test]
fn a_binder_that_is_never_applied_is_named() {
    let concrete = "concrete HEng of H = { lincat P = { s : Str } ; lincat I = { s : Str } ; lin i = { s = \"i\" } ; lin v x = { s = x.s ++ \"v\" } ; lin q f = { s = f } ; }";
    let failures = compile_text(HO_ABS, concrete).unwrap_err();
    assert_eq!(
        failures,
        vec![CompileError::Evaluation {
            fun: fun("q"),
            error: ConcreteError::BinderNotApplied { arg: ArgIndex(0) },
        }]
    );
}

#[test]
fn a_well_formed_quantifier_threads_its_gap() {
    let concrete = "concrete HEng of H = { lincat P = { s : Str } ; lincat I = { s : Str } ; lin i = { s = \"i\" } ; lin v x = { s = x.s ++ \"v\" } ; lin q f = { s = \"q\" ++ (f { s = [] }).s } ; }";
    let grammar = compile_text(HO_ABS, concrete).unwrap();
    assert_eq!(grammar.gap_count, 1);
    let sites = grammar.productions.iter().filter(|p| p.build == Build::GapSite).count();
    assert_eq!(sites, 1);
}

#[test]
fn each_unsupported_argument_type_has_a_reason() {
    let cases: Vec<(&str, UnsupportedReason)> = vec![
        ("(I -> P) -> P", UnsupportedReason::NestedFunction),
        ("Type -> P", UnsupportedReason::Universe),
        ("(x : I) ** I", UnsupportedReason::NotSupportedByParser),
        ("i", UnsupportedReason::NotACategory),
    ];
    for (domain, reason) in cases {
        let abstract_text = format!(
            "abstract H = {{ flags startcat = P ; cat P ; cat I ; fun i : I ; fun q : ({domain}) -> P ; }}"
        );
        let concrete = "concrete HEng of H = { lincat P = { s : Str } ; lincat I = { s : Str } ; lin i = { s = \"i\" } ; lin q f = { s = \"q\" } ; }";
        let failures = compile_text(&abstract_text, concrete).unwrap_err();
        assert_eq!(
            failures,
            vec![CompileError::UnsupportedArgumentType {
                fun: fun("q"),
                arg: ArgIndex(0),
                reason: reason.clone(),
            }],
            "domain {domain}"
        );
    }
}

#[test]
fn a_unit_cycle_is_refused() {
    let abstract_text = "abstract C = { flags startcat = S ; cat S ; fun w : S ; fun id_ : S -> S ; }";
    let concrete = "concrete CEng of C = { lincat S = { s : Str } ; lin w = { s = \"w\" } ; lin id_ x = { s = x.s } ; }";
    let failures = compile_text(abstract_text, concrete).unwrap_err();
    assert!(matches!(&failures[..], [CompileError::UnitCycle { cat, .. }] if cat.0 == "S"), "{failures:?}");
}

#[test]
fn a_rule_state_explosion_is_refused_before_enumeration() {
    let (grammar, concrete) = load(LOGIC, LOGIC_ENG);
    let mut small = limits();
    small.states_per_rule = StateLimit(1);
    let failures = compile(&grammar, &concrete, small).unwrap_err();
    assert!(
        failures.iter().any(|f| matches!(f, CompileError::RuleStateExplosion { fun, count: 2, .. } if fun.0 == "Barks")),
        "{failures:?}"
    );
}
