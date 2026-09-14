//! Chart and forest data for the Earley parser and the tree rebuilder.
//!
//! This module holds data only. `crate::parse::earley` builds the tables, the
//! chart, and the forest. `crate::parse::forest` reads the forest and builds
//! terms.
//!
//! Every container here is a `Vec`, a `BTreeMap`, or a `BTreeSet`, so that
//! iteration order is a pure function of the contents. That is what makes two
//! runs on the same request produce the same bytes.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use crate::compile::grammar::CompiledGrammar;
use crate::error::TypeError;
use crate::mltt::signature::Signature;
use crate::mltt::term::Term;
use crate::newtype::{
    ChartPos, DotPos, EvalFuel, ItemSlot, NonterminalId, ProductionId, RejectionLimit, TerminalId,
    Token,
};

/// Everything the parser reads about a grammar, indexed for lookup.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ParseTables {
    pub grammar: CompiledGrammar,
    /// Position `i` holds the word named by `TerminalId(i)`.
    pub terminals: Vec<Token>,
    /// For each grammar word, its place in the sorted list.
    pub terminal_index: BTreeMap<Token, TerminalId>,
}

/// One place the parser has reached inside one production.
///
/// The left side is `productions[production].lhs`. The end position is the
/// column the item sits in, so the item does not carry it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EarleyItem {
    pub production: ProductionId,
    pub dot: DotPos,
    pub origin: ChartPos,
}

/// What the dot moved over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StepKind {
    /// The dot moved over a grammar word.
    Scanned,
    /// The dot moved over a nonterminal.
    Completed,
}

/// One way the dot reached its place. `split` is the chart position where
/// the symbol the dot moved over begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step {
    pub kind: StepKind,
    pub split: ChartPos,
}

/// One item, together with every way the dot reached it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemRecord {
    pub item: EarleyItem,
    /// Sorted ascending and deduplicated. Empty when `item.dot` is zero.
    pub steps: Vec<Step>,
}

/// Everything the parser knows at one position between two words.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ChartColumn {
    /// In the order the parser added them. This order fixes every later order.
    pub records: Vec<ItemRecord>,
    /// Where each item sits in `records`.
    pub index: BTreeMap<EarleyItem, ItemSlot>,
    /// Which nonterminals this column has already predicted.
    pub predicted: BTreeSet<NonterminalId>,
}

/// One column per position, so `columns.len()` is `tokens.len() + 1`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Chart {
    pub columns: Vec<ChartColumn>,
}

/// One nonterminal covering one stretch of the sentence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpanKey {
    pub symbol: NonterminalId,
    pub start: ChartPos,
    pub end: ChartPos,
}

/// One place inside one production, over one stretch of the sentence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DotKey {
    pub production: ProductionId,
    pub dot: DotPos,
    pub origin: ChartPos,
    pub end: ChartPos,
}

/// Every derivation the chart found, shared and packed.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Forest {
    /// Which productions can build each span. Values ascend by production id.
    pub spans: BTreeMap<SpanKey, Vec<ProductionId>>,
    /// How the dot reached each place. Values ascend by `(kind, split)`.
    pub dots: BTreeMap<DotKey, Vec<Step>>,
    /// The spans that cover the whole sentence at a start nonterminal.
    pub roots: Vec<SpanKey>,
}

/// One rebuilt subtree, together with how many bound-variable leaves it holds
/// that no lambda inside it binds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Built {
    pub term: Term,
    /// Zero or one. Two or more is a defect and raises an error.
    pub traces: usize,
}

/// One candidate tree that the type checker refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejection {
    pub tree: Term,
    pub error: TypeError,
}

/// What the rebuilder needs to type check closed subtrees as it builds them.
///
/// A closed subtree carries no free variable, so its type does not depend on
/// where it stands. Refusing it early is therefore safe, and it keeps the
/// number of candidates small when several arguments print nothing.
#[derive(Clone, Copy, Debug)]
pub struct TypeFilter<'a> {
    pub signature: &'a Signature,
    pub fuel: EvalFuel,
    pub keep: RejectionLimit,
}

/// The trees one sentence yields.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Enumeration {
    /// Sorted and deduplicated. When a filter ran, every term here type checks.
    pub terms: Vec<Term>,
    /// True when some span reached the tree limit and the walk stopped early.
    pub truncated: bool,
    /// How many derivations the walk cut because a span repeated on one path.
    pub cycles_cut: usize,
    /// The first few subtrees the filter refused, in the order it met them.
    pub rejected: Vec<Rejection>,
    /// How many subtrees the filter refused in all.
    pub rejected_total: usize,
}
