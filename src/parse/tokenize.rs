//! Splits input text into sentences and tokens.
//!
//! This module holds behavior only. The tokenizer knows nothing about the
//! grammar. It is one pass over the characters, with no recursion.
//!
//! The rule, in full. Words are runs of characters separated by whitespace.
//! A word drops opening quotation marks and brackets from its front, and
//! drops closing punctuation from its back. A full stop, question mark, or
//! exclamation mark at the back of a word ends the sentence. A blank line
//! ends the sentence. A sentence with no words is never emitted.
//!
//! What the rule does not do: `e.g.` ends a sentence, a missing space is not
//! repaired, and a semicolon does not end a sentence.

use crate::error::ReadError;
use crate::newtype::{InputText, SentenceIndex, SentenceLimit, TextOffset, Token};
use crate::parse::input::{InputToken, Sentence, SentenceEnd, Span};

/// Characters that end a sentence when they sit at the end of a word.
pub const TERMINATORS: [char; 3] = ['.', '?', '!'];

/// Characters the tool drops from the end of a word.
pub const CLOSERS: [char; 7] = [',', ';', ':', '"', ')', ']', '}'];

/// Characters the tool drops from the start of a word.
pub const OPENERS: [char; 4] = ['"', '(', '[', '{'];

/// A sentence still being read.
struct Open {
    tokens: Vec<InputToken>,
    start: TextOffset,
}

/// Closes the open sentence, when it holds any word, and starts a fresh one.
fn close(
    open: &mut Open,
    end: SentenceEnd,
    at: TextOffset,
    out: &mut Vec<Sentence>,
    limit: SentenceLimit,
) -> Result<(), ReadError> {
    if open.tokens.is_empty() {
        open.start = at;
        return Ok(());
    }
    let index = SentenceIndex(out.len());
    if open.tokens.len() > limit.0 {
        return Err(ReadError::SentenceTooLong {
            sentence: index,
            length: open.tokens.len(),
            limit,
        });
    }
    let tokens = std::mem::take(&mut open.tokens);
    out.push(Sentence {
        index,
        tokens,
        end,
        span: Span {
            start: open.start,
            end: at,
        },
    });
    open.start = at;
    Ok(())
}

/// Splits running text into sentences and words.
pub fn tokenize(text: &InputText, limit: SentenceLimit) -> Result<Vec<Sentence>, ReadError> {
    let chars: Vec<char> = text.0.chars().collect();
    let mut out: Vec<Sentence> = Vec::new();
    let mut open = Open {
        tokens: Vec::new(),
        start: TextOffset(0),
    };
    let mut at: usize = 0;
    while at < chars.len() {
        // Skip whitespace and count newlines. Two newlines close the sentence.
        let mut newlines: usize = 0;
        while let Some(&c) = chars.get(at) {
            if !c.is_whitespace() {
                break;
            }
            if c == '\n' {
                newlines = newlines.saturating_add(1);
            }
            at = at.saturating_add(1);
        }
        if newlines >= 2 {
            close(&mut open, SentenceEnd::Break, TextOffset(at), &mut out, limit)?;
        }
        // Read one run of non-whitespace characters.
        let run_start = at;
        while let Some(&c) = chars.get(at) {
            if c.is_whitespace() {
                break;
            }
            at = at.saturating_add(1);
        }
        let run_end = at;
        if run_start == run_end {
            continue;
        }
        // Strip openers from the front.
        let mut lo = run_start;
        while lo < run_end {
            match chars.get(lo) {
                Some(c) if OPENERS.contains(c) => lo = lo.saturating_add(1),
                _ => break,
            }
        }
        // Strip terminators and closers from the back, remembering a terminator.
        let mut hi = run_end;
        let mut terminated = false;
        while hi > lo {
            let previous = hi.saturating_sub(1);
            match chars.get(previous) {
                Some(c) if TERMINATORS.contains(c) => {
                    terminated = true;
                    hi = previous;
                }
                Some(c) if CLOSERS.contains(c) => hi = previous,
                _ => break,
            }
        }
        if lo < hi {
            let surface: String = chars.get(lo..hi).unwrap_or(&[]).iter().collect();
            let folded = surface.to_ascii_lowercase();
            if open.tokens.is_empty() {
                open.start = TextOffset(lo);
            }
            open.tokens.push(InputToken {
                surface: Token(surface),
                folded: Token(folded),
                span: Span {
                    start: TextOffset(lo),
                    end: TextOffset(hi),
                },
            });
        }
        if terminated {
            close(
                &mut open,
                SentenceEnd::Terminator,
                TextOffset(run_end),
                &mut out,
                limit,
            )?;
        }
    }
    close(
        &mut open,
        SentenceEnd::EndOfText,
        TextOffset(chars.len()),
        &mut out,
        limit,
    )?;
    if out.is_empty() {
        return Err(ReadError::EmptyInput);
    }
    Ok(out)
}
