//! Token data for grammar source files.
//!
//! This module holds data only. The module `crate::gf::lexer` produces these
//! tokens and `crate::gf::parser` reads them.

use crate::error::SourcePos;
use crate::newtype::UniverseLevel;

/// One token of a grammar source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A name, such as `Prop` or `mkNoun`.
    Name(String),
    /// A whole number, which only the identity eliminator uses.
    Number(u32),
    /// A word list written between quotation marks.
    Text(Vec<String>),
    /// A universe, written `Type`, `Type1`, `Type2`, and so on.
    Universe(UniverseLevel),
    /// The head of an abstract module.
    Abstract,
    /// The head of a concrete module.
    Concrete,
    /// The word `of` in a concrete module head.
    Of,
    /// The word `cat`.
    Cat,
    /// The word `fun`.
    Fun,
    /// The word `flags`.
    Flags,
    /// The word `param`.
    Param,
    /// The word `lincat`.
    Lincat,
    /// The word `lin`.
    Lin,
    /// The word `oper`.
    Oper,
    /// The word `table`.
    Table,
    /// The word `case`.
    Case,
    /// The type of a run of words, written `Str`.
    Str,
    /// The identity type former, written `Id`.
    IdType,
    /// The reflexive proof, written `refl`.
    Refl,
    /// The identity eliminator, written `J`.
    Eliminate,
    /// The first projection, written `fst`.
    First,
    /// The second projection, written `snd`.
    Second,
    /// The wildcard pattern, written `_`.
    Wildcard,
    /// A semicolon.
    Semicolon,
    /// A comma.
    Comma,
    /// A colon.
    Colon,
    /// A full stop, which projects a record field.
    Dot,
    /// An exclamation mark, which selects a table row.
    Select,
    /// An equals sign.
    Equals,
    /// A function arrow, written `->`.
    Arrow,
    /// A table arrow, written `=>`.
    TableArrow,
    /// The join operator, written `++`.
    Join,
    /// The pair type former, written `**`.
    Times,
    /// A backslash, which starts a function.
    Backslash,
    /// An opening round bracket.
    OpenParen,
    /// A closing round bracket.
    CloseParen,
    /// An opening curly bracket.
    OpenBrace,
    /// A closing curly bracket.
    CloseBrace,
    /// An opening square bracket.
    OpenBracket,
    /// A closing square bracket.
    CloseBracket,
    /// A vertical bar, which separates parameter values.
    Bar,
    /// An opening angle bracket, which starts a pair.
    OpenAngle,
    /// A closing angle bracket, which ends a pair.
    CloseAngle,
}

/// One token together with the place the file writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexeme {
    /// The token.
    pub token: Tok,
    /// Where the token starts.
    pub pos: SourcePos,
}
