//! Threads one missing phrase through the productions for quantifier rules.
//!
//! This module holds behavior only.
//!
//! A quantifier such as `Every` takes a function argument `Ind k -> Prop`.
//! The compiler turns that argument into a nonterminal that misses one noun
//! phrase: a `Prop` with a gap of category `Ind`. Two production families make
//! such a nonterminal derivable.
//!
//! The pass-down family copies every base production once per gap and per
//! child that carries no gap, and moves the gap onto that one child. The
//! gap-site family lets the gapped nonterminal of the gap's own category match
//! no words at all; that is the place where the bound variable stands.
//!
//! One sweep is enough. Copies are made from base productions only, and no
//! copy invents a new gap, so nothing needs a second pass.

use std::collections::{BTreeMap, BTreeSet};

use crate::compile::grammar::{Build, CompileLimits, Nonterminal, RawProduction, RawSymbol};
use crate::concrete::lintype::FieldPath;
use crate::concrete::value::GapSpec;
use crate::error::CompileError;
use crate::newtype::CatName;

/// Reads every missing phrase that some rule already asks for.
pub fn collect_gaps(
    base: &[RawProduction],
    limits: CompileLimits,
) -> Result<BTreeSet<GapSpec>, CompileError> {
    let mut found: BTreeSet<GapSpec> = BTreeSet::new();
    for production in base {
        for symbol in &production.rhs {
            if let RawSymbol::Child { nonterminal, .. } = symbol {
                if let Some(spec) = &nonterminal.gap {
                    found.insert(spec.clone());
                }
            }
        }
    }
    if found.len() > limits.gaps.0 {
        return Err(CompileError::TooManyGaps {
            count: found.len(),
            limit: limits.gaps,
        });
    }
    Ok(found)
}

/// Copies each base production once per gap and per ungapped child, moving
/// the gap onto that one child.
///
/// A child that already carries a gap is skipped. That rule is what keeps the
/// tree rebuilder's variable index correct. A slot carries a gap exactly when
/// its argument is a function, which is exactly where the rebuilder wraps the
/// subtree in a lambda. Skipping those positions means the chain from a
/// gapped left side down to the gap site never passes through a lambda, so
/// the bound variable at the site is always the nearest binder, index zero.
pub fn pass_down(
    base: &[RawProduction],
    gaps: &BTreeSet<GapSpec>,
    rank: &mut usize,
) -> Vec<RawProduction> {
    let mut made: Vec<RawProduction> = Vec::new();
    for production in base {
        for gap in gaps {
            for position in 0..production.rhs.len() {
                let hosts_gap = match production.rhs.get(position) {
                    Some(RawSymbol::Child { nonterminal, .. }) => nonterminal.gap.is_none(),
                    _ => false,
                };
                if !hosts_gap {
                    continue;
                }
                let mut rhs = production.rhs.clone();
                if let Some(RawSymbol::Child { nonterminal, .. }) = rhs.get_mut(position) {
                    nonterminal.gap = Some(gap.clone());
                }
                let mut lhs = production.lhs.clone();
                lhs.gap = Some(gap.clone());
                let here = *rank;
                *rank = rank.saturating_add(1);
                made.push(RawProduction {
                    rank: here,
                    lhs,
                    rhs,
                    build: production.build.clone(),
                });
            }
        }
    }
    made
}

/// Emits the empty production that lets a missing phrase match no words.
///
/// For a gap of category `C` in state `s`, and each part `q` of `C` that
/// prints words, the nonterminal `(C, s, q, gap)` derives the empty list. The
/// ungapped `(C, s, q, none)` does not, so a real phrase can never stand where
/// the bound variable belongs.
pub fn gap_sites(
    gaps: &BTreeSet<GapSpec>,
    paths_of_cat: &BTreeMap<CatName, Vec<FieldPath>>,
    rank: &mut usize,
) -> Vec<RawProduction> {
    let mut made: Vec<RawProduction> = Vec::new();
    for gap in gaps {
        for path in paths_of_cat.get(&gap.cat).into_iter().flatten() {
            let here = *rank;
            *rank = rank.saturating_add(1);
            made.push(RawProduction {
                rank: here,
                lhs: Nonterminal {
                    cat: gap.cat.clone(),
                    state: gap.state.clone(),
                    path: path.clone(),
                    gap: Some(gap.clone()),
                },
                rhs: Vec::new(),
                build: Build::GapSite,
            });
        }
    }
    made
}
