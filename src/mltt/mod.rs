//! The core type theory: data in `term`, `value`, and `signature`; behavior in
//! `eval`, `check`, and the one check in `signature`.

pub mod check;
pub mod eval;
pub mod signature;
pub mod term;
pub mod value;
