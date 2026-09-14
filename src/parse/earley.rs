//! An Earley chart parser that records how each item completed.
//!
//! This module holds behavior only. It contains no recursion, so it takes no
//! fuel. Every loop has a bound written next to it.
//!
//! The grammar has nullable nonterminals: a category such as `Sort` prints
//! nothing, and the place where a bound variable stands matches no words. A
//! plain Earley completer misses items whose next symbol is a nullable
//! nonterminal completed later in the same column. The fix here is the one
//! Aycock and Horspool describe: every insertion advances the dot over every
//! nullable nonterminal at once, so the result never depends on the order in
//! which items arrive.

use std::collections::BTreeMap;

use crate::compile::grammar::{CompiledGrammar, Symbol};
use crate::error::ReadError;
use crate::newtype::{ChartPos, DotPos, ItemLimit, ItemSlot, NonterminalId, TerminalId, Token};
use crate::parse::chart::{
    Chart, ChartColumn, DotKey, EarleyItem, Forest, ItemRecord, ParseTables, SpanKey, Step,
    StepKind,
};
use crate::parse::input::{InputToken, MatchKind, ResolvedToken};

/// Builds the lookup tables the parser reads.
pub fn build_tables(grammar: &CompiledGrammar) -> ParseTables {
    let terminals: Vec<Token> = grammar.words.iter().cloned().collect();
    let terminal_index: BTreeMap<Token, TerminalId> = terminals
        .iter()
        .enumerate()
        .map(|(position, token)| (token.clone(), TerminalId(position)))
        .collect();
    ParseTables {
        grammar: grammar.clone(),
        terminals,
        terminal_index,
    }
}

/// Finds each input word in the grammar.
///
/// The exact spelling wins. When the grammar does not hold it, the spelling
/// with every capital turned small is tried. Folding never goes the other
/// way, so a grammar word written `MUST` matches input `MUST` only.
pub fn resolve(tables: &ParseTables, tokens: &[InputToken]) -> Result<Vec<ResolvedToken>, ReadError> {
    let mut out: Vec<ResolvedToken> = Vec::new();
    for token in tokens {
        if let Some(id) = tables.terminal_index.get(&token.surface) {
            out.push(ResolvedToken {
                terminal: *id,
                kind: MatchKind::Exact,
                span: token.span,
            });
        } else if let Some(id) = tables.terminal_index.get(&token.folded) {
            out.push(ResolvedToken {
                terminal: *id,
                kind: MatchKind::Folded,
                span: token.span,
            });
        } else {
            let shouted = Token(token.surface.0.to_ascii_uppercase());
            let mut capitalized = String::new();
            for (position, c) in token.folded.0.chars().enumerate() {
                if position == 0 {
                    capitalized.extend(c.to_uppercase());
                } else {
                    capitalized.push(c);
                }
            }
            let capitalized = Token(capitalized);
            let other = [shouted, capitalized]
                .into_iter()
                .find(|candidate| tables.terminal_index.contains_key(candidate));
            return Err(match other {
                Some(grammar_spelling) => ReadError::UnknownWordCaseOnly {
                    token: token.surface.clone(),
                    grammar_spelling,
                },
                None => ReadError::UnknownWord {
                    token: token.surface.clone(),
                },
            });
        }
    }
    Ok(out)
}

/// Reads the symbol after the dot, or nothing when the item is complete.
fn next_symbol(tables: &ParseTables, item: EarleyItem) -> Result<Option<Symbol>, ReadError> {
    let production = tables
        .grammar
        .productions
        .get(item.production.0)
        .ok_or(ReadError::BrokenDerivation)?;
    Ok(production.rhs.get(item.dot.0).cloned())
}

/// Puts one item into a column, records how the dot reached it, and then
/// advances the dot over every nonterminal that derives nothing.
///
/// Every insertion in the parser goes through this function. That is the
/// property the completeness argument rests on: an item whose next symbol is
/// nullable is advanced at the moment it arrives, whether or not the nullable
/// symbol has completed yet.
fn add_item(
    column: &mut ChartColumn,
    here: ChartPos,
    item: EarleyItem,
    step: Option<Step>,
    tables: &ParseTables,
    budget: ItemLimit,
) -> Result<(), ReadError> {
    let production = tables
        .grammar
        .productions
        .get(item.production.0)
        .ok_or(ReadError::BrokenDerivation)?;
    let mut walking = item;
    let mut arriving = step;
    // The dot moves forward every turn and never passes the end of the right
    // side, so this loop runs at most `rhs.len() + 1` times.
    for _ in 0..=production.rhs.len() {
        let fresh = match column.index.get(&walking) {
            Some(slot) => {
                let record = column
                    .records
                    .get_mut(slot.0)
                    .ok_or(ReadError::BrokenDerivation)?;
                if let Some(one) = arriving {
                    if let Err(at) = record.steps.binary_search(&one) {
                        record.steps.insert(at, one);
                    }
                }
                false
            }
            None => {
                if column.records.len() >= budget.0 {
                    return Err(ReadError::ChartTooLarge {
                        position: here,
                        limit: budget,
                    });
                }
                let steps = match arriving {
                    Some(one) => vec![one],
                    None => Vec::new(),
                };
                column
                    .index
                    .insert(walking, ItemSlot(column.records.len()));
                column.records.push(ItemRecord {
                    item: walking,
                    steps,
                });
                true
            }
        };
        // An item that was already present already ran its own chain.
        if !fresh {
            return Ok(());
        }
        match next_symbol(tables, walking)? {
            Some(Symbol::Child { nonterminal, .. })
                if tables
                    .grammar
                    .nullable
                    .get(nonterminal.0)
                    .copied()
                    .unwrap_or(false) =>
            {
                walking = EarleyItem {
                    production: walking.production,
                    dot: DotPos(walking.dot.0.saturating_add(1)),
                    origin: walking.origin,
                };
                arriving = Some(Step {
                    kind: StepKind::Completed,
                    split: here,
                });
            }
            _ => return Ok(()),
        }
    }
    Err(ReadError::BrokenDerivation)
}

/// Adds every production of a nonterminal at dot zero, once per column.
fn predict(
    chart: &mut Chart,
    tables: &ParseTables,
    k: usize,
    child: NonterminalId,
    budget: ItemLimit,
) -> Result<(), ReadError> {
    let column = chart.columns.get_mut(k).ok_or(ReadError::BrokenDerivation)?;
    if !column.predicted.insert(child) {
        return Ok(());
    }
    let list = tables.grammar.by_lhs.get(&child).cloned().unwrap_or_default();
    for production in list {
        add_item(
            column,
            ChartPos(k),
            EarleyItem {
                production,
                dot: DotPos(0),
                origin: ChartPos(k),
            },
            None,
            tables,
            budget,
        )?;
    }
    Ok(())
}

/// Advances every item that was waiting for a finished nonterminal.
fn complete(
    chart: &mut Chart,
    tables: &ParseTables,
    k: usize,
    item: EarleyItem,
    budget: ItemLimit,
) -> Result<(), ReadError> {
    let lhs = tables
        .grammar
        .productions
        .get(item.production.0)
        .ok_or(ReadError::BrokenDerivation)?
        .lhs;
    let j = item.origin.0;
    // Copy the waiting items out first. That removes every borrow conflict
    // between reading column `j` and writing column `k`, including `j == k`.
    let waiting: Vec<EarleyItem> = {
        let source = chart.columns.get(j).ok_or(ReadError::BrokenDerivation)?;
        let mut found: Vec<EarleyItem> = Vec::new();
        for record in &source.records {
            if let Some(Symbol::Child { nonterminal, .. }) = next_symbol(tables, record.item)? {
                if nonterminal == lhs {
                    found.push(EarleyItem {
                        production: record.item.production,
                        dot: DotPos(record.item.dot.0.saturating_add(1)),
                        origin: record.item.origin,
                    });
                }
            }
        }
        found
    };
    let target = chart.columns.get_mut(k).ok_or(ReadError::BrokenDerivation)?;
    for advanced in waiting {
        add_item(
            target,
            ChartPos(k),
            advanced,
            Some(Step {
                kind: StepKind::Completed,
                split: ChartPos(j),
            }),
            tables,
            budget,
        )?;
    }
    Ok(())
}

/// Reads one sentence into a chart.
pub fn parse(
    tables: &ParseTables,
    tokens: &[ResolvedToken],
    budget: ItemLimit,
) -> Result<Chart, ReadError> {
    let n = tokens.len();
    let mut chart = Chart {
        columns: vec![ChartColumn::default(); n.saturating_add(1)],
    };
    // Seed column zero with every production of every start nonterminal.
    for symbol in &tables.grammar.start {
        predict(&mut chart, tables, 0, *symbol, budget)?;
    }
    for k in 0..=n {
        // The work list. `at` only moves forward and `records` only grows,
        // so the loop stops when `at` catches the end.
        let mut at: usize = 0;
        loop {
            let item = {
                let column = chart.columns.get(k).ok_or(ReadError::BrokenDerivation)?;
                match column.records.get(at) {
                    None => break,
                    Some(record) => record.item,
                }
            };
            match next_symbol(tables, item)? {
                None => complete(&mut chart, tables, k, item, budget)?,
                Some(Symbol::Child { nonterminal, .. }) => {
                    predict(&mut chart, tables, k, nonterminal, budget)?
                }
                Some(Symbol::Word(_)) => {}
            }
            at = at.saturating_add(1);
        }
        // Scanning writes into column `k + 1`, which nothing has touched yet.
        if k < n {
            let word = tokens.get(k).ok_or(ReadError::BrokenDerivation)?.terminal;
            let wanted = tables
                .terminals
                .get(word.0)
                .ok_or(ReadError::BrokenDerivation)?;
            let advanced: Vec<EarleyItem> = {
                let column = chart.columns.get(k).ok_or(ReadError::BrokenDerivation)?;
                let mut found: Vec<EarleyItem> = Vec::new();
                for record in &column.records {
                    if let Some(Symbol::Word(token)) = next_symbol(tables, record.item)? {
                        if &token == wanted {
                            found.push(EarleyItem {
                                production: record.item.production,
                                dot: DotPos(record.item.dot.0.saturating_add(1)),
                                origin: record.item.origin,
                            });
                        }
                    }
                }
                found
            };
            let next = chart
                .columns
                .get_mut(k.saturating_add(1))
                .ok_or(ReadError::BrokenDerivation)?;
            for item in advanced {
                add_item(
                    next,
                    ChartPos(k.saturating_add(1)),
                    item,
                    Some(Step {
                        kind: StepKind::Scanned,
                        split: ChartPos(k),
                    }),
                    tables,
                    budget,
                )?;
            }
        }
    }
    Ok(chart)
}

/// Names the position the parser got furthest to, and the words it could
/// have accepted there.
fn furthest(tables: &ParseTables, chart: &Chart) -> Result<ReadError, ReadError> {
    let mut best: Option<(usize, Vec<Token>)> = None;
    for (k, column) in chart.columns.iter().enumerate() {
        if column.records.is_empty() {
            continue;
        }
        let mut expected: Vec<Token> = Vec::new();
        for record in &column.records {
            if let Some(Symbol::Word(token)) = next_symbol(tables, record.item)? {
                if !expected.contains(&token) {
                    expected.push(token);
                }
            }
        }
        expected.sort();
        best = Some((k, expected));
    }
    Ok(match best {
        Some((position, expected)) => ReadError::NoParseAt {
            position: ChartPos(position),
            expected,
        },
        None => ReadError::NoParse,
    })
}

/// Turns a finished chart into the packed forest of every derivation.
pub fn build_forest(tables: &ParseTables, chart: &Chart, length: usize) -> Result<Forest, ReadError> {
    let mut forest = Forest::default();
    for (k, column) in chart.columns.iter().enumerate() {
        for record in &column.records {
            let production = tables
                .grammar
                .productions
                .get(record.item.production.0)
                .ok_or(ReadError::BrokenDerivation)?;
            forest.dots.insert(
                DotKey {
                    production: record.item.production,
                    dot: record.item.dot,
                    origin: record.item.origin,
                    end: ChartPos(k),
                },
                record.steps.clone(),
            );
            if record.item.dot.0 == production.rhs.len() {
                let key = SpanKey {
                    symbol: production.lhs,
                    start: record.item.origin,
                    end: ChartPos(k),
                };
                let list = forest.spans.entry(key).or_default();
                if let Err(at) = list.binary_search(&record.item.production) {
                    list.insert(at, record.item.production);
                }
            }
        }
    }
    for symbol in &tables.grammar.start {
        let key = SpanKey {
            symbol: *symbol,
            start: ChartPos(0),
            end: ChartPos(length),
        };
        if forest.spans.contains_key(&key) {
            forest.roots.push(key);
        }
    }
    forest.roots.sort();
    if forest.roots.is_empty() {
        return Err(furthest(tables, chart)?);
    }
    Ok(forest)
}
