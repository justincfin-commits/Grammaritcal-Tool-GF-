//! Evaluates linearization expressions.
//!
//! This module holds behavior only.
//!
//! The evaluator runs twice in the life of a grammar. The compiler runs it with
//! symbolic arguments, so the result carries slots that mark where each
//! argument belongs. The printer runs it with finished arguments, so the result
//! carries words only. One evaluator serves both, because a slot is just
//! another item in a word list.
//!
//! Every call spends one unit of fuel. A helper that names itself therefore
//! stops with `ConcreteError::OutOfFuel` instead of running forever.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::concrete::lintype::ParamTypeDecl;
use crate::concrete::shape::{prints_nothing, shape_of, state_of_value, symbolic_value, ParamTypes};
use crate::concrete::value::{CEnv, CValue, GapSpec};
use crate::error::{CompileError, ConcreteError};
use crate::gf::ast::{CExpr, CPattern, ConcreteModule, OperDecl};
use crate::newtype::{EvalFuel, FieldName, OperName, ParamTypeName, ParamValue, VarName};

/// Everything an expression may name besides its own arguments.
#[derive(Clone, Debug, Default)]
pub struct Helpers {
    /// The helper declarations, keyed by name.
    pub opers: BTreeMap<OperName, OperDecl>,
    /// The parameter type that each parameter value belongs to.
    pub value_types: BTreeMap<ParamValue, ParamTypeName>,
    /// The values that each parameter type holds.
    pub param_types: ParamTypes,
}

/// Collects the helpers and parameter types that a concrete module declares.
pub fn build_helpers(module: &ConcreteModule) -> Result<Helpers, CompileError> {
    let mut helpers = Helpers::default();
    for ParamTypeDecl { name, values } in &module.params {
        helpers.param_types.insert(name.clone(), values.clone());
        for value in values {
            if helpers
                .value_types
                .insert(value.clone(), name.clone())
                .is_some()
            {
                return Err(CompileError::DuplicateParamValue {
                    value: value.clone(),
                });
            }
        }
    }
    for declaration in &module.opers {
        helpers
            .opers
            .insert(declaration.name.clone(), declaration.clone());
    }
    Ok(helpers)
}

/// Spends one unit of fuel, or reports that the budget ran out.
fn spend(fuel: EvalFuel) -> Result<EvalFuel, ConcreteError> {
    match fuel.0.checked_sub(1) {
        Some(left) => Ok(EvalFuel(left)),
        None => Err(ConcreteError::OutOfFuel),
    }
}

/// Puts one binding in front of an environment.
pub fn bind(env: &CEnv, name: &VarName, value: Rc<CValue>) -> CEnv {
    CEnv(
        env.0
            .iter()
            .cloned()
            .chain(std::iter::once((name.clone(), value)))
            .collect(),
    )
}

/// Reads the value a name carries, taking the nearest binding.
fn lookup(env: &CEnv, name: &str) -> Option<Rc<CValue>> {
    env.0
        .iter()
        .rev()
        .find(|(bound, _)| bound.0 == name)
        .map(|(_, value)| Rc::clone(value))
}

/// Answers whether a pattern covers a parameter value.
fn matches(pattern: &CPattern, value: &ParamValue) -> bool {
    match pattern {
        CPattern::Wildcard => true,
        CPattern::Value(named) => named == value,
    }
}

/// Applies a helper or a higher-order argument to one value.
pub fn apply_value(
    helpers: &Helpers,
    function: Rc<CValue>,
    argument: Rc<CValue>,
    fuel: EvalFuel,
) -> Result<Rc<CValue>, ConcreteError> {
    let fuel = spend(fuel)?;
    match function.as_ref() {
        CValue::Closure { names, body, env } => match names.split_first() {
            None => Err(ConcreteError::NotAFunction {
                found: shape_of(function.as_ref()),
            }),
            Some((first, rest)) => {
                let extended = bind(env, first, argument);
                if rest.is_empty() {
                    eval_cexpr(helpers, &extended, body.as_ref(), fuel)
                } else {
                    Ok(Rc::new(CValue::Closure {
                        names: rest.to_vec(),
                        body: Rc::clone(body),
                        env: extended,
                    }))
                }
            }
        },
        CValue::Binder {
            arg,
            result,
            state,
            gap_cat,
            gap_type,
        } => {
            if !prints_nothing(argument.as_ref()) {
                return Err(ConcreteError::GapNotEmpty { arg: *arg });
            }
            let gap_state = state_of_value(argument.as_ref(), gap_type, &helpers.param_types)?;
            let gap = GapSpec {
                cat: gap_cat.clone(),
                state: gap_state,
            };
            symbolic_value(result, &helpers.param_types, state, *arg, &Some(gap)).map(Rc::new)
        }
        other => Err(ConcreteError::NotAFunction {
            found: shape_of(other),
        }),
    }
}

/// Reads the value that a name carries.
fn eval_name(
    helpers: &Helpers,
    env: &CEnv,
    name: &str,
    fuel: EvalFuel,
) -> Result<Rc<CValue>, ConcreteError> {
    if let Some(value) = lookup(env, name) {
        return Ok(value);
    }
    let oper_name = OperName(name.to_string());
    if let Some(declaration) = helpers.opers.get(&oper_name) {
        if declaration.args.is_empty() {
            return eval_cexpr(helpers, &CEnv::default(), &declaration.body, fuel);
        }
        return Ok(Rc::new(CValue::Closure {
            names: declaration.args.clone(),
            body: Rc::new(declaration.body.clone()),
            env: CEnv::default(),
        }));
    }
    let value = ParamValue(name.to_string());
    if helpers.value_types.contains_key(&value) {
        return Ok(Rc::new(CValue::Param(value)));
    }
    Err(ConcreteError::UnknownIdentifier {
        name: name.to_string(),
    })
}

/// Reads the rows that a table holds.
fn eval_table(
    helpers: &Helpers,
    env: &CEnv,
    branches: &[(CPattern, CExpr)],
    fuel: EvalFuel,
) -> Result<Rc<CValue>, ConcreteError> {
    let named = branches.iter().find_map(|(pattern, _)| match pattern {
        CPattern::Value(value) => Some(value.clone()),
        CPattern::Wildcard => None,
    });
    let named = match named {
        Some(value) => value,
        None => return Err(ConcreteError::TableNeedsNamedRow),
    };
    let param_type = match helpers.value_types.get(&named) {
        Some(param_type) => param_type.clone(),
        None => return Err(ConcreteError::UnknownParamValue { value: named }),
    };
    let values = match helpers.param_types.get(&param_type) {
        Some(values) => values.clone(),
        None => return Err(ConcreteError::UnknownParamTypeName { name: param_type }),
    };
    let mut rows: Vec<(ParamValue, CValue)> = Vec::new();
    for value in values {
        match branches
            .iter()
            .find(|(pattern, _)| matches(pattern, &value))
        {
            Some((_, body)) => {
                let evaluated = eval_cexpr(helpers, env, body, fuel)?;
                rows.push((value, evaluated.as_ref().clone()));
            }
            None => return Err(ConcreteError::NoBranchMatches { value }),
        }
    }
    Ok(Rc::new(CValue::Table(rows)))
}

/// Evaluates one linearization expression.
pub fn eval_cexpr(
    helpers: &Helpers,
    env: &CEnv,
    expr: &CExpr,
    fuel: EvalFuel,
) -> Result<Rc<CValue>, ConcreteError> {
    let fuel = spend(fuel)?;
    match expr {
        CExpr::Name(name) => eval_name(helpers, env, name, fuel),
        CExpr::Text(words) => Ok(Rc::new(CValue::Text(
            words
                .iter()
                .map(|word| crate::concrete::value::StrItem::Word(word.clone()))
                .collect(),
        ))),
        CExpr::Empty => Ok(Rc::new(CValue::Text(Vec::new()))),
        CExpr::Join(left, right) => {
            let first = eval_cexpr(helpers, env, left, fuel)?;
            let second = eval_cexpr(helpers, env, right, fuel)?;
            match (first.as_ref(), second.as_ref()) {
                (CValue::Text(one), CValue::Text(two)) => Ok(Rc::new(CValue::Text(
                    one.iter().cloned().chain(two.iter().cloned()).collect(),
                ))),
                (CValue::Text(_), other) => Err(ConcreteError::NotAString {
                    found: shape_of(other),
                }),
                (other, _) => Err(ConcreteError::NotAString {
                    found: shape_of(other),
                }),
            }
        }
        CExpr::Record(fields) => {
            let mut built: Vec<(FieldName, CValue)> = Vec::new();
            for (name, body) in fields {
                if built.iter().any(|(held, _)| held == name) {
                    return Err(ConcreteError::DuplicateField {
                        field: name.clone(),
                    });
                }
                let value = eval_cexpr(helpers, env, body, fuel)?;
                built.push((name.clone(), value.as_ref().clone()));
            }
            Ok(Rc::new(CValue::Record(built)))
        }
        CExpr::Project(body, field) => {
            let value = eval_cexpr(helpers, env, body, fuel)?;
            match value.as_ref() {
                CValue::Record(fields) => match fields.iter().find(|(held, _)| held == field) {
                    Some((_, inner)) => Ok(Rc::new(inner.clone())),
                    None => Err(ConcreteError::MissingField {
                        field: field.clone(),
                        available: fields.iter().map(|(held, _)| held.clone()).collect(),
                    }),
                },
                other => Err(ConcreteError::NotARecord {
                    found: shape_of(other),
                }),
            }
        }
        CExpr::Table(branches) => eval_table(helpers, env, branches, fuel),
        CExpr::Select(table, key) => {
            let rows = eval_cexpr(helpers, env, table, fuel)?;
            let chosen = eval_cexpr(helpers, env, key, fuel)?;
            let wanted = match chosen.as_ref() {
                CValue::Param(value) => value.clone(),
                other => {
                    return Err(ConcreteError::NotAParameter {
                        found: shape_of(other),
                    })
                }
            };
            match rows.as_ref() {
                CValue::Table(held) => match held.iter().find(|(row, _)| *row == wanted) {
                    Some((_, value)) => Ok(Rc::new(value.clone())),
                    None => Err(ConcreteError::MissingRow { value: wanted }),
                },
                other => Err(ConcreteError::NotATable {
                    found: shape_of(other),
                }),
            }
        }
        CExpr::Case(subject, branches) => {
            let value = eval_cexpr(helpers, env, subject, fuel)?;
            let wanted = match value.as_ref() {
                CValue::Param(held) => held.clone(),
                other => {
                    return Err(ConcreteError::NotAParameter {
                        found: shape_of(other),
                    })
                }
            };
            match branches
                .iter()
                .find(|(pattern, _)| matches(pattern, &wanted))
            {
                Some((_, body)) => eval_cexpr(helpers, env, body, fuel),
                None => Err(ConcreteError::NoBranchMatches { value: wanted }),
            }
        }
        CExpr::Lam(names, body) => Ok(Rc::new(CValue::Closure {
            names: names.clone(),
            body: Rc::new(body.as_ref().clone()),
            env: env.clone(),
        })),
        CExpr::App(function, argument) => {
            let head = eval_cexpr(helpers, env, function, fuel)?;
            let value = eval_cexpr(helpers, env, argument, fuel)?;
            apply_value(helpers, head, value, fuel)
        }
    }
}
