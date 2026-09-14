//! Acceptance tests for the `Logic` / `LogicEng` grammar pair.

use gf_mltt::analyze::{analyze, AnalysisRequest, SentenceResult, DEFAULT_LIMITS};
use gf_mltt::error::{ReadError, TypeError};
use gf_mltt::newtype::{InputText, SourceText};
use gf_mltt::render::{render_term, render_type};

const LOGIC: &str = include_str!("../grammars/Logic.gf");
const LOGIC_ENG: &str = include_str!("../grammars/LogicEng.gf");

fn read(text: &str) -> SentenceResult {
    let outcome = analyze(&AnalysisRequest {
        abstract_source: SourceText(LOGIC.to_string()),
        concrete_source: SourceText(LOGIC_ENG.to_string()),
        input: InputText(text.to_string()),
        start: None,
        limits: DEFAULT_LIMITS,
    });
    outcome.sentences.into_iter().next().unwrap().result
}

/// Every reading as `tree : type`, sorted.
fn readings(text: &str) -> Vec<String> {
    match read(text) {
        SentenceResult::Readings { accepted, .. } => accepted
            .iter()
            .map(|r| format!("{} : {}", render_term(&r.tree), render_type(&r.ty)))
            .collect(),
        SentenceResult::Failed(error) => panic!("{text}: {error:?}"),
    }
}

#[test]
fn every_dog_barks_has_one_reading() {
    assert_eq!(
        readings("Every dog barks."),
        vec!["Every Animate dog (\\x -> Barks x) : Prop"]
    );
}

#[test]
fn every_rock_barks_parses_and_fails_to_type_check() {
    match read("Every rock barks.") {
        SentenceResult::Readings {
            accepted,
            rejected,
            rejected_total,
            ..
        } => {
            assert!(accepted.is_empty());
            assert_eq!(rejected_total, 2);
            assert!(rejected.iter().all(|r| matches!(r.error, TypeError::Mismatch { .. })));
            let trees: Vec<String> = rejected.iter().map(|r| render_term(&r.tree)).collect();
            assert_eq!(
                trees,
                vec![
                    "Every Animate rock (\\x -> Barks x)",
                    "Every Thing rock (\\x -> Barks x)"
                ]
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn plural_quantification_and_agreement() {
    assert_eq!(readings("All dogs bark."), vec!["All Animate dog (\\x -> Barks x) : Prop"]);
    assert_eq!(
        readings("Some cat sleeps."),
        vec!["Some_ Animate cat_ (\\x -> Sleeps x) : Prop"]
    );
    match read("All dog barks.") {
        SentenceResult::Failed(ReadError::NoParseAt { position, .. }) => assert_eq!(position.0, 1),
        other => panic!("{other:?}"),
    }
}

#[test]
fn proper_nouns_connectives_and_negation() {
    assert_eq!(
        readings("John barks and Fido sleeps."),
        vec!["And (Barks John) (Sleeps Fido) : Prop"]
    );
    assert_eq!(
        readings("It is not the case that the rock falls."),
        vec!["Not (Falls Thing TheRock) : Prop"]
    );
    assert_eq!(readings("Fido is happy."), vec!["Happy Fido : Prop"]);
}

#[test]
fn a_quantifier_may_scope_over_a_conjunction() {
    assert_eq!(
        readings("Every dog barks and John sleeps."),
        vec![
            "And (Every Animate dog (\\x -> Barks x)) (Sleeps John) : Prop",
            "Every Animate dog (\\x -> And (Barks x) (Sleeps John)) : Prop",
        ]
    );
}

#[test]
fn across_the_board_extraction_is_outside_the_fragment() {
    // Both conjuncts would need the same gap. The design allows one gap per
    // phrase, so this sentence does not parse. Stated in the README.
    assert!(matches!(read("Every dog barks and sleeps."), SentenceResult::Failed(_)));
}

#[test]
fn an_unknown_word_is_named() {
    match read("Every zebra barks.") {
        SentenceResult::Failed(ReadError::UnknownWord { token }) => assert_eq!(token.0, "zebra"),
        other => panic!("{other:?}"),
    }
}
