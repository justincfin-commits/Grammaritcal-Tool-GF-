//! Model of `check_linearity` in `src/compile/productions.rs`.
//!
//! One output field of a rule is a list of items. Each item is either a word
//! or a slot that names an argument. The compiler accepts the field exactly
//! when every argument below the arity appears in exactly one slot. That is
//! the condition that keeps the compiled grammar context free: a production
//! can place each child once and only once.
//!
//! The model writes a slot for argument `a` as `Some(a)` and a word as `None`.

use vstd::prelude::*;

verus! {

/// How many of the first `n` items are slots for `arg`.
pub open spec fn count_prefix(items: Seq<Option<usize>>, n: int, arg: usize) -> nat
    decreases n,
{
    if n <= 0 || n > items.len() {
        0
    } else {
        count_prefix(items, n - 1, arg) + (if items[n - 1] == Some(arg) { 1nat } else { 0nat })
    }
}

/// How many items are slots for `arg`.
pub open spec fn count(items: Seq<Option<usize>>, arg: usize) -> nat {
    count_prefix(items, items.len() as int, arg)
}

/// The condition the compiler enforces on one output field.
pub open spec fn linear(items: Seq<Option<usize>>, arity: usize) -> bool {
    forall|a: usize| a < arity ==> count(items, a) == 1
}

/// Counts the slots of each argument, then demands exactly one each.
///
/// Returns true exactly when the field is linear. An argument that never
/// appears (count zero) and an argument that appears twice (count two) are
/// both refused; the main crate names them `ArgumentDropped` and
/// `ArgumentRepeated`.
pub fn check_linearity(items: &Vec<Option<usize>>, arity: usize) -> (ok: bool)
    ensures
        ok == linear(items@, arity),
{
    let mut counts: Vec<usize> = Vec::new();
    let mut a: usize = 0;
    while a < arity
        invariant
            a <= arity,
            counts.len() == a,
            forall|k: int| 0 <= k < a ==> counts@[k] == 0,
        decreases arity - a,
    {
        counts.push(0);
        a = a + 1;
    }
    let mut i: usize = 0;
    while i < items.len()
        invariant
            i <= items.len(),
            counts.len() == arity,
            forall|k: int| 0 <= k < arity ==> counts@[k] == count_prefix(items@, i as int, k as usize),
            forall|k: int| 0 <= k < arity ==> counts@[k] <= i,
        decreases items.len() - i,
    {
        match items[i] {
            Some(arg) => {
                if arg < arity {
                    let seen = counts[arg];
                    counts.set(arg, seen + 1);
                }
            }
            None => {}
        }
        i = i + 1;
    }
    let mut j: usize = 0;
    let mut ok = true;
    while j < arity
        invariant
            j <= arity,
            counts.len() == arity,
            forall|k: int| 0 <= k < arity ==> counts@[k] == count(items@, k as usize),
            ok == (forall|k: usize| k < j ==> count(items@, k) == 1),
        decreases arity - j,
    {
        if counts[j] != 1 {
            ok = false;
        }
        j = j + 1;
    }
    ok
}

} // verus!
