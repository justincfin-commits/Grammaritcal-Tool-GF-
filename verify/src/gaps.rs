//! Model of `pass_down` in `src/compile/gaps.rs`.
//!
//! A production's children are modeled as a list of gap fields: `None` for a
//! child that misses nothing and `Some(g)` for a child that misses the phrase
//! `g`. Words cannot host a gap, so they are left out of the model.
//!
//! `pass_down` copies a base production once per child that carries no gap,
//! and puts the gap on that one child. Two facts follow and both are stated
//! as postconditions. Every copy differs from the base in exactly one child,
//! and that child carried no gap in the base. The second fact is what makes
//! the tree rebuilder's variable index zero correct: a child carries a gap in
//! the base exactly when its argument is a function, which is exactly where
//! a lambda is added; never sending a gap there means no lambda ever stands
//! between the gap and the lambda that binds it.

use vstd::prelude::*;

verus! {

/// `copy` is `base` with the gap placed on child `at`, and nothing else changed.
pub open spec fn is_copy_at(base: Seq<Option<u64>>, copy: Seq<Option<u64>>, at: int, gap: u64) -> bool {
    &&& copy.len() == base.len()
    &&& 0 <= at < base.len()
    &&& base[at].is_none()
    &&& copy[at] == Some(gap)
    &&& forall|j: int| 0 <= j < base.len() && j != at ==> copy[j] == base[j]
}

/// `copy` is `base` with the gap placed on some one child.
pub open spec fn some_copy(base: Seq<Option<u64>>, copy: Seq<Option<u64>>, gap: u64) -> bool {
    exists|at: int| #[trigger] is_copy_at(base, copy, at, gap)
}

/// How many of the first `n` children carry no gap.
pub open spec fn free_prefix(base: Seq<Option<u64>>, n: int) -> nat
    decreases n,
{
    if n <= 0 || n > base.len() {
        0
    } else {
        free_prefix(base, n - 1) + (if base[n - 1].is_none() { 1nat } else { 0nat })
    }
}

/// Copies the base production once per gap-free child, moving one gap onto
/// that child.
pub fn pass_down(base: &Vec<Option<u64>>, gap: u64) -> (copies: Vec<Vec<Option<u64>>>)
    ensures
        copies.len() == free_prefix(base@, base@.len() as int),
        forall|c: int| 0 <= c < copies.len() ==> #[trigger] some_copy(base@, copies@[c]@, gap),
        forall|at: int| 0 <= at < base.len() && (#[trigger] base@[at]).is_none()
            ==> exists|c: int| 0 <= c < copies.len() && #[trigger] is_copy_at(base@, copies@[c]@, at, gap),
{
    let mut copies: Vec<Vec<Option<u64>>> = Vec::new();
    // The child each copy placed its gap on, in copy order.
    let ghost mut ats: Seq<int> = Seq::empty();
    let mut i: usize = 0;
    while i < base.len()
        invariant
            i <= base.len(),
            copies.len() == free_prefix(base@, i as int),
            ats.len() == copies.len(),
            forall|c: int| 0 <= c < copies.len() ==> #[trigger] is_copy_at(base@, copies@[c]@, ats[c], gap),
            forall|at: int| 0 <= at < i && (#[trigger] base@[at]).is_none()
                ==> exists|c: int| 0 <= c < copies.len() && #[trigger] is_copy_at(base@, copies@[c]@, at, gap),
        decreases base.len() - i,
    {
        if base[i].is_none() {
            let ghost earlier = copies@;
            let mut copy: Vec<Option<u64>> = base.clone();
            copy.set(i, Some(gap));
            let ghost copy_view = copy@;
            assert(is_copy_at(base@, copy_view, i as int, gap));
            copies.push(copy);
            proof {
                ats = ats.push(i as int);
                assert forall|c: int| 0 <= c < copies.len()
                    implies #[trigger] is_copy_at(base@, copies@[c]@, ats[c], gap) by {
                    if c < earlier.len() {
                        assert(copies@[c] == earlier[c]);
                    } else {
                        assert(copies@[c]@ == copy_view);
                    }
                }
                assert forall|at: int| 0 <= at < i + 1 && (#[trigger] base@[at]).is_none()
                    implies exists|c: int| 0 <= c < copies.len()
                        && #[trigger] is_copy_at(base@, copies@[c]@, at, gap) by {
                    if at < i {
                        let c = choose|c: int| 0 <= c < earlier.len()
                            && #[trigger] is_copy_at(base@, earlier[c]@, at, gap);
                        assert(copies@[c] == earlier[c]);
                    } else {
                        let c = copies.len() - 1;
                        assert(copies@[c as int]@ == copy_view);
                        assert(is_copy_at(base@, copies@[c as int]@, i as int, gap));
                    }
                }
            }
        }
        i = i + 1;
    }
    proof {
        assert forall|c: int| 0 <= c < copies.len()
            implies #[trigger] some_copy(base@, copies@[c]@, gap) by {
            assert(is_copy_at(base@, copies@[c]@, ats[c], gap));
        }
    }
    copies
}

/// A copy has exactly one more gapped child than the base. This is the
/// "at most one gap per phrase" limit seen from the production side: the
/// base has no gapped child except the function arguments' own gaps, and the
/// copy adds exactly one.
pub proof fn lemma_copy_adds_one_gap(base: Seq<Option<u64>>, copy: Seq<Option<u64>>, at: int, gap: u64)
    requires
        is_copy_at(base, copy, at, gap),
    ensures
        gapped_prefix(copy, copy.len() as int) == gapped_prefix(base, base.len() as int) + 1,
{
    lemma_gapped_prefix_copy(base, copy, at, gap, base.len() as int);
}

/// How many of the first `n` children carry a gap.
pub open spec fn gapped_prefix(children: Seq<Option<u64>>, n: int) -> nat
    decreases n,
{
    if n <= 0 || n > children.len() {
        0
    } else {
        gapped_prefix(children, n - 1) + (if children[n - 1].is_some() { 1nat } else { 0nat })
    }
}

proof fn lemma_gapped_prefix_copy(base: Seq<Option<u64>>, copy: Seq<Option<u64>>, at: int, gap: u64, n: int)
    requires
        is_copy_at(base, copy, at, gap),
        0 <= n <= base.len(),
    ensures
        n <= at ==> gapped_prefix(copy, n) == gapped_prefix(base, n),
        n > at ==> gapped_prefix(copy, n) == gapped_prefix(base, n) + 1,
    decreases n,
{
    if n > 0 {
        lemma_gapped_prefix_copy(base, copy, at, gap, n - 1);
    }
}

} // verus!
