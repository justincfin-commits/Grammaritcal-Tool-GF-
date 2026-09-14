//! Tests for the grammar source lexer and parser.

use gf_mltt::error::{LexError, ParseError};
use gf_mltt::gf::lexer::lex;
use gf_mltt::gf::parser::{parse_abstract, parse_concrete};
use gf_mltt::newtype::{CatName, SourceText};

const LOGIC: &str = include_str!("../grammars/Logic.gf");
const LOGIC_ENG: &str = include_str!("../grammars/LogicEng.gf");

fn source(text: &str) -> SourceText {
    SourceText(text.to_string())
}

#[test]
fn the_parser_reads_the_abstract_grammar() {
    let tokens = lex(&source(LOGIC)).unwrap();
    let module = parse_abstract(&tokens).unwrap();
    assert_eq!(module.name.0, "Logic");
    assert_eq!(module.start, Some(CatName("Prop".to_string())));
    assert_eq!(module.cats.len(), 4);
    let names: Vec<String> = module.funs.iter().map(|f| f.name.0.clone()).collect();
    assert!(names.contains(&"Every".to_string()));
    assert!(names.contains(&"cat_".to_string()));
    assert_eq!(names.len(), 18);
}

#[test]
fn the_parser_reads_the_concrete_grammar() {
    let tokens = lex(&source(LOGIC_ENG)).unwrap();
    let module = parse_concrete(&tokens).unwrap();
    assert_eq!(module.name.0, "LogicEng");
    assert_eq!(module.abstract_name.0, "Logic");
    assert_eq!(module.params.len(), 1);
    assert_eq!(module.lincats.len(), 4);
    assert_eq!(module.opers.len(), 2);
    let rules: Vec<String> = module.lins.iter().map(|l| l.fun.0.clone()).collect();
    assert!(rules.contains(&"Every".to_string()));
    assert_eq!(rules.len(), 18);
}

#[test]
fn the_lexer_drops_both_kinds_of_comment() {
    let text = source("abstract A = { -- a line comment\n {- a block comment -} cat C ; }");
    let tokens = lex(&text).unwrap();
    let module = parse_abstract(&tokens).unwrap();
    assert_eq!(module.cats.len(), 1);
}

#[test]
fn the_lexer_names_the_place_of_an_unknown_character() {
    let failure = lex(&source("abstract A = { cat C # ; }")).unwrap_err();
    match failure {
        LexError::UnexpectedCharacter { pos, found } => {
            assert_eq!(found, '#');
            assert_eq!(pos.line.0, 1);
            assert_eq!(pos.column.0, 22);
        }
        other => panic!("wanted an unknown character, found {other:?}"),
    }
}

#[test]
fn the_parser_names_the_place_of_a_missing_semicolon() {
    let tokens = lex(&source("abstract A = {\n  cat C\n}")).unwrap();
    let failure = parse_abstract(&tokens).unwrap_err();
    match failure {
        ParseError::Expected { pos, .. } => {
            assert_eq!(pos.line.0, 3);
        }
        other => panic!("wanted a missing semicolon, found {other:?}"),
    }
}

#[test]
fn the_parser_reads_a_dependent_function_type() {
    let tokens = lex(&source(
        "abstract A = { cat S ; cat I (k : S) ; fun f : (k : S) -> I k -> S ; }",
    ))
    .unwrap();
    let module = parse_abstract(&tokens).unwrap();
    assert_eq!(module.funs.len(), 1);
    assert_eq!(module.cats[1].args.len(), 1);
}

#[test]
fn lowering_builds_a_signature_that_checks() {
    let tokens = lex(&source(LOGIC)).unwrap();
    let module = parse_abstract(&tokens).unwrap();
    let grammar = gf_mltt::gf::lower::lower_abstract(&module, None).unwrap();
    assert_eq!(grammar.start.0, "Prop");
    assert_eq!(grammar.signature.order.len(), 22);
    let failures = gf_mltt::mltt::signature::check_signature(
        &grammar.signature,
        gf_mltt::newtype::EvalFuel(4096),
    );
    assert_eq!(failures, vec![]);
}

#[test]
fn lowering_reports_a_name_the_grammar_never_declares() {
    let tokens = lex(&source("abstract A = { flags startcat = C ; cat C ; fun f : D ; }")).unwrap();
    let module = parse_abstract(&tokens).unwrap();
    let failures = gf_mltt::gf::lower::lower_abstract(&module, None).unwrap_err();
    assert_eq!(
        failures,
        vec![gf_mltt::error::LowerError::UnknownIdentifier {
            name: "D".to_string()
        }]
    );
}

#[test]
fn lowering_gives_a_category_a_dependent_type() {
    let tokens = lex(&source(LOGIC)).unwrap();
    let module = parse_abstract(&tokens).unwrap();
    let grammar = gf_mltt::gf::lower::lower_abstract(&module, None).unwrap();
    let name = gf_mltt::mltt::term::ConstName::Category(CatName("Ind".to_string()));
    let ty = grammar.signature.types.get(&name).unwrap();
    match ty {
        gf_mltt::mltt::term::Term::Pi { domain, codomain, .. } => {
            assert_eq!(
                domain.as_ref(),
                &gf_mltt::mltt::term::Term::Const(gf_mltt::mltt::term::ConstName::Category(
                    CatName("Sort".to_string())
                ))
            );
            assert_eq!(
                codomain.as_ref(),
                &gf_mltt::mltt::term::Term::Universe(gf_mltt::newtype::UniverseLevel(0))
            );
        }
        other => panic!("wanted a dependent function type, found {other:?}"),
    }
}

#[test]
fn a_row_after_an_exclamation_mark_may_project_a_field() {
    use gf_mltt::gf::ast::CExpr;
    use gf_mltt::newtype::FieldName;
    let tokens = lex(&source(
        "concrete B of A = { oper f x = t ! x.n ; oper g x = t ! a ! b ; }",
    ))
    .unwrap();
    let module = parse_concrete(&tokens).unwrap();
    let f = &module.opers[0].body;
    let g = &module.opers[1].body;
    let name = |s: &str| Box::new(CExpr::Name(s.to_string()));
    assert_eq!(
        f,
        &CExpr::Select(
            name("t"),
            Box::new(CExpr::Project(name("x"), FieldName("n".to_string())))
        ),
        "the row after ! must take its own projections"
    );
    assert_eq!(
        g,
        &CExpr::Select(Box::new(CExpr::Select(name("t"), name("a"))), name("b")),
        "two selections in a row must stay left to right"
    );
}
