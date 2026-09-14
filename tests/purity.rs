//! The purity claim, checked by reading the library source.
//!
//! The library must contain no `unsafe`, no file or socket or clock access,
//! no `unwrap`, `expect`, or `panic!`, and no hash map whose iteration order
//! could reach the output. This test reads every file under `src/` except
//! the binary and looks for each forbidden token. A test may read files; the
//! library may not.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: [&str; 13] = [
    "unsafe {",
    "unsafe fn",
    "std::fs",
    "std::io",
    "std::time",
    "std::net",
    ".unwrap()",
    ".expect(",
    "panic!(",
    "unreachable!(",
    "todo!(",
    "HashMap",
    "HashSet",
];

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().map(|n| n == "bin").unwrap_or(false) {
                continue;
            }
            collect(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

#[test]
fn the_library_holds_no_forbidden_token() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<PathBuf> = Vec::new();
    collect(&root, &mut files);
    files.sort();
    assert!(files.len() >= 20, "expected the whole library, found {} files", files.len());
    let mut hits: Vec<String> = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap();
        for (number, line) in text.lines().enumerate() {
            // Documentation and comments may mention a token by name.
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            for token in FORBIDDEN {
                if code.contains(token) {
                    hits.push(format!("{}:{}: {}", file.display(), number + 1, line.trim()));
                }
            }
        }
    }
    assert!(hits.is_empty(), "forbidden tokens in the library:\n{}", hits.join("\n"));
}

#[test]
fn the_crate_has_no_dependencies() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let after = manifest.split("[dependencies]").nth(1).unwrap();
    let section = after.split('[').next().unwrap();
    let entries: Vec<&str> = section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    assert!(entries.is_empty(), "dependencies found: {entries:?}");
}
