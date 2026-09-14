//! Normalization by evaluation for the core type theory.
//!
//! This module holds behavior only. It reads the data in
//! `crate::mltt::term` and `crate::mltt::value` and returns new data.
//!
//! Three functions carry the work. `eval` turns a term into a value and fires
//! every redex on the way. `quote` turns a value back into a term in normal
//! form. `convertible` decides judgmental equality by comparing two values,
//! and it treats a function as equal to its eta expansion.
//!
//! Every function takes a fuel counter. Each recursive call spends one unit.
//! A function that runs out of fuel returns `TypeError::OutOfFuel` instead of
//! descending further, so no call can loop forever or exhaust the stack.

use std::rc::Rc;

use crate::error::TypeError;
use crate::mltt::term::Term;
use crate::mltt::value::{Closure, Env, Neutral, Value};
use crate::newtype::{DeBruijnIndex, DeBruijnLevel, EvalFuel, UniverseLevel};

/// Spends one unit of fuel, or reports that the budget ran out.
pub fn spend(fuel: EvalFuel) -> Result<EvalFuel, TypeError> {
    match fuel.0.checked_sub(1) {
        Some(left) => Ok(EvalFuel(left)),
        None => Err(TypeError::OutOfFuel),
    }
}

/// Puts one value in front of an environment and returns the new environment.
pub fn extend(env: &Env, value: Rc<Value>) -> Env {
    Env::Extend {
        head: value,
        tail: Rc::new(env.clone()),
    }
}

/// Reads the value that an environment gives one variable.
pub fn lookup(env: &Env, index: DeBruijnIndex) -> Option<Rc<Value>> {
    match env {
        Env::Empty => None,
        Env::Extend { head, tail } => match index.0.checked_sub(1) {
            None => Some(Rc::clone(head)),
            Some(deeper) => lookup(tail, DeBruijnIndex(deeper)),
        },
    }
}

/// Builds the neutral variable that stands for the next binder.
pub fn fresh(depth: DeBruijnLevel) -> Rc<Value> {
    Rc::new(Value::Neutral(Neutral::Var(depth)))
}

/// Counts one more binder.
pub fn deeper(depth: DeBruijnLevel) -> DeBruijnLevel {
    DeBruijnLevel(depth.0.saturating_add(1))
}

/// Applies a function value to an argument value.
pub fn apply(function: Rc<Value>, argument: Rc<Value>, fuel: EvalFuel) -> Result<Rc<Value>, TypeError> {
    let fuel = spend(fuel)?;
    match function.as_ref() {
        Value::Lam { body, .. } => apply_closure(body, argument, fuel),
        Value::Neutral(blocked) => Ok(Rc::new(Value::Neutral(Neutral::App {
            function: Rc::new(blocked.clone()),
            argument,
        }))),
        _ => Err(TypeError::AppliedNonFunction),
    }
}

/// Fills the one open slot of a closure and evaluates its body.
pub fn apply_closure(closure: &Closure, argument: Rc<Value>, fuel: EvalFuel) -> Result<Rc<Value>, TypeError> {
    let fuel = spend(fuel)?;
    eval(&extend(&closure.env, argument), closure.body.as_ref(), fuel)
}

/// Takes the first component of a pair value.
pub fn first_of(pair: Rc<Value>, fuel: EvalFuel) -> Result<Rc<Value>, TypeError> {
    let _ = spend(fuel)?;
    match pair.as_ref() {
        Value::Pair { first, .. } => Ok(Rc::clone(first)),
        Value::Neutral(blocked) => Ok(Rc::new(Value::Neutral(Neutral::First(Rc::new(
            blocked.clone(),
        ))))),
        _ => Err(TypeError::ProjectedNonPair),
    }
}

/// Takes the second component of a pair value.
pub fn second_of(pair: Rc<Value>, fuel: EvalFuel) -> Result<Rc<Value>, TypeError> {
    let _ = spend(fuel)?;
    match pair.as_ref() {
        Value::Pair { second, .. } => Ok(Rc::clone(second)),
        Value::Neutral(blocked) => Ok(Rc::new(Value::Neutral(Neutral::Second(Rc::new(
            blocked.clone(),
        ))))),
        _ => Err(TypeError::ProjectedNonPair),
    }
}

/// Eliminates an identity proof. A reflexive proof hands back the base case.
#[allow(clippy::too_many_arguments)]
pub fn eliminate_id(
    level: UniverseLevel,
    ty: Rc<Value>,
    left: Rc<Value>,
    motive: Rc<Value>,
    base: Rc<Value>,
    right: Rc<Value>,
    proof: Rc<Value>,
    fuel: EvalFuel,
) -> Result<Rc<Value>, TypeError> {
    let _ = spend(fuel)?;
    match proof.as_ref() {
        Value::Refl { .. } => Ok(base),
        Value::Neutral(blocked) => Ok(Rc::new(Value::Neutral(Neutral::J {
            level,
            ty,
            left,
            motive,
            base,
            right,
            proof: Rc::new(blocked.clone()),
        }))),
        _ => Err(TypeError::EliminatedNonProof),
    }
}

/// Builds the closure that a binder leaves behind.
fn close(env: &Env, body: &Term) -> Closure {
    Closure {
        env: env.clone(),
        body: Rc::new(body.clone()),
    }
}

/// Turns a term into a value and fires every redex on the way.
pub fn eval(env: &Env, term: &Term, fuel: EvalFuel) -> Result<Rc<Value>, TypeError> {
    let fuel = spend(fuel)?;
    match term {
        Term::Var(index) => lookup(env, *index).ok_or(TypeError::UnboundVariable { index: *index }),
        Term::Const(name) => Ok(Rc::new(Value::Neutral(Neutral::Const(name.clone())))),
        Term::Universe(level) => Ok(Rc::new(Value::Universe(*level))),
        Term::Pi {
            binder,
            domain,
            codomain,
        } => Ok(Rc::new(Value::Pi {
            binder: binder.clone(),
            domain: eval(env, domain, fuel)?,
            codomain: close(env, codomain),
        })),
        Term::Lam { binder, body } => Ok(Rc::new(Value::Lam {
            binder: binder.clone(),
            body: close(env, body),
        })),
        Term::App { function, argument } => {
            let head = eval(env, function, fuel)?;
            let value = eval(env, argument, fuel)?;
            apply(head, value, fuel)
        }
        Term::Sigma {
            binder,
            first,
            second,
        } => Ok(Rc::new(Value::Sigma {
            binder: binder.clone(),
            first: eval(env, first, fuel)?,
            second: close(env, second),
        })),
        Term::Pair { first, second } => Ok(Rc::new(Value::Pair {
            first: eval(env, first, fuel)?,
            second: eval(env, second, fuel)?,
        })),
        Term::First(pair) => first_of(eval(env, pair, fuel)?, fuel),
        Term::Second(pair) => second_of(eval(env, pair, fuel)?, fuel),
        Term::Id { ty, left, right } => Ok(Rc::new(Value::Id {
            ty: eval(env, ty, fuel)?,
            left: eval(env, left, fuel)?,
            right: eval(env, right, fuel)?,
        })),
        Term::Refl { ty, point } => Ok(Rc::new(Value::Refl {
            ty: eval(env, ty, fuel)?,
            point: eval(env, point, fuel)?,
        })),
        Term::J {
            level,
            ty,
            left,
            motive,
            base,
            right,
            proof,
        } => eliminate_id(
            *level,
            eval(env, ty, fuel)?,
            eval(env, left, fuel)?,
            eval(env, motive, fuel)?,
            eval(env, base, fuel)?,
            eval(env, right, fuel)?,
            eval(env, proof, fuel)?,
            fuel,
        ),
    }
}

/// Turns a value back into a term in normal form.
pub fn quote(depth: DeBruijnLevel, value: &Value, fuel: EvalFuel) -> Result<Term, TypeError> {
    let fuel = spend(fuel)?;
    match value {
        Value::Neutral(blocked) => quote_neutral(depth, blocked, fuel),
        Value::Universe(level) => Ok(Term::Universe(*level)),
        Value::Pi {
            binder,
            domain,
            codomain,
        } => {
            let under = apply_closure(codomain, fresh(depth), fuel)?;
            Ok(Term::Pi {
                binder: binder.clone(),
                domain: Box::new(quote(depth, domain, fuel)?),
                codomain: Box::new(quote(deeper(depth), under.as_ref(), fuel)?),
            })
        }
        Value::Lam { binder, body } => {
            let under = apply_closure(body, fresh(depth), fuel)?;
            Ok(Term::Lam {
                binder: binder.clone(),
                body: Box::new(quote(deeper(depth), under.as_ref(), fuel)?),
            })
        }
        Value::Sigma {
            binder,
            first,
            second,
        } => {
            let under = apply_closure(second, fresh(depth), fuel)?;
            Ok(Term::Sigma {
                binder: binder.clone(),
                first: Box::new(quote(depth, first, fuel)?),
                second: Box::new(quote(deeper(depth), under.as_ref(), fuel)?),
            })
        }
        Value::Pair { first, second } => Ok(Term::Pair {
            first: Box::new(quote(depth, first, fuel)?),
            second: Box::new(quote(depth, second, fuel)?),
        }),
        Value::Id { ty, left, right } => Ok(Term::Id {
            ty: Box::new(quote(depth, ty, fuel)?),
            left: Box::new(quote(depth, left, fuel)?),
            right: Box::new(quote(depth, right, fuel)?),
        }),
        Value::Refl { ty, point } => Ok(Term::Refl {
            ty: Box::new(quote(depth, ty, fuel)?),
            point: Box::new(quote(depth, point, fuel)?),
        }),
    }
}

/// Turns a blocked value back into a term in normal form.
pub fn quote_neutral(depth: DeBruijnLevel, blocked: &Neutral, fuel: EvalFuel) -> Result<Term, TypeError> {
    let fuel = spend(fuel)?;
    match blocked {
        Neutral::Var(level) => match depth.0.checked_sub(level.0).and_then(|d| d.checked_sub(1)) {
            Some(index) => Ok(Term::Var(DeBruijnIndex(index))),
            None => Err(TypeError::UnboundVariable {
                index: DeBruijnIndex(level.0),
            }),
        },
        Neutral::Const(name) => Ok(Term::Const(name.clone())),
        Neutral::App { function, argument } => Ok(Term::App {
            function: Box::new(quote_neutral(depth, function, fuel)?),
            argument: Box::new(quote(depth, argument, fuel)?),
        }),
        Neutral::First(pair) => Ok(Term::First(Box::new(quote_neutral(depth, pair, fuel)?))),
        Neutral::Second(pair) => Ok(Term::Second(Box::new(quote_neutral(depth, pair, fuel)?))),
        Neutral::J {
            level,
            ty,
            left,
            motive,
            base,
            right,
            proof,
        } => Ok(Term::J {
            level: *level,
            ty: Box::new(quote(depth, ty, fuel)?),
            left: Box::new(quote(depth, left, fuel)?),
            motive: Box::new(quote(depth, motive, fuel)?),
            base: Box::new(quote(depth, base, fuel)?),
            right: Box::new(quote(depth, right, fuel)?),
            proof: Box::new(quote_neutral(depth, proof, fuel)?),
        }),
    }
}

/// Evaluates a term and reads the result back in normal form.
pub fn normalize(env: &Env, term: &Term, fuel: EvalFuel) -> Result<Term, TypeError> {
    let value = eval(env, term, fuel)?;
    quote(DeBruijnLevel(0), value.as_ref(), fuel)
}

/// Decides judgmental equality of two values.
///
/// The first two cases give eta equality for functions: a function equals any
/// value that behaves the same on a fresh argument. The next two cases give
/// eta equality for pairs.
pub fn convertible(
    depth: DeBruijnLevel,
    left: &Value,
    right: &Value,
    fuel: EvalFuel,
) -> Result<bool, TypeError> {
    let fuel = spend(fuel)?;
    match (left, right) {
        (Value::Lam { body, .. }, _) => {
            let point = fresh(depth);
            let opened = apply_closure(body, Rc::clone(&point), fuel)?;
            let other = apply(Rc::new(right.clone()), point, fuel)?;
            convertible(deeper(depth), opened.as_ref(), other.as_ref(), fuel)
        }
        (_, Value::Lam { body, .. }) => {
            let point = fresh(depth);
            let opened = apply_closure(body, Rc::clone(&point), fuel)?;
            let other = apply(Rc::new(left.clone()), point, fuel)?;
            convertible(deeper(depth), other.as_ref(), opened.as_ref(), fuel)
        }
        (Value::Pair { first, second }, _) => {
            let other = Rc::new(right.clone());
            let other_first = first_of(Rc::clone(&other), fuel)?;
            let other_second = second_of(other, fuel)?;
            Ok(convertible(depth, first, other_first.as_ref(), fuel)?
                && convertible(depth, second, other_second.as_ref(), fuel)?)
        }
        (_, Value::Pair { first, second }) => {
            let other = Rc::new(left.clone());
            let other_first = first_of(Rc::clone(&other), fuel)?;
            let other_second = second_of(other, fuel)?;
            Ok(convertible(depth, other_first.as_ref(), first, fuel)?
                && convertible(depth, other_second.as_ref(), second, fuel)?)
        }
        (Value::Universe(one), Value::Universe(two)) => Ok(one == two),
        (
            Value::Pi {
                domain: left_domain,
                codomain: left_codomain,
                ..
            },
            Value::Pi {
                domain: right_domain,
                codomain: right_codomain,
                ..
            },
        ) => {
            if !convertible(depth, left_domain, right_domain, fuel)? {
                return Ok(false);
            }
            let point = fresh(depth);
            let opened_left = apply_closure(left_codomain, Rc::clone(&point), fuel)?;
            let opened_right = apply_closure(right_codomain, point, fuel)?;
            convertible(deeper(depth), opened_left.as_ref(), opened_right.as_ref(), fuel)
        }
        (
            Value::Sigma {
                first: left_first,
                second: left_second,
                ..
            },
            Value::Sigma {
                first: right_first,
                second: right_second,
                ..
            },
        ) => {
            if !convertible(depth, left_first, right_first, fuel)? {
                return Ok(false);
            }
            let point = fresh(depth);
            let opened_left = apply_closure(left_second, Rc::clone(&point), fuel)?;
            let opened_right = apply_closure(right_second, point, fuel)?;
            convertible(deeper(depth), opened_left.as_ref(), opened_right.as_ref(), fuel)
        }
        (
            Value::Id {
                ty: left_ty,
                left: left_left,
                right: left_right,
            },
            Value::Id {
                ty: right_ty,
                left: right_left,
                right: right_right,
            },
        ) => Ok(convertible(depth, left_ty, right_ty, fuel)?
            && convertible(depth, left_left, right_left, fuel)?
            && convertible(depth, left_right, right_right, fuel)?),
        (
            Value::Refl {
                ty: left_ty,
                point: left_point,
            },
            Value::Refl {
                ty: right_ty,
                point: right_point,
            },
        ) => Ok(convertible(depth, left_ty, right_ty, fuel)?
            && convertible(depth, left_point, right_point, fuel)?),
        (Value::Neutral(one), Value::Neutral(two)) => convertible_neutral(depth, one, two, fuel),
        _ => Ok(false),
    }
}

/// Decides judgmental equality of two blocked values.
pub fn convertible_neutral(
    depth: DeBruijnLevel,
    left: &Neutral,
    right: &Neutral,
    fuel: EvalFuel,
) -> Result<bool, TypeError> {
    let fuel = spend(fuel)?;
    match (left, right) {
        (Neutral::Var(one), Neutral::Var(two)) => Ok(one == two),
        (Neutral::Const(one), Neutral::Const(two)) => Ok(one == two),
        (
            Neutral::App {
                function: left_function,
                argument: left_argument,
            },
            Neutral::App {
                function: right_function,
                argument: right_argument,
            },
        ) => Ok(
            convertible_neutral(depth, left_function, right_function, fuel)?
                && convertible(depth, left_argument, right_argument, fuel)?,
        ),
        (Neutral::First(one), Neutral::First(two)) => convertible_neutral(depth, one, two, fuel),
        (Neutral::Second(one), Neutral::Second(two)) => convertible_neutral(depth, one, two, fuel),
        (
            Neutral::J {
                level: left_level,
                ty: left_ty,
                left: left_left,
                motive: left_motive,
                base: left_base,
                right: left_right,
                proof: left_proof,
            },
            Neutral::J {
                level: right_level,
                ty: right_ty,
                left: right_left,
                motive: right_motive,
                base: right_base,
                right: right_right,
                proof: right_proof,
            },
        ) => Ok(left_level == right_level
            && convertible(depth, left_ty, right_ty, fuel)?
            && convertible(depth, left_left, right_left, fuel)?
            && convertible(depth, left_motive, right_motive, fuel)?
            && convertible(depth, left_base, right_base, fuel)?
            && convertible(depth, left_right, right_right, fuel)?
            && convertible_neutral(depth, left_proof, right_proof, fuel)?),
        _ => Ok(false),
    }
}
