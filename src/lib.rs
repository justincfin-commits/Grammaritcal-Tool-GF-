//! A Grammatical Framework tool whose abstract syntax is Martin-Lof type
//! theory.
//!
//! The library reads two grammar source files and a piece of text, and returns
//! one dependently typed abstract syntax tree for each reading of each
//! sentence. The library performs no input and no output. It opens no file,
//! reads no clock, draws no random number, and mutates no global state. The
//! binary `gfmltt` performs every file read and every print, and calls this
//! library once.
//!
//! The modules split data from behavior. `newtype`, `error`, `mltt::term`,
//! `mltt::value`, `mltt::signature`, `concrete::lintype`, `concrete::value`,
//! `compile::grammar`, `parse::input`, and `parse::chart` hold data. Every
//! other module holds functions that read data and return data.
//!
//! Five stages run inside `analyze::analyze`. The GF parser reads the two
//! grammar files. The lowering step turns `cat` and `fun` declarations into
//! constants of the type theory. The compiler turns each `lin` rule into
//! context-free productions. An Earley chart parser reads each sentence into
//! candidate trees. The dependent type checker rejects every candidate that
//! violates a type index and returns the survivors with their normal types.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::indexing_slicing
)]

pub mod analyze;
pub mod compile;
pub mod concrete;
pub mod dcbor;
pub mod error;
pub mod gf;
pub mod mltt;
pub mod newtype;
pub mod parse;
pub mod render;
