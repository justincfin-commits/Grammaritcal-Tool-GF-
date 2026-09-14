//! Input text data: sentences and their words.
//!
//! This module holds data only. `crate::parse::tokenize` builds these values
//! and `crate::parse::earley` reads them.

#![allow(missing_docs)]

use crate::newtype::{SentenceIndex, TerminalId, TextOffset, Token};

/// A stretch of the input text, counted in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: TextOffset,
    pub end: TextOffset,
}

/// One word of running text, before the parser looks it up in the grammar.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputToken {
    /// The word as the input writes it, with punctuation stripped.
    pub surface: Token,
    /// The same word with every ASCII capital turned small.
    pub folded: Token,
    /// Where the word stands in the input text.
    pub span: Span,
}

/// Why a sentence ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SentenceEnd {
    /// A full stop, a question mark, or an exclamation mark closed it.
    Terminator,
    /// A blank line closed it.
    Break,
    /// The text ran out.
    EndOfText,
}

/// One sentence of the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentence {
    pub index: SentenceIndex,
    pub tokens: Vec<InputToken>,
    pub end: SentenceEnd,
    pub span: Span,
}

/// How a word matched a grammar word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchKind {
    /// The grammar holds the word exactly as the input writes it.
    Exact,
    /// The grammar holds the word only with every capital turned small.
    Folded,
}

/// One word after the parser found it in the grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResolvedToken {
    pub terminal: TerminalId,
    pub kind: MatchKind,
    pub span: Span,
}
