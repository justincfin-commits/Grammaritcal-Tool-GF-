//! Model of the level and index arithmetic in `src/mltt/eval.rs`.
//!
//! Terms carry de Bruijn indices, counted from the nearest binder inward.
//! Values carry de Bruijn levels, counted from the outside in. Under `depth`
//! binders, the variable at level `l` has index `depth - l - 1`, and the
//! variable at index `i` has level `depth - i - 1`. `quote_neutral` turns a
//! level back into an index with `depth.checked_sub(level).and_then(|d|
//! d.checked_sub(1))`, which is defined exactly when `level < depth`.

use vstd::prelude::*;

verus! {

/// Turns a level into an index under `depth` binders, as `quote_neutral`
/// does. Returns nothing when the level is not under the depth.
pub fn level_to_index(depth: usize, level: usize) -> (index: Option<usize>)
    ensures
        level < depth ==> index == Some((depth - level - 1) as usize),
        level >= depth ==> index.is_none(),
        index.is_some() ==> index.unwrap() < depth,
{
    if level < depth {
        Some(depth - level - 1)
    } else {
        None
    }
}

/// Turns an index into a level under `depth` binders, as `fresh` and `bind`
/// do when they push a new variable.
pub fn index_to_level(depth: usize, index: usize) -> (level: Option<usize>)
    ensures
        index < depth ==> level == Some((depth - index - 1) as usize),
        index >= depth ==> level.is_none(),
        level.is_some() ==> level.unwrap() < depth,
{
    if index < depth {
        Some(depth - index - 1)
    } else {
        None
    }
}

/// The two conversions invert each other, so quoting a value that was built
/// from a term gives back the term's variable indices unchanged.
pub proof fn lemma_round_trip(depth: usize, level: usize)
    requires
        level < depth,
    ensures
        (depth - ((depth - level - 1) as usize) - 1) as usize == level,
{
}

/// Counting one more binder, as `deeper` does, keeps every existing level
/// under the depth and gives each existing variable an index one larger.
pub proof fn lemma_deeper_shifts_indices(depth: usize, level: usize)
    requires
        level < depth,
        depth < usize::MAX,
    ensures
        level < depth + 1,
        ((depth + 1) - level - 1) as usize == ((depth - level - 1) as usize) + 1,
{
}

} // verus!
