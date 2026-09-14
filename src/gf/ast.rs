//! The shape of a grammar source file.
//!
//! This module holds data only. `crate::gf::parser` builds it,
//! `crate::gf::lower` turns the abstract half into signature entries, and
//! `crate::compile` turns the concrete half into parsing rules.

#![allow(missing_docs)]

use crate::concrete::lintype::{LinType, ParamTypeDecl};
use crate::error::SourcePos;
use crate::newtype::{
    CatName, FieldName, FunName, ModuleName, OperName, ParamValue, Token, UniverseLevel, VarName,
};

/// A type or a term as the source file writes it, with names still in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SExpr {
    /// A name, which stands for a bound variable or for a declared constant.
    Name(String),
    /// A universe.
    Universe(UniverseLevel),
    /// A dependent function type. One binder may name several variables.
    Pi {
        names: Vec<VarName>,
        domain: Box<SExpr>,
        codomain: Box<SExpr>,
    },
    /// A function.
    Lam {
        names: Vec<VarName>,
        body: Box<SExpr>,
    },
    /// An application.
    App {
        function: Box<SExpr>,
        argument: Box<SExpr>,
    },
    /// A dependent pair type.
    Sigma {
        names: Vec<VarName>,
        first: Box<SExpr>,
        second: Box<SExpr>,
    },
    /// A pair.
    Pair {
        first: Box<SExpr>,
        second: Box<SExpr>,
    },
    /// The first projection.
    First(Box<SExpr>),
    /// The second projection.
    Second(Box<SExpr>),
    /// An identity type.
    Id {
        ty: Box<SExpr>,
        left: Box<SExpr>,
        right: Box<SExpr>,
    },
    /// A reflexive proof.
    Refl {
        ty: Box<SExpr>,
        point: Box<SExpr>,
    },
    /// An identity elimination.
    Eliminate {
        level: UniverseLevel,
        ty: Box<SExpr>,
        left: Box<SExpr>,
        motive: Box<SExpr>,
        base: Box<SExpr>,
        right: Box<SExpr>,
        proof: Box<SExpr>,
    },
}

/// A category declaration, such as `cat Ind (s : Sort)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatDecl {
    pub name: CatName,
    pub args: Vec<(VarName, SExpr)>,
    pub pos: SourcePos,
}

/// A constructor declaration, such as `fun Barks : Ind Animate -> Prop`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunDecl {
    pub name: FunName,
    pub ty: SExpr,
    pub pos: SourcePos,
}

/// The abstract half of a grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbstractModule {
    pub name: ModuleName,
    pub start: Option<CatName>,
    pub cats: Vec<CatDecl>,
    pub funs: Vec<FunDecl>,
}

/// A pattern in a table row or a case branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CPattern {
    /// One named parameter value.
    Value(ParamValue),
    /// Every remaining parameter value.
    Wildcard,
}

/// A linearization expression as the source file writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CExpr {
    /// A name, which stands for an argument, a helper, or a parameter value.
    Name(String),
    /// A word list.
    Text(Vec<Token>),
    /// The empty word list.
    Empty,
    /// Two word lists one after the other.
    Join(Box<CExpr>, Box<CExpr>),
    /// A record.
    Record(Vec<(FieldName, CExpr)>),
    /// A field of a record.
    Project(Box<CExpr>, FieldName),
    /// A table, which holds one value per parameter value.
    Table(Vec<(CPattern, CExpr)>),
    /// One row of a table.
    Select(Box<CExpr>, Box<CExpr>),
    /// A choice among parameter values.
    Case(Box<CExpr>, Vec<(CPattern, CExpr)>),
    /// A helper function.
    Lam(Vec<VarName>, Box<CExpr>),
    /// An application.
    App(Box<CExpr>, Box<CExpr>),
}

/// A linearization type declaration, such as `lincat Prop = { s : Str }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinCatDecl {
    pub cat: CatName,
    pub ty: LinType,
    pub pos: SourcePos,
}

/// A linearization rule, such as `lin Barks x = { s = x.s ++ "barks" }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinDecl {
    pub fun: FunName,
    pub args: Vec<VarName>,
    pub body: CExpr,
    pub pos: SourcePos,
}

/// A helper declaration, such as `oper noun s = { s = s }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperDecl {
    pub name: OperName,
    pub args: Vec<VarName>,
    pub body: CExpr,
    pub pos: SourcePos,
}

/// The concrete half of a grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConcreteModule {
    pub name: ModuleName,
    pub abstract_name: ModuleName,
    pub params: Vec<ParamTypeDecl>,
    pub lincats: Vec<LinCatDecl>,
    pub lins: Vec<LinDecl>,
    pub opers: Vec<OperDecl>,
}
