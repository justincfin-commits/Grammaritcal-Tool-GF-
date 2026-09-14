//! Rebuilds abstract terms from completed chart items.
//!
//! This module holds behavior only.
//!
//! The walk starts at each root span and follows the packed forest downward.
//! A production for `fun F` with children `t0 .. tn` becomes the application
//! `F t0 .. tn`, left nested. A production that marks the place of a bound
//! variable becomes `Var(0)`. A function argument receives a subtree that
//! holds exactly one such variable, and the walk wraps that subtree in a
//! lambda. No other lambda ever stands between that lambda and its variable,
//! because the gap threading never sends a gap into a child that already
//! carries one; so index zero is always right.
//!
//! Every recursive function spends fuel, and a span already on the current
//! path is cut, so the walk is total even on a grammar with a cycle.

use std::collections::{BTreeMap, BTreeSet};

use crate::compile::grammar::{ArgSpec, Build, Production, Symbol};
use crate::error::ReadError;
use crate::mltt::check::infer_closed;
use crate::mltt::term::{ConstName, Term};
use crate::newtype::{
    ArgIndex, ChartPos, DeBruijnIndex, DotPos, EvalFuel, FunName, SplitLimit, TreeLimit,
};
use crate::parse::chart::{
    Built, DotKey, Enumeration, Forest, ParseTables, Rejection, SpanKey, TypeFilter,
};

/// What the walk carries while it enumerates.
struct Walk<'a> {
    /// The spans the walk currently stands inside.
    active: BTreeSet<SpanKey>,
    /// The terms already built for each span. Only complete results are
    /// stored, never a result that a cut shortened.
    memo: BTreeMap<SpanKey, Vec<Built>>,
    truncated: bool,
    cycles_cut: usize,
    rejected: Vec<Rejection>,
    rejected_total: usize,
    filter: Option<&'a TypeFilter<'a>>,
    limit: TreeLimit,
    splits: SplitLimit,
}

/// Spends one unit of fuel, or reports that the budget ran out.
fn spend(fuel: EvalFuel) -> Result<EvalFuel, ReadError> {
    match fuel.0.checked_sub(1) {
        Some(left) => Ok(EvalFuel(left)),
        None => Err(ReadError::OutOfFuel),
    }
}

/// Builds `F t0 .. tn`, left nested, in argument order.
fn spine(fun: &FunName, args: &[Term]) -> Term {
    let mut here = Term::Const(ConstName::Function(fun.clone()));
    for argument in args {
        here = Term::App {
            function: Box::new(here),
            argument: Box::new(argument.clone()),
        };
    }
    here
}

/// Lists every way the right side of a production splits one span.
///
/// The walk follows the dot chain backwards from the end of the right side.
/// Each split vector has one more entry than the right side has symbols; it
/// starts at the span's start and ends at the span's end.
fn split_vectors(
    walk: &Walk<'_>,
    forest: &Forest,
    production: &Production,
    key: SpanKey,
) -> Result<Vec<Vec<ChartPos>>, ReadError> {
    let mut frontier: Vec<(DotPos, ChartPos, Vec<ChartPos>)> =
        vec![(DotPos(production.rhs.len()), key.end, vec![key.end])];
    for _ in 0..production.rhs.len() {
        let mut next: Vec<(DotPos, ChartPos, Vec<ChartPos>)> = Vec::new();
        for (dot, at, tail) in &frontier {
            let earlier = dot.0.checked_sub(1).ok_or(ReadError::BrokenDerivation)?;
            let steps = forest
                .dots
                .get(&DotKey {
                    production: production.id,
                    dot: *dot,
                    origin: key.start,
                    end: *at,
                })
                .ok_or(ReadError::BrokenDerivation)?;
            for step in steps {
                if next.len() >= walk.splits.0 {
                    return Err(ReadError::TooManySplits {
                        production: production.id,
                        limit: walk.splits,
                    });
                }
                let mut extended = vec![step.split];
                extended.extend(tail.iter().copied());
                next.push((DotPos(earlier), step.split, extended));
            }
        }
        frontier = next;
    }
    Ok(frontier
        .into_iter()
        .filter(|(_, at, _)| *at == key.start)
        .map(|(_, _, splits)| splits)
        .collect())
}

/// Runs the type filter on one closed term. Returns whether it survives.
fn survives(walk: &mut Walk<'_>, term: &Term) -> bool {
    let filter = match walk.filter {
        Some(filter) => filter,
        None => return true,
    };
    match infer_closed(filter.signature, term, filter.fuel) {
        Ok(_) => true,
        Err(error) => {
            walk.rejected_total = walk.rejected_total.saturating_add(1);
            if walk.rejected.len() < filter.keep.0 {
                walk.rejected.push(Rejection {
                    tree: term.clone(),
                    error,
                });
            }
            false
        }
    }
}

/// Combines the candidate children of one production into whole subtrees.
fn cross(
    walk: &mut Walk<'_>,
    fun: &FunName,
    specs: &[ArgSpec],
    per_arg: &[Vec<Built>],
    out: &mut Vec<Built>,
) -> Result<(), ReadError> {
    // Each element is one choice of children so far, plus the running count
    // of unbound variables.
    let mut partial: Vec<(Vec<Term>, usize)> = vec![(Vec::new(), 0usize)];
    for (position, spec) in specs.iter().enumerate() {
        let choices = per_arg.get(position).ok_or(ReadError::BrokenDerivation)?;
        let mut next: Vec<(Vec<Term>, usize)> = Vec::new();
        for (prefix, count) in &partial {
            for child in choices {
                if next.len() >= walk.limit.0 {
                    walk.truncated = true;
                    break;
                }
                let (piece, added) = match spec {
                    // A function argument. The child must hold exactly one
                    // bound variable, and the lambda binds it.
                    ArgSpec::HigherOrder { binder, .. } => {
                        if child.traces != 1 {
                            return Err(ReadError::WrongTraceCount {
                                fun: fun.clone(),
                                arg: ArgIndex(position),
                                found: child.traces,
                            });
                        }
                        (
                            Term::Lam {
                                binder: binder.clone(),
                                body: Box::new(child.term.clone()),
                            },
                            0usize,
                        )
                    }
                    // An ordinary argument. A variable inside it belongs to a
                    // lambda further out, so it keeps counting.
                    ArgSpec::FirstOrder { .. } => (child.term.clone(), child.traces),
                };
                let total = count.saturating_add(added);
                if total > 1 {
                    return Err(ReadError::WrongTraceCount {
                        fun: fun.clone(),
                        arg: ArgIndex(position),
                        found: total,
                    });
                }
                let mut extended = prefix.clone();
                extended.push(piece);
                next.push((extended, total));
            }
        }
        partial = next;
    }
    for (args, traces) in partial {
        if out.len() >= walk.limit.0 {
            walk.truncated = true;
            break;
        }
        let term = spine(fun, &args);
        if traces == 0 && !survives(walk, &term) {
            continue;
        }
        out.push(Built { term, traces });
    }
    Ok(())
}

/// Turns one production over one span into subtrees.
fn terms_of_production(
    walk: &mut Walk<'_>,
    tables: &ParseTables,
    forest: &Forest,
    key: SpanKey,
    production: &Production,
    fuel: EvalFuel,
) -> Result<(Vec<Built>, bool), ReadError> {
    let fuel = spend(fuel)?;
    let fun = match &production.build {
        Build::GapSite => {
            return Ok((
                vec![Built {
                    term: Term::Var(DeBruijnIndex(0)),
                    traces: 1,
                }],
                false,
            ))
        }
        Build::Apply(fun) => fun,
    };
    let shape = tables
        .grammar
        .rules
        .get(fun)
        .ok_or(ReadError::BrokenDerivation)?;
    let vectors = split_vectors(walk, forest, production, key)?;
    let mut out: Vec<Built> = Vec::new();
    let mut cut = false;
    for splits in vectors {
        let mut per_arg: Vec<Vec<Built>> = vec![Vec::new(); shape.args.len()];
        let mut usable = true;
        for (at, symbol) in production.rhs.iter().enumerate() {
            let (arg, child_symbol) = match symbol {
                Symbol::Word(_) => continue,
                Symbol::Child { arg, nonterminal } => (*arg, *nonterminal),
            };
            let start = *splits.get(at).ok_or(ReadError::BrokenDerivation)?;
            let end = *splits
                .get(at.saturating_add(1))
                .ok_or(ReadError::BrokenDerivation)?;
            let (children, cut_here) = terms_of_span(
                walk,
                tables,
                forest,
                SpanKey {
                    symbol: child_symbol,
                    start,
                    end,
                },
                fuel,
            )?;
            cut = cut || cut_here;
            if children.is_empty() {
                usable = false;
                break;
            }
            let slot = per_arg.get_mut(arg.0).ok_or(ReadError::BrokenDerivation)?;
            *slot = children;
        }
        if !usable {
            continue;
        }
        cross(walk, fun, &shape.args, &per_arg, &mut out)?;
    }
    Ok((out, cut))
}

/// Turns one span into subtrees.
fn terms_of_span(
    walk: &mut Walk<'_>,
    tables: &ParseTables,
    forest: &Forest,
    key: SpanKey,
    fuel: EvalFuel,
) -> Result<(Vec<Built>, bool), ReadError> {
    let fuel = spend(fuel)?;
    if walk.active.contains(&key) {
        walk.cycles_cut = walk.cycles_cut.saturating_add(1);
        return Ok((Vec::new(), true));
    }
    if let Some(found) = walk.memo.get(&key) {
        return Ok((found.clone(), false));
    }
    let productions = forest
        .spans
        .get(&key)
        .ok_or(ReadError::BrokenDerivation)?
        .clone();
    walk.active.insert(key);
    let mut out: Vec<Built> = Vec::new();
    let mut cut = false;
    for id in productions {
        if out.len() >= walk.limit.0 {
            walk.truncated = true;
            break;
        }
        let production = tables
            .grammar
            .productions
            .get(id.0)
            .ok_or(ReadError::BrokenDerivation)?;
        let (mut built, cut_here) =
            terms_of_production(walk, tables, forest, key, production, fuel)?;
        cut = cut || cut_here;
        out.append(&mut built);
    }
    walk.active.remove(&key);
    out.sort();
    out.dedup();
    if out.len() > walk.limit.0 {
        out.truncate(walk.limit.0);
        walk.truncated = true;
    }
    // A result that a cut shortened is not a correct answer for this span in
    // every context, so it is never stored.
    if !cut {
        walk.memo.insert(key, out.clone());
    }
    Ok((out, cut))
}

/// Enumerates the trees of every root span.
///
/// When a filter is given, every closed subtree is type checked as soon as it
/// is built, and one that fails is dropped. That never changes the set of
/// well typed trees, because a closed subterm's type does not depend on the
/// context it stands in.
pub fn enumerate(
    tables: &ParseTables,
    forest: &Forest,
    limit: TreeLimit,
    splits: SplitLimit,
    fuel: EvalFuel,
    filter: Option<&TypeFilter<'_>>,
) -> Result<Enumeration, ReadError> {
    let mut walk = Walk {
        active: BTreeSet::new(),
        memo: BTreeMap::new(),
        truncated: false,
        cycles_cut: 0,
        rejected: Vec::new(),
        rejected_total: 0,
        filter,
        limit,
        splits,
    };
    let mut terms: Vec<Term> = Vec::new();
    for root in &forest.roots {
        let (built, _) = terms_of_span(&mut walk, tables, forest, *root, fuel)?;
        for one in built {
            if one.traces != 0 {
                let nonterminal = tables
                    .grammar
                    .nonterminals
                    .get(root.symbol.0)
                    .ok_or(ReadError::BrokenDerivation)?;
                return Err(ReadError::TraceOutsideBinder {
                    cat: nonterminal.cat.clone(),
                    state: nonterminal.state.clone(),
                });
            }
            terms.push(one.term);
        }
    }
    terms.sort();
    terms.dedup();
    Ok(Enumeration {
        terms,
        truncated: walk.truncated,
        cycles_cut: walk.cycles_cut,
        rejected: walk.rejected,
        rejected_total: walk.rejected_total,
    })
}
