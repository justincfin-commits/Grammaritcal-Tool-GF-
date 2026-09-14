//! Acceptance tests for the `DynamicControlPlane` grammar pair.
//!
//! The seven sentences come from the claims of WO 2025/075926, rewritten as
//! RFC 2119 requirements. Each must return exactly one reading whose type
//! names the modal strength. Three further sentences must parse and then
//! fail to type check, because a verb is applied to a noun of the wrong sort.

use gf_mltt::analyze::{analyze, AnalysisRequest, GrammarOutcome, SentenceResult, DEFAULT_LIMITS};
use gf_mltt::error::TypeError;
use gf_mltt::newtype::{InputText, SourceText};
use gf_mltt::render::{render_term, render_type, render_type_error};

const ABSTRACT: &str = include_str!("../grammars/DynamicControlPlane.gf");
const CONCRETE: &str = include_str!("../grammars/DynamicControlPlaneEng.gf");
const ACCEPTANCE: &str = include_str!("data/acceptance.txt");
const REJECTED: &str = include_str!("data/rejected.txt");

fn run(text: &str) -> Vec<SentenceResult> {
    let outcome = analyze(&AnalysisRequest {
        abstract_source: SourceText(ABSTRACT.to_string()),
        concrete_source: SourceText(CONCRETE.to_string()),
        input: InputText(text.to_string()),
        start: None,
        limits: DEFAULT_LIMITS,
    });
    match &outcome.grammar {
        GrammarOutcome::Loaded(summary) => {
            assert_eq!(summary.start.0, "Req");
            assert_eq!(summary.gaps, 0, "this grammar uses no quantifier gap");
        }
        GrammarOutcome::Failed(failures) => panic!("grammar failed: {failures:#?}"),
    }
    outcome.sentences.into_iter().map(|s| s.result).collect()
}

#[test]
fn the_seven_acceptance_sentences_each_have_exactly_one_reading() {
    let expected: Vec<(&str, &str)> = vec![
        (
            "Obligation MUST (Pred Actor (DetNP Actor theDet cloudPlatform) (Act Actor Resource determine_V2 (DetNP Resource theDet specificationTemplate)))",
            "Req MUST",
        ),
        (
            "Obligation MUST (Pred Message (DetNP Message allDet apiCall) (BePassBy Message authenticated_VPass (DetNP Actor theDet provider)))",
            "Req MUST",
        ),
        (
            "Obligation SHOULD (Pred Resource (DetNP Resource theDet credential) (BePass Resource stored_VPass))",
            "Req SHOULD",
        ),
        (
            "Obligation MUST_NOT (Pred Actor (DetNP Actor theDet cloudPlatform) (Act Actor Actor redirect_V2 (DetNP Actor theDet (OfMod Actor Resource developer (DetNP Resource theDet application)))))",
            "Req MUST_NOT",
        ),
        (
            "Obligation MAY (Pred Resource (DetNP Resource anDet (OfMod Resource Resource account (DetNP Resource theDet application))) (BePass Resource configured_VPass))",
            "Req MAY",
        ),
        (
            "Obligation SHOULD_NOT (Pred Actor (DetNP Actor theDet provider) (Act Actor Resource store_V2 (DetNP Resource theDet credential)))",
            "Req SHOULD_NOT",
        ),
        (
            "Obligation MUST (Pred Resource (DetNP Resource everyDet specificationTemplate) (BeAP Resource common_AP))",
            "Req MUST",
        ),
    ];
    let results = run(ACCEPTANCE);
    assert_eq!(results.len(), 7);
    for (position, (result, (tree, ty))) in results.iter().zip(expected).enumerate() {
        match result {
            SentenceResult::Readings { accepted, .. } => {
                let shown: Vec<(String, String)> = accepted
                    .iter()
                    .map(|r| (render_term(&r.tree), render_type(&r.ty)))
                    .collect();
                assert_eq!(
                    shown,
                    vec![(tree.to_string(), ty.to_string())],
                    "sentence {}",
                    position + 1
                );
            }
            SentenceResult::Failed(error) => panic!("sentence {}: {error:?}", position + 1),
        }
    }
}

#[test]
fn a_message_verb_applied_to_an_actor_parses_and_fails_to_type_check() {
    let results = run(REJECTED);
    assert_eq!(results.len(), 3);
    let wanted = [
        // authenticate_V2 : V2 Actor Message, but "the provider" is an Actor.
        "expected type NP Message, found type NP Actor",
        // determine_V2 wants an Actor subject, but "the credential" is a Resource.
        "expected type NP Resource, found type NP Actor",
        // common_AP : AP Resource, but "the API call" is a Message.
        "expected type VP Message, found type VP Resource",
    ];
    for (position, (result, wanted)) in results.iter().zip(wanted).enumerate() {
        match result {
            SentenceResult::Readings {
                accepted,
                rejected,
                rejected_total,
                ..
            } => {
                assert!(accepted.is_empty(), "sentence {}", position + 1);
                assert!(*rejected_total > 0);
                assert!(rejected.iter().all(|r| matches!(r.error, TypeError::Mismatch { .. })));
                let messages: Vec<String> =
                    rejected.iter().map(|r| render_type_error(&r.error)).collect();
                assert!(
                    messages.iter().any(|m| m == wanted),
                    "sentence {}: wanted {wanted:?} among {messages:#?}",
                    position + 1
                );
            }
            SentenceResult::Failed(error) => panic!("sentence {}: {error:?}", position + 1),
        }
    }
}

#[test]
fn lowercase_must_is_refused_with_the_grammar_spelling() {
    let results = run("The credential must be stored.");
    match &results[0] {
        SentenceResult::Failed(gf_mltt::error::ReadError::UnknownWordCaseOnly {
            token,
            grammar_spelling,
        }) => {
            assert_eq!(token.0, "must");
            assert_eq!(grammar_spelling.0, "MUST");
        }
        other => panic!("{other:?}"),
    }
}
