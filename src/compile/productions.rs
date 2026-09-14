//! Compiles each `lin` rule into context-free productions.
//!
//! This module holds behavior only.
//!
//! For each rule the compiler fixes a parameter state for every argument,
//! evaluates the rule body with symbolic arguments, and reads one production
//! off every output field of the result category. A symbolic argument is a
//! value whose word lists hold slots instead of words, so the evaluated body
//! shows exactly where each argument's words belong. Each argument must
//! appear exactly once in each output field; that is what keeps the compiled
//! grammar context free.
//!
//! The compiler never reads the index arguments of a category application.
//! `CN k` and `CN Animate` both compile to the nonterminal family of `CN`.
//! The context-free stage therefore over-generates, and the type checker
//! filters the result. That split is the design.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::compile::gaps::{collect_gaps, gap_sites, pass_down};
use crate::compile::grammar::{
    ArgSpec, Build, CompileLimits, CompiledGrammar, Nonterminal, Production, RawProduction,
    RawSymbol, RuleShape, Symbol,
};
use crate::concrete::eval::{bind, build_helpers, eval_cexpr, Helpers};
use crate::concrete::lintype::{FieldPath, LinType, ParamState, ValueShape};
use crate::concrete::shape::{
    enumerate_states, state_of_value, string_paths, symbolic_value, text_at_path, ParamTypes,
};
use crate::concrete::value::{CEnv, CValue, Slot, StrItem};
use crate::error::{CompileError, CompileStage, ConcreteError, UnsupportedReason};
use crate::gf::ast::{ConcreteModule, LinDecl};
use crate::gf::lower::AbstractGrammar;
use crate::mltt::term::{ConstName, Term};
use crate::newtype::{
    ArgIndex, CatName, EvalFuel, FunName, NonterminalId, ProductionId, StateLimit, Token, VarName,
};

/// One argument of a constructor as the telescope of its type presents it.
struct Telescoped<'a> {
    ty: &'a Term,
}

/// Spends one unit of a walk budget.
fn spend(fuel: EvalFuel, stage: CompileStage) -> Result<EvalFuel, CompileError> {
    match fuel.0.checked_sub(1) {
        Some(left) => Ok(EvalFuel(left)),
        None => Err(CompileError::OutOfFuel { stage }),
    }
}

/// Splits a constructor type into its arguments and its result.
///
/// The loop is greedy over `Pi`. A domain is not under its own binder, so
/// argument `d` sees exactly `d` enclosing binders and needs no shift. Only
/// the head of each application is ever read, so the indices never matter.
fn strip_telescope(
    ty: &Term,
    fuel: EvalFuel,
) -> Result<(Vec<Telescoped<'_>>, &Term), CompileError> {
    let mut args: Vec<Telescoped<'_>> = Vec::new();
    let mut here = ty;
    let mut left = fuel;
    while let Term::Pi {
        domain, codomain, ..
    } = here
    {
        left = spend(left, CompileStage::ArgumentTypes)?;
        args.push(Telescoped {
            ty: domain.as_ref(),
        });
        here = codomain.as_ref();
    }
    Ok((args, here))
}

/// Walks down the function side of an application and returns the head.
fn spine_head(ty: &Term, fuel: EvalFuel) -> Result<&Term, CompileError> {
    let mut here = ty;
    let mut left = fuel;
    while let Term::App { function, .. } = here {
        left = spend(left, CompileStage::ArgumentTypes)?;
        here = function.as_ref();
    }
    Ok(here)
}

/// Names the category a type head stands for, or says why it is not one.
fn category_of(head: &Term) -> Result<CatName, UnsupportedReason> {
    match head {
        Term::Const(ConstName::Category(name)) => Ok(name.clone()),
        Term::Universe(_) => Err(UnsupportedReason::Universe),
        Term::Sigma { .. }
        | Term::Pair { .. }
        | Term::Id { .. }
        | Term::Refl { .. }
        | Term::J { .. }
        | Term::First(_)
        | Term::Second(_) => Err(UnsupportedReason::NotSupportedByParser),
        Term::Pi { .. } => Err(UnsupportedReason::NestedFunction),
        Term::Var(_) | Term::Const(ConstName::Function(_)) | Term::Lam { .. } | Term::App { .. } => {
            Err(UnsupportedReason::NotACategory)
        }
    }
}

/// Reads what kind of argument one telescope entry is.
fn classify_argument(
    fun: &FunName,
    index: ArgIndex,
    arg: &Telescoped<'_>,
    fuel: EvalFuel,
) -> Result<ArgSpec, CompileError> {
    let fail = |reason: UnsupportedReason| CompileError::UnsupportedArgumentType {
        fun: fun.clone(),
        arg: index,
        reason,
    };
    match arg.ty {
        Term::Pi {
            binder,
            domain,
            codomain,
        } => {
            let gap_head = spine_head(domain.as_ref(), fuel)?;
            let gap_cat = category_of(gap_head).map_err(fail)?;
            let result_head = spine_head(codomain.as_ref(), fuel)?;
            let result_cat = category_of(result_head).map_err(fail)?;
            Ok(ArgSpec::HigherOrder {
                binder: binder.clone(),
                gap_cat,
                result_cat,
            })
        }
        other => {
            let head = spine_head(other, fuel)?;
            let cat = category_of(head).map_err(fail)?;
            Ok(ArgSpec::FirstOrder { cat })
        }
    }
}

/// Reads the shape of one constructor off its declared type.
fn rule_shape(
    fun: &FunName,
    ty: &Term,
    decl: &LinDecl,
    fuel: EvalFuel,
) -> Result<RuleShape, CompileError> {
    let (telescope, result) = strip_telescope(ty, fuel)?;
    let mut args: Vec<ArgSpec> = Vec::new();
    for (position, arg) in telescope.iter().enumerate() {
        args.push(classify_argument(fun, ArgIndex(position), arg, fuel)?);
    }
    let result_head = spine_head(result, fuel)?;
    let result_cat = match category_of(result_head) {
        Ok(name) => name,
        Err(_) => return Err(CompileError::UnsupportedResultType { fun: fun.clone() }),
    };
    Ok(RuleShape {
        fun: fun.clone(),
        args,
        result_cat,
        pos: decl.pos,
    })
}

/// The category whose states an argument ranges over.
///
/// A function argument ranges over the states of its result category only.
/// The state of the missing phrase is read off the record the rule applies
/// the argument to, inside `apply_value`, so the compiler never enumerates it.
fn state_category(spec: &ArgSpec) -> &CatName {
    match spec {
        ArgSpec::FirstOrder { cat } => cat,
        ArgSpec::HigherOrder { result_cat, .. } => result_cat,
    }
}

/// Lists every combination of argument states, argument zero varying slowest.
fn state_combinations(
    shape: &RuleShape,
    states_of_cat: &BTreeMap<CatName, Vec<ParamState>>,
    limits: CompileLimits,
) -> Result<Vec<Vec<ParamState>>, CompileError> {
    let mut per_arg: Vec<&Vec<ParamState>> = Vec::new();
    for spec in &shape.args {
        let cat = state_category(spec);
        match states_of_cat.get(cat) {
            Some(states) => per_arg.push(states),
            None => return Err(CompileError::MissingLinCat { cat: cat.clone() }),
        }
    }
    let total = per_arg
        .iter()
        .try_fold(1usize, |count, list| count.checked_mul(list.len()))
        .unwrap_or(usize::MAX);
    if total > limits.states_per_rule.0 {
        return Err(CompileError::RuleStateExplosion {
            fun: shape.fun.clone(),
            count: total,
            limit: limits.states_per_rule,
        });
    }
    let mut rows: Vec<Vec<ParamState>> = vec![Vec::new()];
    for list in per_arg {
        rows = rows
            .into_iter()
            .flat_map(|prefix| {
                list.iter().map(move |state| {
                    let mut next = prefix.clone();
                    next.push(state.clone());
                    next
                })
            })
            .collect();
    }
    Ok(rows)
}

/// Builds the values the rule body sees for its arguments.
fn argument_environment(
    shape: &RuleShape,
    names: &[VarName],
    combination: &[ParamState],
    lincats: &BTreeMap<CatName, LinType>,
    params: &ParamTypes,
) -> Result<CEnv, CompileError> {
    let mut env = CEnv::default();
    for (position, spec) in shape.args.iter().enumerate() {
        let arg = ArgIndex(position);
        let name = match names.get(position) {
            Some(name) => name,
            None => {
                return Err(CompileError::LinArityMismatch {
                    fun: shape.fun.clone(),
                    expected: shape.args.len(),
                    found: names.len(),
                })
            }
        };
        let state = match combination.get(position) {
            Some(state) => state,
            None => {
                return Err(CompileError::RuleStateExplosion {
                    fun: shape.fun.clone(),
                    count: combination.len(),
                    limit: StateLimit(0),
                })
            }
        };
        let value = match spec {
            ArgSpec::FirstOrder { cat } => {
                let ty = lincats
                    .get(cat)
                    .ok_or_else(|| CompileError::MissingLinCat { cat: cat.clone() })?;
                symbolic_value(ty, params, state, arg, &None).map_err(|error| {
                    CompileError::Evaluation {
                        fun: shape.fun.clone(),
                        error,
                    }
                })?
            }
            ArgSpec::HigherOrder {
                gap_cat,
                result_cat,
                ..
            } => {
                let result = lincats
                    .get(result_cat)
                    .ok_or_else(|| CompileError::MissingLinCat {
                        cat: result_cat.clone(),
                    })?;
                let gap_type = lincats
                    .get(gap_cat)
                    .ok_or_else(|| CompileError::MissingLinCat {
                        cat: gap_cat.clone(),
                    })?;
                CValue::Binder {
                    arg,
                    result: result.clone(),
                    state: state.clone(),
                    gap_cat: gap_cat.clone(),
                    gap_type: gap_type.clone(),
                }
            }
        };
        env = bind(&env, name, Rc::new(value));
    }
    Ok(env)
}

/// A value that should have become words but did not.
enum Leftover {
    /// A function argument the rule never applied to a record.
    Binder(ArgIndex),
    /// A helper the rule never gave enough arguments.
    Function,
}

/// Finds a binder or a helper that survived into the result of a rule.
///
/// This walk runs before `state_of_value`, which would otherwise report a
/// surviving binder as a shape mismatch and hide the real cause.
fn leftover_in_value(value: &CValue, fuel: EvalFuel) -> Result<Option<Leftover>, CompileError> {
    let left = spend(fuel, CompileStage::ResultValue)?;
    match value {
        CValue::Text(_) | CValue::Param(_) => Ok(None),
        CValue::Binder { arg, .. } => Ok(Some(Leftover::Binder(*arg))),
        CValue::Closure { .. } => Ok(Some(Leftover::Function)),
        CValue::Record(fields) => {
            for (_, inner) in fields {
                if let Some(found) = leftover_in_value(inner, left)? {
                    return Ok(Some(found));
                }
            }
            Ok(None)
        }
        CValue::Table(rows) => {
            for (_, inner) in rows {
                if let Some(found) = leftover_in_value(inner, left)? {
                    return Ok(Some(found));
                }
            }
            Ok(None)
        }
    }
}

/// Checks that each argument appears exactly once in one output field.
///
/// The count is by argument index alone. Two slots of one argument at two
/// different paths still count as two, because no context-free production
/// can print one argument twice.
pub fn check_linearity(
    fun: &FunName,
    arity: usize,
    path: &FieldPath,
    items: &[StrItem],
) -> Result<(), CompileError> {
    let mut seen: BTreeMap<ArgIndex, usize> = BTreeMap::new();
    for item in items {
        if let StrItem::Slot(slot) = item {
            let count = seen.entry(slot.arg).or_insert(0);
            *count = count.saturating_add(1);
        }
    }
    for position in 0..arity {
        let arg = ArgIndex(position);
        match seen.get(&arg).copied().unwrap_or(0) {
            0 => {
                return Err(CompileError::ArgumentDropped {
                    fun: fun.clone(),
                    arg,
                    path: path.clone(),
                })
            }
            1 => {}
            _ => {
                return Err(CompileError::ArgumentRepeated {
                    fun: fun.clone(),
                    arg,
                    path: path.clone(),
                })
            }
        }
    }
    Ok(())
}

/// Turns one slot into the nonterminal of the child that fills it.
fn child_nonterminal(
    shape: &RuleShape,
    combination: &[ParamState],
    slot: &Slot,
) -> Result<Nonterminal, CompileError> {
    let dropped = || CompileError::ArgumentDropped {
        fun: shape.fun.clone(),
        arg: slot.arg,
        path: slot.path.clone(),
    };
    let spec = shape.args.get(slot.arg.0).ok_or_else(dropped)?;
    let state = combination.get(slot.arg.0).ok_or_else(dropped)?;
    let cat = match spec {
        ArgSpec::FirstOrder { cat } => cat.clone(),
        ArgSpec::HigherOrder { result_cat, .. } => result_cat.clone(),
    };
    Ok(Nonterminal {
        cat,
        state: state.clone(),
        path: slot.path.clone(),
        gap: slot.gap.clone(),
    })
}

/// Evaluates one rule at one combination of argument states and reads one
/// production off each output field.
#[allow(clippy::too_many_arguments)]
fn productions_for_combination(
    shape: &RuleShape,
    decl: &LinDecl,
    combination: &[ParamState],
    helpers: &Helpers,
    lincats: &BTreeMap<CatName, LinType>,
    paths_of_cat: &BTreeMap<CatName, Vec<FieldPath>>,
    limits: CompileLimits,
    rank: &mut usize,
) -> Result<Vec<RawProduction>, CompileError> {
    let fun = &shape.fun;
    let wrap = |error: ConcreteError| CompileError::Evaluation {
        fun: fun.clone(),
        error,
    };
    let env = argument_environment(
        shape,
        &decl.args,
        combination,
        lincats,
        &helpers.param_types,
    )?;
    let value = eval_cexpr(helpers, &env, &decl.body, limits.rule_fuel).map_err(wrap)?;
    match leftover_in_value(value.as_ref(), limits.walk_fuel)? {
        Some(Leftover::Binder(arg)) => return Err(wrap(ConcreteError::BinderNotApplied { arg })),
        Some(Leftover::Function) => {
            return Err(wrap(ConcreteError::NotAString {
                found: ValueShape::Function,
            }))
        }
        None => {}
    }
    let result_ty = lincats
        .get(&shape.result_cat)
        .ok_or_else(|| CompileError::MissingLinCat {
            cat: shape.result_cat.clone(),
        })?;
    let result_state = match state_of_value(value.as_ref(), result_ty, &helpers.param_types) {
        Ok(state) => state,
        Err(ConcreteError::ShapeMismatch { found }) => {
            return Err(CompileError::ResultShapeMismatch {
                fun: fun.clone(),
                cat: shape.result_cat.clone(),
                found,
            })
        }
        Err(other) => return Err(wrap(other)),
    };
    let mut made: Vec<RawProduction> = Vec::new();
    for path in paths_of_cat.get(&shape.result_cat).into_iter().flatten() {
        let items = text_at_path(value.as_ref(), path).map_err(wrap)?;
        check_linearity(fun, shape.args.len(), path, &items)?;
        let mut rhs: Vec<RawSymbol> = Vec::new();
        for item in &items {
            match item {
                StrItem::Word(token) => rhs.push(RawSymbol::Word(token.clone())),
                StrItem::Slot(slot) => rhs.push(RawSymbol::Child {
                    arg: slot.arg,
                    nonterminal: child_nonterminal(shape, combination, slot)?,
                }),
            }
        }
        let here = *rank;
        *rank = rank.saturating_add(1);
        made.push(RawProduction {
            rank: here,
            lhs: Nonterminal {
                cat: shape.result_cat.clone(),
                state: result_state.clone(),
                path: path.clone(),
                gap: None,
            },
            rhs,
            build: Build::Apply(fun.clone()),
        });
    }
    Ok(made)
}

/// Answers whether every child of a production is in the given set.
fn children_in(production: &RawProduction, set: &BTreeSet<Nonterminal>) -> bool {
    production.rhs.iter().all(|symbol| match symbol {
        RawSymbol::Word(_) => true,
        RawSymbol::Child { nonterminal, .. } => set.contains(nonterminal),
    })
}

/// Drops every production that can never derive a word list.
fn prune_unproductive(
    all: Vec<RawProduction>,
    limits: CompileLimits,
) -> Result<Vec<RawProduction>, CompileError> {
    let mut productive: BTreeSet<Nonterminal> = BTreeSet::new();
    let mut rounds = limits.rounds;
    loop {
        let mut changed = false;
        for production in &all {
            if productive.contains(&production.lhs) {
                continue;
            }
            if children_in(production, &productive) {
                productive.insert(production.lhs.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
        rounds = spend(rounds, CompileStage::Pruning)?;
    }
    Ok(all
        .into_iter()
        .filter(|production| {
            productive.contains(&production.lhs) && children_in(production, &productive)
        })
        .collect())
}

/// Drops every production whose left side no start nonterminal reaches.
fn prune_unreachable(
    all: Vec<RawProduction>,
    start: &[Nonterminal],
    limits: CompileLimits,
) -> Result<Vec<RawProduction>, CompileError> {
    let mut reachable: BTreeSet<Nonterminal> = start.iter().cloned().collect();
    let mut rounds = limits.rounds;
    loop {
        let mut changed = false;
        for production in &all {
            if !reachable.contains(&production.lhs) {
                continue;
            }
            for symbol in &production.rhs {
                if let RawSymbol::Child { nonterminal, .. } = symbol {
                    if reachable.insert(nonterminal.clone()) {
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
        rounds = spend(rounds, CompileStage::Pruning)?;
    }
    Ok(all
        .into_iter()
        .filter(|production| reachable.contains(&production.lhs))
        .collect())
}

/// Computes which nonterminals derive the empty word list.
///
/// Each round marks at least one more nonterminal or stops, so the number of
/// rounds never passes the number of nonterminals plus one.
pub fn compute_nullable(
    nonterminals: &[Nonterminal],
    productions: &[Production],
) -> Result<Vec<bool>, CompileError> {
    let mut nullable = vec![false; nonterminals.len()];
    let bound = nonterminals.len().saturating_add(1);
    for _ in 0..=bound {
        let mut changed = false;
        for production in productions {
            let already = *nullable
                .get(production.lhs.0)
                .ok_or(CompileError::NullableUnstable)?;
            if already {
                continue;
            }
            let mut all_empty = true;
            for symbol in &production.rhs {
                match symbol {
                    Symbol::Word(_) => {
                        all_empty = false;
                        break;
                    }
                    Symbol::Child { nonterminal, .. } => {
                        let child = *nullable
                            .get(nonterminal.0)
                            .ok_or(CompileError::NullableUnstable)?;
                        if !child {
                            all_empty = false;
                            break;
                        }
                    }
                }
            }
            if all_empty {
                match nullable.get_mut(production.lhs.0) {
                    Some(slot) => {
                        *slot = true;
                        changed = true;
                    }
                    None => return Err(CompileError::NullableUnstable),
                }
            }
        }
        if !changed {
            return Ok(nullable);
        }
    }
    Err(CompileError::NullableUnstable)
}

/// Reports a nonterminal that derives itself with no words in between.
///
/// The edge `A -> B` exists when some production of `A` is `B` together with
/// symbols that all derive nothing. A cycle in that graph makes the number of
/// derivations of some sentence unbounded, so the compiler refuses it.
fn unit_cycle_check(grammar: &CompiledGrammar) -> Result<(), CompileError> {
    let count = grammar.nonterminals.len();
    let mut edges: BTreeMap<NonterminalId, BTreeSet<NonterminalId>> = BTreeMap::new();
    for production in &grammar.productions {
        let has_word = production
            .rhs
            .iter()
            .any(|symbol| matches!(symbol, Symbol::Word(_)));
        if has_word {
            continue;
        }
        let children: Vec<NonterminalId> = production
            .rhs
            .iter()
            .filter_map(|symbol| match symbol {
                Symbol::Word(_) => None,
                Symbol::Child { nonterminal, .. } => Some(*nonterminal),
            })
            .collect();
        for candidate in &children {
            let rest_nullable = children.iter().all(|other| {
                other == candidate || grammar.nullable.get(other.0).copied().unwrap_or(false)
            });
            if rest_nullable {
                edges
                    .entry(production.lhs)
                    .or_default()
                    .insert(*candidate);
            }
        }
    }
    // A depth first walk with three colors. Zero: unseen. One: on the current
    // path. Two: finished. Reaching a node colored one closes a cycle.
    let mut color: Vec<u8> = vec![0; count];
    let successors = |id: NonterminalId| -> Vec<NonterminalId> {
        edges
            .get(&id)
            .map(|set| set.iter().rev().copied().collect())
            .unwrap_or_default()
    };
    for root in 0..count {
        if color.get(root).copied().unwrap_or(2) != 0 {
            continue;
        }
        if let Some(slot) = color.get_mut(root) {
            *slot = 1;
        }
        let root_id = NonterminalId(root);
        let mut stack: Vec<(NonterminalId, Vec<NonterminalId>)> =
            vec![(root_id, successors(root_id))];
        let mut steps = count.saturating_mul(count).saturating_add(count).saturating_add(1);
        while let Some((node, pending)) = stack.last_mut() {
            steps = match steps.checked_sub(1) {
                Some(left) => left,
                None => {
                    return Err(CompileError::OutOfFuel {
                        stage: CompileStage::Pruning,
                    })
                }
            };
            match pending.pop() {
                Some(next) => match color.get(next.0).copied() {
                    Some(1) => {
                        return match grammar.nonterminals.get(next.0).cloned() {
                            Some(nonterminal) => Err(CompileError::UnitCycle {
                                cat: nonterminal.cat,
                                state: nonterminal.state,
                                path: nonterminal.path,
                            }),
                            None => Err(CompileError::NullableUnstable),
                        };
                    }
                    Some(0) => {
                        if let Some(slot) = color.get_mut(next.0) {
                            *slot = 1;
                        }
                        stack.push((next, successors(next)));
                    }
                    _ => {}
                },
                None => {
                    let finished = *node;
                    if let Some(slot) = color.get_mut(finished.0) {
                        *slot = 2;
                    }
                    stack.pop();
                }
            }
        }
    }
    Ok(())
}

/// Gives every surviving nonterminal and production a number.
///
/// Nonterminals are numbered in their sorted order and productions in
/// `(left side, rank)` order, so every number is a pure function of the
/// grammar text.
fn intern(
    survivors: Vec<RawProduction>,
    start: &[Nonterminal],
    rules: BTreeMap<FunName, RuleShape>,
    base_count: usize,
    gap_count: usize,
) -> Result<CompiledGrammar, CompileError> {
    let mut sorted = survivors;
    sorted.sort_by(|left, right| (&left.lhs, left.rank).cmp(&(&right.lhs, right.rank)));
    let names: BTreeSet<Nonterminal> = sorted
        .iter()
        .flat_map(|production| {
            std::iter::once(production.lhs.clone()).chain(production.rhs.iter().filter_map(
                |symbol| match symbol {
                    RawSymbol::Word(_) => None,
                    RawSymbol::Child { nonterminal, .. } => Some(nonterminal.clone()),
                },
            ))
        })
        .collect();
    let nonterminals: Vec<Nonterminal> = names.into_iter().collect();
    let index: BTreeMap<&Nonterminal, NonterminalId> = nonterminals
        .iter()
        .enumerate()
        .map(|(position, nonterminal)| (nonterminal, NonterminalId(position)))
        .collect();
    let mut productions: Vec<Production> = Vec::new();
    let mut by_lhs: BTreeMap<NonterminalId, Vec<ProductionId>> = BTreeMap::new();
    let mut words: BTreeSet<Token> = BTreeSet::new();
    for (position, raw) in sorted.into_iter().enumerate() {
        let id = ProductionId(position);
        let lhs = *index.get(&raw.lhs).ok_or(CompileError::NullableUnstable)?;
        let mut rhs: Vec<Symbol> = Vec::new();
        for symbol in raw.rhs {
            match symbol {
                RawSymbol::Word(token) => {
                    words.insert(token.clone());
                    rhs.push(Symbol::Word(token));
                }
                RawSymbol::Child { arg, nonterminal } => {
                    let child = *index
                        .get(&nonterminal)
                        .ok_or(CompileError::NullableUnstable)?;
                    rhs.push(Symbol::Child {
                        arg,
                        nonterminal: child,
                    });
                }
            }
        }
        by_lhs.entry(lhs).or_default().push(id);
        productions.push(Production {
            id,
            lhs,
            rhs,
            build: raw.build,
        });
    }
    let start_ids: Vec<NonterminalId> = start
        .iter()
        .filter_map(|nonterminal| index.get(nonterminal).copied())
        .collect();
    let nullable = compute_nullable(&nonterminals, &productions)?;
    Ok(CompiledGrammar {
        nonterminals,
        productions,
        by_lhs,
        rules,
        start: start_ids,
        words,
        nullable,
        base_count,
        gap_count,
    })
}

/// Turns a concrete module into the parsing grammar.
///
/// Per-rule failures collect into the returned list, so one run reports every
/// bad rule. A failure that concerns the whole grammar stops the run at once.
pub fn compile(
    abstract_grammar: &AbstractGrammar,
    concrete: &ConcreteModule,
    limits: CompileLimits,
) -> Result<CompiledGrammar, Vec<CompileError>> {
    let mut failures: Vec<CompileError> = Vec::new();
    let helpers = match build_helpers(concrete) {
        Ok(helpers) => helpers,
        Err(failure) => return Err(vec![failure]),
    };

    // Linearization types, one per declared category.
    let mut lincats: BTreeMap<CatName, LinType> = BTreeMap::new();
    let known_cats: BTreeSet<&CatName> = abstract_grammar.categories.iter().collect();
    for decl in &concrete.lincats {
        if !known_cats.contains(&decl.cat) {
            failures.push(CompileError::LinCatForUnknownCategory {
                cat: decl.cat.clone(),
            });
            continue;
        }
        if lincats.insert(decl.cat.clone(), decl.ty.clone()).is_some() {
            failures.push(CompileError::DuplicateLinCat {
                cat: decl.cat.clone(),
            });
        }
    }
    for cat in &abstract_grammar.categories {
        if !lincats.contains_key(cat) {
            failures.push(CompileError::MissingLinCat { cat: cat.clone() });
        }
    }

    // Rules, one per declared constructor.
    let mut lins: BTreeMap<FunName, &LinDecl> = BTreeMap::new();
    let known_funs: BTreeSet<&FunName> = abstract_grammar.functions.iter().collect();
    for decl in &concrete.lins {
        if !known_funs.contains(&decl.fun) {
            failures.push(CompileError::LinForUnknownFunction {
                fun: decl.fun.clone(),
            });
            continue;
        }
        if lins.insert(decl.fun.clone(), decl).is_some() {
            failures.push(CompileError::DuplicateLin {
                fun: decl.fun.clone(),
            });
        }
    }
    for fun in &abstract_grammar.functions {
        if !lins.contains_key(fun) {
            failures.push(CompileError::MissingLin { fun: fun.clone() });
        }
    }
    if !failures.is_empty() {
        return Err(failures);
    }

    // String paths and parameter states, one list per category.
    let mut paths_of_cat: BTreeMap<CatName, Vec<FieldPath>> = BTreeMap::new();
    let mut states_of_cat: BTreeMap<CatName, Vec<ParamState>> = BTreeMap::new();
    for (cat, ty) in &lincats {
        match string_paths(ty, &helpers.param_types) {
            Ok(paths) if paths.is_empty() => {
                failures.push(CompileError::CategoryPrintsNothing { cat: cat.clone() })
            }
            Ok(paths) => {
                paths_of_cat.insert(cat.clone(), paths);
            }
            Err(failure) => failures.push(failure),
        }
        match enumerate_states(cat, ty, &helpers.param_types, limits.states_per_category) {
            Ok(states) => {
                states_of_cat.insert(cat.clone(), states);
            }
            Err(failure) => failures.push(failure),
        }
    }
    if !failures.is_empty() {
        return Err(failures);
    }

    // The base productions, in declaration order.
    let mut rules: BTreeMap<FunName, RuleShape> = BTreeMap::new();
    let mut base: Vec<RawProduction> = Vec::new();
    let mut rank: usize = 0;
    for fun in &abstract_grammar.functions {
        let decl = match lins.get(fun) {
            Some(decl) => *decl,
            None => continue,
        };
        let ty = match abstract_grammar
            .signature
            .types
            .get(&ConstName::Function(fun.clone()))
        {
            Some(ty) => ty,
            None => {
                failures.push(CompileError::MissingLin { fun: fun.clone() });
                continue;
            }
        };
        let shape = match rule_shape(fun, ty, decl, limits.walk_fuel) {
            Ok(shape) => shape,
            Err(failure) => {
                failures.push(failure);
                continue;
            }
        };
        if decl.args.len() != shape.args.len() {
            failures.push(CompileError::LinArityMismatch {
                fun: fun.clone(),
                expected: shape.args.len(),
                found: decl.args.len(),
            });
            continue;
        }
        let combinations = match state_combinations(&shape, &states_of_cat, limits) {
            Ok(rows) => rows,
            Err(failure) => {
                failures.push(failure);
                continue;
            }
        };
        let mut rule_failed = false;
        for combination in &combinations {
            match productions_for_combination(
                &shape,
                decl,
                combination,
                &helpers,
                &lincats,
                &paths_of_cat,
                limits,
                &mut rank,
            ) {
                Ok(mut made) => base.append(&mut made),
                Err(failure) => {
                    failures.push(failure);
                    rule_failed = true;
                    break;
                }
            }
        }
        if rule_failed {
            continue;
        }
        rules.insert(fun.clone(), shape);
    }
    if !failures.is_empty() {
        return Err(failures);
    }
    if base.len() > limits.productions.0 {
        return Err(vec![CompileError::TooManyProductions {
            count: base.len(),
            limit: limits.productions,
        }]);
    }
    let base_count = base.len();

    // Threading the missing phrases.
    let gaps = match collect_gaps(&base, limits) {
        Ok(gaps) => gaps,
        Err(failure) => return Err(vec![failure]),
    };
    let gap_count = gaps.len();
    let mut all = base;
    let mut copies = pass_down(&all, &gaps, &mut rank);
    all.append(&mut copies);
    let mut sites = gap_sites(&gaps, &paths_of_cat, &mut rank);
    all.append(&mut sites);
    if all.len() > limits.productions.0 {
        return Err(vec![CompileError::TooManyProductions {
            count: all.len(),
            limit: limits.productions,
        }]);
    }

    // The start nonterminals.
    let start_cat = &abstract_grammar.start;
    let mut start: Vec<Nonterminal> = Vec::new();
    for state in states_of_cat.get(start_cat).into_iter().flatten() {
        for path in paths_of_cat.get(start_cat).into_iter().flatten() {
            start.push(Nonterminal {
                cat: start_cat.clone(),
                state: state.clone(),
                path: path.clone(),
                gap: None,
            });
        }
    }

    // Pruning, numbering, and the last two checks.
    let productive = match prune_unproductive(all, limits) {
        Ok(kept) => kept,
        Err(failure) => return Err(vec![failure]),
    };
    let reachable = match prune_unreachable(productive, &start, limits) {
        Ok(kept) => kept,
        Err(failure) => return Err(vec![failure]),
    };
    let grammar = match intern(reachable, &start, rules, base_count, gap_count) {
        Ok(grammar) => grammar,
        Err(failure) => return Err(vec![failure]),
    };
    if let Err(failure) = unit_cycle_check(&grammar) {
        return Err(vec![failure]);
    }
    Ok(grammar)
}
