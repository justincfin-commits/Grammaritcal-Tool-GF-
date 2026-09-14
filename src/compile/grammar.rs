//! The compiled context-free grammar.
//!
//! This module holds data only. `crate::compile::productions` builds it,
//! `crate::compile::gaps` adds the productions that thread a missing phrase,
//! and `crate::parse` reads it.
//!
//! A nonterminal is the four part key the design names: the category, the
//! parameter state, the field path, and the missing phrase. A production has
//! one nonterminal on the left and a list of words and child nonterminals on
//! the right. Each child names which argument of the constructor it fills, so
//! the tree rebuilder can put the words back in argument order.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use crate::concrete::lintype::{FieldPath, ParamState};
use crate::concrete::value::GapSpec;
use crate::error::SourcePos;
use crate::mltt::term::Binder;
use crate::newtype::{
    ArgIndex, CatName, EvalFuel, FunName, GapLimit, NonterminalId, ProductionId, ProductionLimit,
    StateLimit, Token,
};

/// One nonterminal of the compiled grammar.
///
/// The gap holds at most one missing phrase. A phrase that misses two phrases
/// cannot be named, so the limit "at most one gap" is enforced by this type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nonterminal {
    pub cat: CatName,
    pub state: ParamState,
    pub path: FieldPath,
    pub gap: Option<GapSpec>,
}

/// One item on the right of a finished production.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Symbol {
    /// A word the grammar writes down. The parser matches it against one token.
    Word(Token),
    /// The words of one argument. `arg` says which argument of the constructor.
    Child {
        arg: ArgIndex,
        nonterminal: NonterminalId,
    },
}

/// What the tree rebuilder writes when it finishes a production.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Build {
    /// Apply this constructor to the subtree of each child, in argument order.
    Apply(FunName),
    /// This is the place where the missing phrase would have stood. The
    /// rebuilder writes the bound variable here.
    GapSite,
}

/// One compiled production.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Production {
    pub id: ProductionId,
    pub lhs: NonterminalId,
    pub rhs: Vec<Symbol>,
    pub build: Build,
}

/// One item on the right of a production before nonterminals get numbers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RawSymbol {
    Word(Token),
    Child {
        arg: ArgIndex,
        nonterminal: Nonterminal,
    },
}

/// A production before nonterminals get numbers. Generation and pruning work
/// on this form. Interning turns it into a `Production`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawProduction {
    /// The order in which the compiler made this production. Sorting by
    /// `(lhs, rank)` keeps declaration order inside each nonterminal.
    pub rank: usize,
    pub lhs: Nonterminal,
    pub rhs: Vec<RawSymbol>,
    pub build: Build,
}

/// What one argument of a constructor is, after the compiler reads its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArgSpec {
    /// A phrase of one category.
    FirstOrder { cat: CatName },
    /// A function from one category to another. The parser builds it from a
    /// phrase of `result_cat` that misses one phrase of `gap_cat`, and the
    /// rebuilder wraps that phrase in a lambda that writes down `binder`.
    HigherOrder {
        binder: Binder,
        gap_cat: CatName,
        result_cat: CatName,
    },
}

/// What the compiler read off one constructor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleShape {
    pub fun: FunName,
    pub args: Vec<ArgSpec>,
    pub result_cat: CatName,
    /// Where the `lin` rule stands in the concrete file.
    pub pos: SourcePos,
}

/// How large the compiler lets the compiled grammar grow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileLimits {
    /// How many parameter states one category may carry.
    pub states_per_category: StateLimit,
    /// How many combinations of argument states one rule may fix.
    pub states_per_rule: StateLimit,
    /// How many productions the grammar may hold.
    pub productions: ProductionLimit,
    /// How many distinct missing phrases the compiler may thread.
    pub gaps: GapLimit,
    /// The depth budget for evaluating one rule body.
    pub rule_fuel: EvalFuel,
    /// The depth budget for walking one type or one value.
    pub walk_fuel: EvalFuel,
    /// How many rounds each fixed point may run.
    pub rounds: EvalFuel,
}

/// The finished parsing grammar.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CompiledGrammar {
    /// Position `i` holds the nonterminal named by `NonterminalId(i)`.
    pub nonterminals: Vec<Nonterminal>,
    /// Position `i` holds the production named by `ProductionId(i)`.
    pub productions: Vec<Production>,
    /// Every production of one nonterminal, in production order.
    pub by_lhs: BTreeMap<NonterminalId, Vec<ProductionId>>,
    /// What the compiler read off each constructor.
    pub rules: BTreeMap<FunName, RuleShape>,
    /// Where a whole sentence must land.
    pub start: Vec<NonterminalId>,
    /// Every word any production prints.
    pub words: BTreeSet<Token>,
    /// Position `i` says whether `NonterminalId(i)` derives the empty word list.
    pub nullable: Vec<bool>,
    /// How many productions the compiler made before threading gaps.
    pub base_count: usize,
    /// How many distinct missing phrases the compiler threaded.
    pub gap_count: usize,
}
