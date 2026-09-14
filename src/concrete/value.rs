//! Values of the linearization language.
//!
//! This module holds data only. `crate::concrete::eval` builds these values
//! and `crate::compile` reads them.
//!
//! A word list holds two kinds of item. A word is a literal that the grammar
//! writes down. A slot marks the place where the words of one argument belong.
//! Printing a finished tree produces word lists that hold words only. Compiling
//! a rule produces word lists that hold slots, and each slot becomes one symbol
//! on the right of a context-free production.

#![allow(missing_docs)]

use std::rc::Rc;

use crate::concrete::lintype::{FieldPath, LinType, ParamState};
use crate::gf::ast::CExpr;
use crate::newtype::{ArgIndex, CatName, FieldName, ParamValue, Token, VarName};

/// The category and the feature values of a phrase that a rule leaves out.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GapSpec {
    /// The category of the missing phrase.
    pub cat: CatName,
    /// The feature values the missing phrase carries.
    pub state: ParamState,
}

/// The place where the words of one argument belong.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot {
    /// Which argument fills this place.
    pub arg: ArgIndex,
    /// Which part of that argument fills this place.
    pub path: FieldPath,
    /// The phrase that argument leaves out, when the argument is a function.
    pub gap: Option<GapSpec>,
}

/// One item of a word list.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StrItem {
    /// A word the grammar writes down.
    Word(Token),
    /// The place where the words of one argument belong.
    Slot(Slot),
}

/// A value of the linearization language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CValue {
    /// A word list.
    Text(Vec<StrItem>),
    /// A parameter value.
    Param(ParamValue),
    /// A record.
    Record(Vec<(FieldName, CValue)>),
    /// A table.
    Table(Vec<(ParamValue, CValue)>),
    /// A helper that still waits for arguments.
    Closure {
        names: Vec<VarName>,
        body: Rc<CExpr>,
        env: CEnv,
    },
    /// A higher-order argument that still waits for its bound variable.
    Binder {
        arg: ArgIndex,
        result: LinType,
        state: ParamState,
        gap_cat: CatName,
        gap_type: LinType,
    },
}

/// The values that the names in scope carry.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CEnv(pub Vec<(VarName, Rc<CValue>)>);
