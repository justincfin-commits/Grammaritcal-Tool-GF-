//! Model of `spend` in `src/mltt/eval.rs`, `src/concrete/eval.rs`, and
//! `src/parse/forest.rs`.
//!
//! Every recursive function in the main crate spends one unit of fuel per
//! call and stops with a named error when the fuel is gone. The property that
//! makes every such function total is: `spend` either fails or returns
//! strictly less fuel. A function whose every recursive call takes strictly
//! less fuel terminates.

use vstd::prelude::*;

verus! {

/// Spends one unit of fuel, or reports that the budget ran out.
pub fn spend(fuel: u32) -> (left: Option<u32>)
    ensures
        fuel == 0 ==> left.is_none(),
        fuel > 0 ==> left == Some((fuel - 1) as u32),
        left.is_some() ==> left.unwrap() < fuel,
{
    if fuel == 0 {
        None
    } else {
        Some(fuel - 1)
    }
}

/// A recursive walk that spends fuel on every call. Verus checks that it
/// terminates because `fuel` strictly decreases, which is exactly the
/// argument the main crate makes for `eval`, `quote`, `convertible`,
/// `infer`, `check`, `eval_cexpr`, and the forest walker.
///
/// The walk returns how many steps it took. With enough fuel it takes
/// `work` steps; otherwise it stops when the fuel runs out.
pub fn walk(fuel: u32, work: u32) -> (steps: u32)
    ensures
        steps <= fuel,
        steps <= work,
        work <= fuel ==> steps == work,
    decreases fuel,
{
    if work == 0 {
        return 0;
    }
    match spend(fuel) {
        None => 0,
        Some(left) => {
            let below = walk(left, work - 1);
            below + 1
        }
    }
}

} // verus!
