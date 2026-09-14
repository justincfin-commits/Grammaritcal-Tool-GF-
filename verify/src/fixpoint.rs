//! Model of `compute_nullable` in `src/compile/productions.rs`.
//!
//! The nullable computation keeps one flag per nonterminal and repeats a
//! round that can only turn flags on, until a round turns nothing on. The
//! main crate bounds the loop at `n + 1` rounds and reports
//! `NullableUnstable` if the bound is passed. This module proves that the
//! bound can never be passed: each round that changes anything turns at
//! least one more flag on, at most `n` flags exist, so at most `n` changing
//! rounds happen, and the round after the last change ends the loop.
//!
//! A production is modeled as a left side and a list of children; a
//! production with a word in it can never derive nothing, so it is left out
//! of the model.

use vstd::prelude::*;

verus! {

/// How many of the first `n` flags are on.
pub open spec fn on_prefix(flags: Seq<bool>, n: int) -> nat
    decreases n,
{
    if n <= 0 || n > flags.len() {
        0
    } else {
        on_prefix(flags, n - 1) + (if flags[n - 1] { 1nat } else { 0nat })
    }
}

/// How many flags are on.
pub open spec fn on_count(flags: Seq<bool>) -> nat {
    on_prefix(flags, flags.len() as int)
}

/// Every flag on before is on after.
pub open spec fn monotone(before: Seq<bool>, after: Seq<bool>) -> bool {
    &&& before.len() == after.len()
    &&& forall|j: int| 0 <= j < before.len() && before[j] ==> after[j]
}

proof fn lemma_on_prefix_bound(flags: Seq<bool>, n: int)
    requires
        0 <= n <= flags.len(),
    ensures
        on_prefix(flags, n) <= n,
    decreases n,
{
    if n > 0 {
        lemma_on_prefix_bound(flags, n - 1);
    }
}

/// At most as many flags are on as there are flags.
pub proof fn lemma_on_count_bound(flags: Seq<bool>)
    ensures
        on_count(flags) <= flags.len(),
{
    lemma_on_prefix_bound(flags, flags.len() as int);
}

proof fn lemma_on_prefix_monotone(before: Seq<bool>, after: Seq<bool>, n: int, witness: int)
    requires
        monotone(before, after),
        0 <= n <= before.len(),
        0 <= witness < before.len(),
        !before[witness],
        after[witness],
    ensures
        on_prefix(before, n) <= on_prefix(after, n),
        witness < n ==> on_prefix(before, n) < on_prefix(after, n),
    decreases n,
{
    if n > 0 {
        lemma_on_prefix_monotone(before, after, n - 1, witness);
    }
}

/// A round that turns some flag on, and turns none off, strictly raises
/// the number of flags that are on.
pub proof fn lemma_change_raises_count(before: Seq<bool>, after: Seq<bool>, witness: int)
    requires
        monotone(before, after),
        0 <= witness < before.len(),
        !before[witness],
        after[witness],
    ensures
        on_count(before) < on_count(after),
{
    lemma_on_prefix_monotone(before, after, before.len() as int, witness);
}

/// One round: every production whose children are all on turns its left
/// side on. Returns whether anything changed.
pub fn round(flags: &mut Vec<bool>, productions: &Vec<(usize, Vec<usize>)>) -> (changed: bool)
    requires
        forall|p: int| 0 <= p < productions.len() ==> productions@[p].0 < old(flags).len(),
        forall|p: int, c: int| 0 <= p < productions.len() && 0 <= c < productions@[p].1.len()
            ==> productions@[p].1@[c] < old(flags).len(),
    ensures
        monotone(old(flags)@, final(flags)@),
        changed ==> exists|j: int| 0 <= j < final(flags).len() && !old(flags)@[j] && final(flags)@[j],
        !changed ==> final(flags)@ == old(flags)@,
{
    let ghost before = flags@;
    let mut changed = false;
    let mut p: usize = 0;
    while p < productions.len()
        invariant
            p <= productions.len(),
            flags.len() == before.len(),
            monotone(before, flags@),
            forall|q: int| 0 <= q < productions.len() ==> productions@[q].0 < flags.len(),
            forall|q: int, c: int| 0 <= q < productions.len() && 0 <= c < productions@[q].1.len()
                ==> productions@[q].1@[c] < flags.len(),
            changed ==> exists|j: int| 0 <= j < flags.len() && !before[j] && flags@[j],
            !changed ==> flags@ == before,
        decreases productions.len() - p,
    {
        let lhs = productions[p].0;
        if !flags[lhs] {
            let mut all_on = true;
            let mut c: usize = 0;
            let children_len = productions[p].1.len();
            while c < children_len
                invariant
                    c <= children_len,
                    children_len == productions@[p as int].1.len(),
                    p < productions.len(),
                    flags.len() == before.len(),
                    forall|k: int| 0 <= k < children_len ==> productions@[p as int].1@[k] < flags.len(),
                decreases children_len - c,
            {
                let child = productions[p].1[c];
                if !flags[child] {
                    all_on = false;
                }
                c = c + 1;
            }
            if all_on {
                flags.set(lhs, true);
                changed = true;
                assert(!before[lhs as int] && flags@[lhs as int]);
            }
        }
        p = p + 1;
    }
    changed
}

/// Repeats rounds until one changes nothing. Returns how many rounds ran.
///
/// The postcondition is the bound the main crate relies on: the number of
/// rounds never passes the number of flags plus one.
pub fn compute_nullable(flags: &mut Vec<bool>, productions: &Vec<(usize, Vec<usize>)>) -> (rounds: usize)
    requires
        forall|p: int| 0 <= p < productions.len() ==> productions@[p].0 < old(flags).len(),
        forall|p: int, c: int| 0 <= p < productions.len() && 0 <= c < productions@[p].1.len()
            ==> productions@[p].1@[c] < old(flags).len(),
        forall|j: int| 0 <= j < old(flags).len() ==> !old(flags)@[j],
        old(flags).len() < usize::MAX,
    ensures
        rounds <= final(flags).len() + 1,
        final(flags).len() == old(flags).len(),
{
    let ghost start = flags@;
    let mut rounds: usize = 0;
    proof {
        lemma_on_count_bound(flags@);
    }
    loop
        invariant
            flags.len() == start.len(),
            flags.len() == old(flags).len(),
            forall|p: int| 0 <= p < productions.len() ==> productions@[p].0 < flags.len(),
            forall|p: int, c: int| 0 <= p < productions.len() && 0 <= c < productions@[p].1.len()
                ==> productions@[p].1@[c] < flags.len(),
            rounds <= on_count(flags@),
            on_count(flags@) <= flags.len(),
            flags.len() < usize::MAX,
        ensures
            rounds <= flags.len() + 1,
            flags.len() == start.len(),
        decreases flags.len() - rounds,
    {
        let ghost before = flags@;
        let changed = round(flags, productions);
        proof {
            lemma_on_count_bound(flags@);
        }
        if !changed {
            // This round ran but changed nothing; count it and stop.
            return rounds + 1;
        }
        proof {
            let witness = choose|j: int| 0 <= j < flags.len() && !before[j] && flags@[j];
            lemma_change_raises_count(before, flags@, witness);
        }
        rounds = rounds + 1;
    }
}

} // verus!
