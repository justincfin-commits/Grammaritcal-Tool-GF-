//! Semantic values for normalization by evaluation.
//!
//! This module holds data only. The module `crate::mltt::eval` holds the
//! behavior that builds and reads these values.
//!
//! A value is a term whose redexes have already fired. A neutral value is a
//! value that cannot reduce further, because a variable or a constant blocks
//! it. Neutral values carry de Bruijn levels, counted from the outside in, so
//! that a stored environment stays correct when the tool pushes it under a new
//! binder.

use std::rc::Rc;

use crate::mltt::term::{Binder, ConstName, Term};
use crate::newtype::{DeBruijnLevel, UniverseLevel};

/// A list that supplies a value for every free variable of a term.
///
/// Index zero names the head of the list, which is the most recent binding.
#[derive(Clone, Debug)]
pub enum Env {
    /// No bindings.
    Empty,
    /// One binding in front of the earlier bindings.
    Extend {
        /// The most recent binding.
        head: Rc<Value>,
        /// The earlier bindings.
        tail: Rc<Env>,
    },
}

/// A term paired with the environment that supplies its free variables.
#[derive(Clone, Debug)]
pub struct Closure {
    /// The environment the body reads.
    pub env: Env,
    /// The body, which expects one extra binding at index zero.
    pub body: Rc<Term>,
}

/// A value of the core type theory.
#[derive(Clone, Debug)]
pub enum Value {
    /// A value that a variable or a constant blocks.
    Neutral(Neutral),
    /// A universe.
    Universe(UniverseLevel),
    /// A dependent function type.
    Pi {
        /// The name the source file gives the bound variable.
        binder: Binder,
        /// The type of the argument.
        domain: Rc<Value>,
        /// The result type, which waits for the argument.
        codomain: Closure,
    },
    /// A function.
    Lam {
        /// The name the source file gives the bound variable.
        binder: Binder,
        /// The body, which waits for the argument.
        body: Closure,
    },
    /// A dependent pair type.
    Sigma {
        /// The name the source file gives the bound variable.
        binder: Binder,
        /// The type of the first component.
        first: Rc<Value>,
        /// The type of the second component, which waits for the first.
        second: Closure,
    },
    /// A pair.
    Pair {
        /// The first component.
        first: Rc<Value>,
        /// The second component.
        second: Rc<Value>,
    },
    /// An identity type.
    Id {
        /// The type both sides inhabit.
        ty: Rc<Value>,
        /// The left side.
        left: Rc<Value>,
        /// The right side.
        right: Rc<Value>,
    },
    /// The proof that a point equals itself.
    Refl {
        /// The type the point inhabits.
        ty: Rc<Value>,
        /// The point.
        point: Rc<Value>,
    },
}

/// A value that cannot reduce, because a variable or a constant blocks it.
#[derive(Clone, Debug)]
pub enum Neutral {
    /// A free variable, counted from the outside in.
    Var(DeBruijnLevel),
    /// A declared constant. The tool never unfolds a constant.
    Const(ConstName),
    /// A blocked application.
    App {
        /// The blocked function.
        function: Rc<Neutral>,
        /// The argument.
        argument: Rc<Value>,
    },
    /// A blocked first projection.
    First(Rc<Neutral>),
    /// A blocked second projection.
    Second(Rc<Neutral>),
    /// A blocked identity elimination.
    J {
        /// The universe the motive lands in.
        level: UniverseLevel,
        /// The type both points inhabit.
        ty: Rc<Value>,
        /// The left point.
        left: Rc<Value>,
        /// The motive.
        motive: Rc<Value>,
        /// The base case.
        base: Rc<Value>,
        /// The right point.
        right: Rc<Value>,
        /// The blocked proof.
        proof: Rc<Neutral>,
    },
}

/// A stack that records the type each enclosing binder gives its variable.
#[derive(Clone, Debug)]
pub enum TypeStack {
    /// No binders.
    Empty,
    /// One binder in front of the enclosing binders.
    Extend {
        /// The name the binder writes down.
        binder: Binder,
        /// The type the binder gives its variable.
        ty: Rc<Value>,
        /// The enclosing binders.
        tail: Rc<TypeStack>,
    },
}

/// Everything the type checker knows about the binders it stands under.
#[derive(Clone, Debug)]
pub struct Context {
    /// The value each binder gives its variable, which is a fresh neutral variable.
    pub values: Env,
    /// The type each binder gives its variable.
    pub types: Rc<TypeStack>,
    /// How many binders the checker stands under.
    pub depth: DeBruijnLevel,
}
