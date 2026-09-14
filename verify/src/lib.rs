//! Verus models of the core algorithms of `gf-mltt`.
//!
//! Every function here is a model of a function in `../src`, written in the
//! subset of Rust that Verus verifies, with the property the main crate
//! relies on stated as a postcondition and checked by the SMT solver.
//!
//! | module      | models                                   | proves |
//! | ----------- | ---------------------------------------- | ------ |
//! | `fuel`      | `spend` in `src/mltt/eval.rs`            | fuel strictly decreases or the call stops; a fuelled loop terminates |
//! | `debruijn`  | `quote_neutral` in `src/mltt/eval.rs`    | the level-to-index arithmetic is defined exactly when the level is under the depth, and it inverts the index-to-level arithmetic |
//! | `linearity` | `check_linearity` in `src/compile/productions.rs` | the check accepts exactly when every argument appears once |
//! | `gaps`      | `pass_down` in `src/compile/gaps.rs`     | every copy moves the gap onto exactly one child, and that child carried no gap before |
//! | `fixpoint`  | `compute_nullable` in `src/compile/productions.rs` | a monotone fixed point over `n` flags takes at most `n` strict rounds |
//! | `dcbor`     | `put_head` and `read_head` in `src/dcbor.rs` | the head is the shortest form for its value, and reading it back gives the value and the major type |

pub mod dcbor;
pub mod debruijn;
pub mod fixpoint;
pub mod fuel;
pub mod gaps;
pub mod linearity;
