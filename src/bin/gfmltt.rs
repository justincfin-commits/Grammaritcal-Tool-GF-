//! The command line program. This is the only module that reads files or
//! prints. It reads the three inputs, calls `analyze` exactly once, and
//! writes what the renderers return.
//!
//! Usage:
//!
//! ```text
//! gfmltt --abstract A.gf --concrete AEng.gf (--text "..." | --file in.txt)
//!        [--start Cat] [--dcbor out.cbor] [--diag out.diag] [--vectors DIR]
//! ```
//!
//! Exit code 0 when every sentence has at least one reading, 1 otherwise,
//! and 2 on a usage error.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gf_mltt::analyze::{analyze, AnalysisRequest, GrammarOutcome, SentenceResult, DEFAULT_LIMITS};
use gf_mltt::newtype::{CatName, InputText, SourceText};
use gf_mltt::render::{
    render_dcbor, render_diagnostic, render_sentence_dcbor, render_sentence_diagnostic, render_text,
};

/// What the command line asked for.
struct Options {
    abstract_path: PathBuf,
    concrete_path: PathBuf,
    text: Option<String>,
    file: Option<PathBuf>,
    start: Option<CatName>,
    dcbor: Option<PathBuf>,
    diag: Option<PathBuf>,
    vectors: Option<PathBuf>,
}

const USAGE: &str = "usage: gfmltt --abstract A.gf --concrete AEng.gf (--text \"...\" | --file in.txt) [--start Cat] [--dcbor out.cbor] [--diag out.diag] [--vectors DIR]";

/// Reads the command line. Every flag takes one value.
fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut abstract_path: Option<PathBuf> = None;
    let mut concrete_path: Option<PathBuf> = None;
    let mut text: Option<String> = None;
    let mut file: Option<PathBuf> = None;
    let mut start: Option<CatName> = None;
    let mut dcbor: Option<PathBuf> = None;
    let mut diag: Option<PathBuf> = None;
    let mut vectors: Option<PathBuf> = None;
    let mut at = 0;
    while let Some(flag) = args.get(at) {
        let value = match args.get(at + 1) {
            Some(value) => value.clone(),
            None => return Err(format!("{flag} needs a value\n{USAGE}")),
        };
        match flag.as_str() {
            "--abstract" => abstract_path = Some(PathBuf::from(&value)),
            "--concrete" => concrete_path = Some(PathBuf::from(&value)),
            "--text" => text = Some(value),
            "--file" => file = Some(PathBuf::from(&value)),
            "--start" => start = Some(CatName(value)),
            "--dcbor" => dcbor = Some(PathBuf::from(&value)),
            "--diag" => diag = Some(PathBuf::from(&value)),
            "--vectors" => vectors = Some(PathBuf::from(&value)),
            other => return Err(format!("unknown flag {other}\n{USAGE}")),
        }
        at += 2;
    }
    let abstract_path = abstract_path.ok_or_else(|| format!("--abstract is required\n{USAGE}"))?;
    let concrete_path = concrete_path.ok_or_else(|| format!("--concrete is required\n{USAGE}"))?;
    if text.is_none() && file.is_none() {
        return Err(format!("one of --text or --file is required\n{USAGE}"));
    }
    Ok(Options {
        abstract_path,
        concrete_path,
        text,
        file,
        start,
        dcbor,
        diag,
        vectors,
    })
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
    }
    fs::write(path, bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn run(options: &Options) -> Result<bool, String> {
    let abstract_source = SourceText(read_text(&options.abstract_path)?);
    let concrete_source = SourceText(read_text(&options.concrete_path)?);
    let input = match (&options.text, &options.file) {
        (Some(text), _) => text.clone(),
        (None, Some(path)) => read_text(path)?,
        (None, None) => return Err(USAGE.to_string()),
    };
    let request = AnalysisRequest {
        abstract_source,
        concrete_source,
        input: InputText(input),
        start: options.start.clone(),
        limits: DEFAULT_LIMITS,
    };
    let outcome = analyze(&request);

    let report = render_text(&outcome);
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(report.as_bytes())
        .map_err(|error| format!("cannot write to standard output: {error}"))?;

    if let Some(path) = &options.dcbor {
        write_bytes(path, &render_dcbor(&outcome))?;
    }
    if let Some(path) = &options.diag {
        write_bytes(path, render_diagnostic(&outcome).as_bytes())?;
    }
    if let Some(dir) = &options.vectors {
        fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        for sentence in &outcome.sentences {
            let stem = format!("{:03}", sentence.index.0 + 1);
            write_bytes(&dir.join(format!("{stem}.cbor")), &render_sentence_dcbor(sentence))?;
            write_bytes(
                &dir.join(format!("{stem}.diag")),
                render_sentence_diagnostic(sentence).as_bytes(),
            )?;
        }
    }

    let grammar_ok = matches!(outcome.grammar, GrammarOutcome::Loaded(_));
    let every_sentence_read = outcome.input_failure.is_none()
        && outcome.sentences.iter().all(|sentence| {
            matches!(&sentence.result, SentenceResult::Readings { accepted, .. } if !accepted.is_empty())
        });
    Ok(grammar_ok && every_sentence_read)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_options(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match run(&options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}
