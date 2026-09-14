//! The purity claim, checked by running the whole tool twice.

use gf_mltt::analyze::{analyze, AnalysisRequest, GrammarOutcome, SentenceResult, DEFAULT_LIMITS};
use gf_mltt::dcbor::decode;
use gf_mltt::newtype::{InputText, SourceText};
use gf_mltt::render::{render_dcbor, render_diagnostic, render_text};

const LOGIC: &str = include_str!("../grammars/Logic.gf");
const LOGIC_ENG: &str = include_str!("../grammars/LogicEng.gf");

fn request(text: &str) -> AnalysisRequest {
    AnalysisRequest {
        abstract_source: SourceText(LOGIC.to_string()),
        concrete_source: SourceText(LOGIC_ENG.to_string()),
        input: InputText(text.to_string()),
        start: None,
        limits: DEFAULT_LIMITS,
    }
}

#[test]
fn two_calls_on_one_request_agree_byte_for_byte() {
    let req = request(
        "Every dog barks. Every rock barks. All dogs bark and John sleeps. Fido is happy. Some cat sleeps or the rock falls. Zebra.",
    );
    let first = analyze(&req);
    let second = analyze(&req);
    assert_eq!(first, second, "the outcomes differ as data");
    assert_eq!(render_text(&first), render_text(&second), "the text reports differ");
    assert_eq!(render_diagnostic(&first), render_diagnostic(&second));
    let bytes_first = render_dcbor(&first);
    let bytes_second = render_dcbor(&second);
    assert_eq!(bytes_first, bytes_second, "the dCBOR bytes differ");
    assert!(!bytes_first.is_empty());
    // The bytes decode, which checks that the encoder obeyed every rule the
    // decoder enforces: shortest heads, sorted keys, definite lengths.
    decode(&bytes_first).unwrap();
}

#[test]
fn the_report_names_the_sentence_that_failed_to_type_check() {
    let outcome = analyze(&request("Every rock barks."));
    assert!(matches!(outcome.grammar, GrammarOutcome::Loaded(_)));
    let sentence = &outcome.sentences[0];
    assert_eq!(
        sentence.words.iter().map(|w| w.0.as_str()).collect::<Vec<_>>(),
        vec!["Every", "rock", "barks"]
    );
    match &sentence.result {
        SentenceResult::Readings {
            accepted,
            rejected,
            rejected_total,
            ..
        } => {
            assert!(accepted.is_empty());
            assert_eq!(*rejected_total, 2);
            assert_eq!(rejected.len(), 2);
        }
        other => panic!("wanted readings, found {other:?}"),
    }
    let text = render_text(&outcome);
    assert!(text.contains("sentence 1: Every rock barks"));
    assert!(text.contains("every one failed to type check"));
    assert!(text.contains("expected type Ind Animate, found type Ind Thing"));
}

#[test]
fn a_grammar_that_fails_to_load_reports_every_problem_and_reads_nothing() {
    let broken = LOGIC.replace("fun rock : CN Thing ;", "fun rock : CN Thinng ;");
    let outcome = analyze(&AnalysisRequest {
        abstract_source: SourceText(broken),
        concrete_source: SourceText(LOGIC_ENG.to_string()),
        input: InputText("Every dog barks.".to_string()),
        start: None,
        limits: DEFAULT_LIMITS,
    });
    match &outcome.grammar {
        GrammarOutcome::Failed(failures) => assert_eq!(failures.len(), 1),
        other => panic!("wanted a failure, found {other:?}"),
    }
    assert!(outcome.sentences.is_empty());
    assert!(render_text(&outcome).contains("Thinng"));
}
