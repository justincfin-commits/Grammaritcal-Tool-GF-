//! Bidirectional type checking for the core type theory.
//!
//! This module holds behavior only.
//!
//! Two functions carry the work. `infer` reads a type off a term. `check`
//! compares a term against a type the caller already knows. The split lets a
//! function stay unannotated: the tool always checks a function against a
//! known dependent function type, and never guesses one.

use std::rc::Rc;

use crate::error::TypeError;
use crate::mltt::eval::{
    apply, apply_closure, convertible, deeper, eval, extend, fresh, quote, spend,
};
use crate::mltt::signature::{lookup_constant, Signature};
use crate::mltt::term::{Binder, Term};
use crate::mltt::value::{Closure, Context, Env, TypeStack, Value};
use crate::newtype::{DeBruijnIndex, DeBruijnLevel, EvalFuel, UniverseLevel, VarName};

/// Builds the context that stands under no binder.
pub fn empty_context() -> Context {
    Context {
        values: Env::Empty,
        types: Rc::new(TypeStack::Empty),
        depth: DeBruijnLevel(0),
    }
}

/// Pushes one binder onto a context and returns the new context.
pub fn bind(context: &Context, binder: &Binder, ty: Rc<Value>) -> Context {
    Context {
        values: extend(&context.values, fresh(context.depth)),
        types: Rc::new(TypeStack::Extend {
            binder: binder.clone(),
            ty,
            tail: Rc::clone(&context.types),
        }),
        depth: deeper(context.depth),
    }
}

/// Reads the type that a binder gives one variable.
pub fn type_of_var(stack: &TypeStack, index: DeBruijnIndex) -> Option<Rc<Value>> {
    match stack {
        TypeStack::Empty => None,
        TypeStack::Extend { ty, tail, .. } => match index.0.checked_sub(1) {
            None => Some(Rc::clone(ty)),
            Some(deeper_index) => type_of_var(tail, DeBruijnIndex(deeper_index)),
        },
    }
}

/// Checks that a term denotes a type, and returns the universe it sits in.
pub fn check_is_type(
    signature: &Signature,
    context: &Context,
    term: &Term,
    fuel: EvalFuel,
) -> Result<UniverseLevel, TypeError> {
    let fuel = spend(fuel)?;
    let found = infer(signature, context, term, fuel)?;
    match found.as_ref() {
        Value::Universe(level) => Ok(*level),
        other => Err(TypeError::NotAType {
            found_type: quote(context.depth, other, fuel)?,
        }),
    }
}

/// Returns the larger of two universe levels.
fn join(left: UniverseLevel, right: UniverseLevel) -> UniverseLevel {
    if left.0 >= right.0 {
        left
    } else {
        right
    }
}

/// Reads a type off a term.
pub fn infer(
    signature: &Signature,
    context: &Context,
    term: &Term,
    fuel: EvalFuel,
) -> Result<Rc<Value>, TypeError> {
    let fuel = spend(fuel)?;
    match term {
        Term::Var(index) => type_of_var(context.types.as_ref(), *index)
            .ok_or(TypeError::UnboundVariable { index: *index }),
        Term::Const(name) => match lookup_constant(signature, name) {
            Some(declared) => eval(&Env::Empty, declared, fuel),
            None => Err(TypeError::UnknownConstant { name: name.clone() }),
        },
        Term::Universe(level) => Ok(Rc::new(Value::Universe(UniverseLevel(
            level.0.saturating_add(1),
        )))),
        Term::Pi {
            binder,
            domain,
            codomain,
        } => {
            let domain_level = check_is_type(signature, context, domain, fuel)?;
            let domain_value = eval(&context.values, domain, fuel)?;
            let inner = bind(context, binder, domain_value);
            let codomain_level = check_is_type(signature, &inner, codomain, fuel)?;
            Ok(Rc::new(Value::Universe(join(domain_level, codomain_level))))
        }
        Term::Sigma {
            binder,
            first,
            second,
        } => {
            let first_level = check_is_type(signature, context, first, fuel)?;
            let first_value = eval(&context.values, first, fuel)?;
            let inner = bind(context, binder, first_value);
            let second_level = check_is_type(signature, &inner, second, fuel)?;
            Ok(Rc::new(Value::Universe(join(first_level, second_level))))
        }
        Term::App { function, argument } => {
            let head = infer(signature, context, function, fuel)?;
            match head.as_ref() {
                Value::Pi {
                    domain, codomain, ..
                } => {
                    check(signature, context, argument, domain, fuel)?;
                    let value = eval(&context.values, argument, fuel)?;
                    apply_closure(codomain, value, fuel)
                }
                other => Err(TypeError::NotAFunction {
                    found_type: quote(context.depth, other, fuel)?,
                }),
            }
        }
        Term::First(pair) => {
            let head = infer(signature, context, pair, fuel)?;
            match head.as_ref() {
                Value::Sigma { first, .. } => Ok(Rc::clone(first)),
                other => Err(TypeError::NotAPair {
                    found_type: quote(context.depth, other, fuel)?,
                }),
            }
        }
        Term::Second(pair) => {
            let head = infer(signature, context, pair, fuel)?;
            match head.as_ref() {
                Value::Sigma { second, .. } => {
                    let first_value = eval(&context.values, &Term::First(pair.clone()), fuel)?;
                    apply_closure(second, first_value, fuel)
                }
                other => Err(TypeError::NotAPair {
                    found_type: quote(context.depth, other, fuel)?,
                }),
            }
        }
        Term::Lam { .. } | Term::Pair { .. } => Err(TypeError::CannotInfer {
            subject: term.clone(),
        }),
        Term::Id { ty, left, right } => {
            let level = check_is_type(signature, context, ty, fuel)?;
            let ty_value = eval(&context.values, ty, fuel)?;
            check(signature, context, left, &ty_value, fuel)?;
            check(signature, context, right, &ty_value, fuel)?;
            Ok(Rc::new(Value::Universe(level)))
        }
        Term::Refl { ty, point } => {
            let _ = check_is_type(signature, context, ty, fuel)?;
            let ty_value = eval(&context.values, ty, fuel)?;
            check(signature, context, point, &ty_value, fuel)?;
            let point_value = eval(&context.values, point, fuel)?;
            Ok(Rc::new(Value::Id {
                ty: ty_value,
                left: Rc::clone(&point_value),
                right: point_value,
            }))
        }
        Term::J {
            level,
            ty,
            left,
            motive,
            base,
            right,
            proof,
        } => infer_elimination(
            signature, context, *level, ty, left, motive, base, right, proof, fuel,
        ),
    }
}

/// Builds the type that the motive of an identity elimination must carry.
///
/// The motive maps a right point `b` and a proof that `a` equals `b` to a
/// type in the universe the term names. This function builds that dependent
/// function type as a value, so the checker never has to shift indices.
fn motive_type_value(ty: Rc<Value>, left: Rc<Value>, level: UniverseLevel) -> Value {
    let with_ty = extend(&Env::Empty, Rc::clone(&ty));
    let with_left = extend(&with_ty, left);
    Value::Pi {
        binder: Binder(VarName("b".to_string())),
        domain: ty,
        codomain: Closure {
            env: with_left,
            body: Rc::new(Term::Pi {
                binder: Binder(VarName("p".to_string())),
                domain: Box::new(Term::Id {
                    ty: Box::new(Term::Var(DeBruijnIndex(2))),
                    left: Box::new(Term::Var(DeBruijnIndex(1))),
                    right: Box::new(Term::Var(DeBruijnIndex(0))),
                }),
                codomain: Box::new(Term::Universe(level)),
            }),
        },
    }
}

/// Reads a type off an identity elimination.
#[allow(clippy::too_many_arguments)]
fn infer_elimination(
    signature: &Signature,
    context: &Context,
    level: UniverseLevel,
    ty: &Term,
    left: &Term,
    motive: &Term,
    base: &Term,
    right: &Term,
    proof: &Term,
    fuel: EvalFuel,
) -> Result<Rc<Value>, TypeError> {
    let fuel = spend(fuel)?;
    let _ = check_is_type(signature, context, ty, fuel)?;
    let ty_value = eval(&context.values, ty, fuel)?;
    check(signature, context, left, &ty_value, fuel)?;
    let left_value = eval(&context.values, left, fuel)?;

    let wanted_motive = motive_type_value(Rc::clone(&ty_value), Rc::clone(&left_value), level);
    check(signature, context, motive, &wanted_motive, fuel)?;
    let motive_value = eval(&context.values, motive, fuel)?;

    let reflexive = Rc::new(Value::Refl {
        ty: Rc::clone(&ty_value),
        point: Rc::clone(&left_value),
    });
    let base_type = apply(
        apply(Rc::clone(&motive_value), Rc::clone(&left_value), fuel)?,
        reflexive,
        fuel,
    )?;
    check(signature, context, base, base_type.as_ref(), fuel)?;

    check(signature, context, right, &ty_value, fuel)?;
    let right_value = eval(&context.values, right, fuel)?;
    let proof_type = Rc::new(Value::Id {
        ty: ty_value,
        left: left_value,
        right: Rc::clone(&right_value),
    });
    check(signature, context, proof, proof_type.as_ref(), fuel)?;
    let proof_value = eval(&context.values, proof, fuel)?;
    apply(apply(motive_value, right_value, fuel)?, proof_value, fuel)
}

/// Compares a term against a type the caller already knows.
pub fn check(
    signature: &Signature,
    context: &Context,
    term: &Term,
    expected: &Value,
    fuel: EvalFuel,
) -> Result<(), TypeError> {
    let fuel = spend(fuel)?;
    match (term, expected) {
        (
            Term::Lam { binder, body },
            Value::Pi {
                domain, codomain, ..
            },
        ) => {
            let inner = bind(context, binder, Rc::clone(domain));
            let wanted = apply_closure(codomain, fresh(context.depth), fuel)?;
            check(signature, &inner, body, wanted.as_ref(), fuel)
        }
        (Term::Lam { .. }, other) => Err(TypeError::LambdaAgainstNonFunction {
            expected: quote(context.depth, other, fuel)?,
        }),
        (
            Term::Pair { first, second },
            Value::Sigma {
                first: first_type,
                second: second_type,
                ..
            },
        ) => {
            check(signature, context, first, first_type, fuel)?;
            let first_value = eval(&context.values, first, fuel)?;
            let wanted = apply_closure(second_type, first_value, fuel)?;
            check(signature, context, second, wanted.as_ref(), fuel)
        }
        (Term::Pair { .. }, other) => Err(TypeError::PairAgainstNonPairType {
            expected: quote(context.depth, other, fuel)?,
        }),
        _ => {
            let found = infer(signature, context, term, fuel)?;
            if convertible(context.depth, found.as_ref(), expected, fuel)? {
                Ok(())
            } else {
                Err(TypeError::Mismatch {
                    expected: quote(context.depth, expected, fuel)?,
                    found: quote(context.depth, found.as_ref(), fuel)?,
                })
            }
        }
    }
}

/// Reads a type off a closed term and returns that type in normal form.
pub fn infer_closed(
    signature: &Signature,
    term: &Term,
    fuel: EvalFuel,
) -> Result<Term, TypeError> {
    let context = empty_context();
    let found = infer(signature, &context, term, fuel)?;
    quote(context.depth, found.as_ref(), fuel)
}
