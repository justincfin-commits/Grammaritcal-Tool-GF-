//! Splits a grammar source file into tokens.
//!
//! This module holds behavior only. The function `lex` takes the text of one
//! file and returns the token list, or the first failure it meets. The
//! function owns every value it touches and returns fresh data. It reads no
//! file and changes nothing outside itself.
//!
//! The lexer drops two kinds of comment. A line comment starts at `--` and
//! runs to the end of the line. A block comment starts at `{-` and ends at
//! `-}`. Block comments do not nest.
//!
//! A word list between quotation marks splits on spaces, so `"is a dog"`
//! carries three words. A quotation mark holds no escape character.

use crate::error::{LexError, SourcePos};
use crate::gf::token::{Lexeme, Tok};
use crate::newtype::{Column, Line, SourceText, UniverseLevel};

/// Where the lexer stands in the file.
#[derive(Clone, Copy, Debug)]
struct Cursor {
    at: usize,
    line: u32,
    column: u32,
}

impl Cursor {
    fn pos(self) -> SourcePos {
        SourcePos {
            line: Line(self.line),
            column: Column(self.column),
        }
    }
}

/// Moves the cursor past one character.
fn step(cursor: Cursor, character: char) -> Cursor {
    if character == '\n' {
        Cursor {
            at: cursor.at + 1,
            line: cursor.line + 1,
            column: 1,
        }
    } else {
        Cursor {
            at: cursor.at + 1,
            line: cursor.line,
            column: cursor.column + 1,
        }
    }
}

/// Reads the character at the cursor.
fn peek(text: &[char], cursor: Cursor) -> Option<char> {
    text.get(cursor.at).copied()
}

/// Reads the character one place after the cursor.
fn peek_next(text: &[char], cursor: Cursor) -> Option<char> {
    text.get(cursor.at + 1).copied()
}

/// Answers whether a character may start a name.
fn starts_name(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

/// Answers whether a character may continue a name.
fn continues_name(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '\''
}

/// Turns a name into the token it stands for.
fn classify(name: &str) -> Tok {
    match name {
        "abstract" => Tok::Abstract,
        "concrete" => Tok::Concrete,
        "of" => Tok::Of,
        "cat" => Tok::Cat,
        "fun" => Tok::Fun,
        "flags" => Tok::Flags,
        "param" => Tok::Param,
        "lincat" => Tok::Lincat,
        "lin" => Tok::Lin,
        "oper" => Tok::Oper,
        "table" => Tok::Table,
        "case" => Tok::Case,
        "Str" => Tok::Str,
        "Id" => Tok::IdType,
        "refl" => Tok::Refl,
        "J" => Tok::Eliminate,
        "fst" => Tok::First,
        "snd" => Tok::Second,
        "_" => Tok::Wildcard,
        _ => match universe_level(name) {
            Some(level) => Tok::Universe(level),
            None => Tok::Name(name.to_string()),
        },
    }
}

/// Reads the level of a universe name, such as `Type` or `Type2`.
fn universe_level(name: &str) -> Option<UniverseLevel> {
    let digits = name.strip_prefix("Type")?;
    if digits.is_empty() {
        return Some(UniverseLevel(0));
    }
    digits.parse::<u32>().ok().map(UniverseLevel)
}

/// Collects the characters that satisfy a test, starting at the cursor.
fn read_while(text: &[char], cursor: Cursor, keep: impl Fn(char) -> bool) -> (String, Cursor) {
    let mut collected = String::new();
    let mut here = cursor;
    while let Some(character) = peek(text, here) {
        if !keep(character) {
            break;
        }
        collected.push(character);
        here = step(here, character);
    }
    (collected, here)
}

/// Moves the cursor past a line comment.
fn skip_line_comment(text: &[char], cursor: Cursor) -> Cursor {
    let mut here = cursor;
    while let Some(character) = peek(text, here) {
        if character == '\n' {
            return step(here, character);
        }
        here = step(here, character);
    }
    here
}

/// Moves the cursor past a block comment.
fn skip_block_comment(text: &[char], cursor: Cursor) -> Result<Cursor, LexError> {
    let start = cursor;
    let mut here = step(step(cursor, '{'), '-');
    while let Some(character) = peek(text, here) {
        if character == '-' && peek_next(text, here) == Some('}') {
            return Ok(step(step(here, '-'), '}'));
        }
        here = step(here, character);
    }
    Err(LexError::UnterminatedComment { pos: start.pos() })
}

/// Moves the cursor past every space and comment.
fn skip_blank(text: &[char], cursor: Cursor) -> Result<Cursor, LexError> {
    let mut here = cursor;
    loop {
        match peek(text, here) {
            Some(character) if character.is_whitespace() => here = step(here, character),
            Some('-') if peek_next(text, here) == Some('-') => here = skip_line_comment(text, here),
            Some('{') if peek_next(text, here) == Some('-') => {
                here = skip_block_comment(text, here)?
            }
            _ => return Ok(here),
        }
    }
}

/// Reads a word list between quotation marks.
fn read_text(text: &[char], cursor: Cursor) -> Result<(Vec<String>, Cursor), LexError> {
    let start = cursor;
    let mut here = step(cursor, '"');
    let mut body = String::new();
    while let Some(character) = peek(text, here) {
        if character == '"' {
            let words = body
                .split_whitespace()
                .map(|word| word.to_string())
                .collect::<Vec<String>>();
            return Ok((words, step(here, character)));
        }
        body.push(character);
        here = step(here, character);
    }
    Err(LexError::UnterminatedString { pos: start.pos() })
}

/// The backslash character. The tool spells it as a number so that no
/// escape character reaches this file.
const BACKSLASH: char = 92u8 as char;

/// Reads the token that one or two punctuation characters spell.
fn punctuation(first: char, second: Option<char>) -> Option<(Tok, usize)> {
    match (first, second) {
        ('-', Some('>')) => Some((Tok::Arrow, 2)),
        ('=', Some('>')) => Some((Tok::TableArrow, 2)),
        ('+', Some('+')) => Some((Tok::Join, 2)),
        ('*', Some('*')) => Some((Tok::Times, 2)),
        (';', _) => Some((Tok::Semicolon, 1)),
        (',', _) => Some((Tok::Comma, 1)),
        (':', _) => Some((Tok::Colon, 1)),
        ('.', _) => Some((Tok::Dot, 1)),
        ('!', _) => Some((Tok::Select, 1)),
        ('=', _) => Some((Tok::Equals, 1)),
        (character, _) if character == BACKSLASH => Some((Tok::Backslash, 1)),
        ('(', _) => Some((Tok::OpenParen, 1)),
        (')', _) => Some((Tok::CloseParen, 1)),
        ('{', _) => Some((Tok::OpenBrace, 1)),
        ('}', _) => Some((Tok::CloseBrace, 1)),
        ('[', _) => Some((Tok::OpenBracket, 1)),
        (']', _) => Some((Tok::CloseBracket, 1)),
        ('|', _) => Some((Tok::Bar, 1)),
        ('<', _) => Some((Tok::OpenAngle, 1)),
        ('>', _) => Some((Tok::CloseAngle, 1)),
        _ => None,
    }
}

/// Splits one grammar source file into tokens.
pub fn lex(source: &SourceText) -> Result<Vec<Lexeme>, LexError> {
    let text: Vec<char> = source.0.chars().collect();
    let mut here = Cursor {
        at: 0,
        line: 1,
        column: 1,
    };
    let mut produced: Vec<Lexeme> = Vec::new();
    loop {
        here = skip_blank(&text, here)?;
        let character = match peek(&text, here) {
            None => return Ok(produced),
            Some(character) => character,
        };
        let start = here;
        if starts_name(character) {
            let (name, next) = read_while(&text, here, continues_name);
            produced.push(Lexeme {
                token: classify(&name),
                pos: start.pos(),
            });
            here = next;
        } else if character.is_ascii_digit() {
            let (digits, next) = read_while(&text, here, |c| c.is_ascii_digit());
            match digits.parse::<u32>() {
                Ok(number) => produced.push(Lexeme {
                    token: Tok::Number(number),
                    pos: start.pos(),
                }),
                Err(_) => {
                    return Err(LexError::UnexpectedCharacter {
                        pos: start.pos(),
                        found: character,
                    })
                }
            }
            here = next;
        } else if character == '"' {
            let (words, next) = read_text(&text, here)?;
            produced.push(Lexeme {
                token: Tok::Text(words),
                pos: start.pos(),
            });
            here = next;
        } else {
            match punctuation(character, peek_next(&text, here)) {
                Some((token, width)) => {
                    produced.push(Lexeme {
                        token,
                        pos: start.pos(),
                    });
                    here = (0..width).fold(here, |cursor, _| {
                        let seen = peek(&text, cursor).unwrap_or(' ');
                        step(cursor, seen)
                    });
                }
                None => {
                    return Err(LexError::UnexpectedCharacter {
                        pos: start.pos(),
                        found: character,
                    })
                }
            }
        }
    }
}
