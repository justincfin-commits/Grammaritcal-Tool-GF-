//! Core term data for Martin-Lof type theory.
//!
//! This module holds data only. `eval`, `check`, and `render` hold the behavior
//! that reads this data.
//!
//! A term carries de Bruijn indices. `Term::Var(DeBruijnIndex(0))` names the
//! nearest enclosing binder. The derived `PartialEq` compares two terms letter
//! by letter, including binder names. It does not decide judgmental equality.
//! `crate::mltt::eval::convertible` decides that.

use crate::newtype::{CatName, DeBruijnIndex, FunName, UniverseLevel, VarName};

/// The name of a constant that the grammar signature declares.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConstName {
    /// A category that a `cat` declaration introduces.
    Category(CatName),
    /// A constructor that a `fun` declaration introduces.
    Function(FunName),
}

/// The name a binder writes down. Term equality compares this name. Conversion
/// checking ignores it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Binder(pub VarName);

/// One term of the core type theory. Types and terms share this type, because
/// Martin-Lof type theory places both in one syntactic class.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    /// A bound variable, counted from the nearest binder.
    Var(DeBruijnIndex),
    /// A constant that the signature declares.
    Const(ConstName),
    /// A universe. `Universe(n)` inhabits `Universe(n + 1)`.
    Universe(UniverseLevel),
    /// A dependent function type, written `(x : A) -> B`.
    Pi {
        /// The name the source file gives the bound variable.
        binder: Binder,
        /// The type of the argument.
        domain: Box<Term>,
        /// The result type, which may mention the argument.
        codomain: Box<Term>,
    },
    /// A function.
    Lam {
        /// The name the source file gives the bound variable.
        binder: Binder,
        /// The body of the function.
        body: Box<Term>,
    },
    /// An application.
    App {
        /// The function.
        function: Box<Term>,
        /// The argument.
        argument: Box<Term>,
    },
    /// A dependent pair type.
    Sigma {
        /// The name the source file gives the bound variable.
        binder: Binder,
        /// The type of the first component.
        first: Box<Term>,
        /// The type of the second component, which may mention the first.
        second: Box<Term>,
    },
    /// A pair.
    Pair {
        /// The first component.
        first: Box<Term>,
        /// The second component.
        second: Box<Term>,
    },
    /// The first projection of a pair.
    First(Box<Term>),
    /// The second projection of a pair.
    Second(Box<Term>),
    /// The identity type, which states that the left side equals the right side.
    Id {
        /// The type both sides inhabit.
        ty: Box<Term>,
        /// The left side.
        left: Box<Term>,
        /// The right side.
        right: Box<Term>,
    },
    /// The proof that a point equals itself.
    Refl {
        /// The type the point inhabits.
        ty: Box<Term>,
        /// The point.
        point: Box<Term>,
    },
    /// The eliminator of the identity type.
    ///
    /// The eliminator takes a type A, a left point a, a motive that maps a
    /// right point and a proof to a type, a base case at the reflexive proof, a
    /// right point b, and a proof that a equals b. It produces the motive at b
    /// and that proof.
    J {
        /// The universe the motive lands in.
        level: UniverseLevel,
        /// The type both points inhabit.
        ty: Box<Term>,
        /// The left point.
        left: Box<Term>,
        /// The motive.
        motive: Box<Term>,
        /// The base case.
        base: Box<Term>,
        /// The right point.
        right: Box<Term>,
        /// The proof of the identity.
        proof: Box<Term>,
    },
}
