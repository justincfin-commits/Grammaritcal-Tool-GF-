//! Walks linearization types and the values that match them.
//!
//! This module holds behavior only.
//!
//! Four questions matter to the rest of the tool. Which parts of a category
//! print words? Which parts carry features? Which feature assignments exist?
//! And what does an argument look like before the parser knows which words
//! fill it? The functions below answer those four questions.

use std::collections::BTreeMap;

use crate::concrete::lintype::{FieldPath, LinType, ParamState, PathStep, ValueShape};
use crate::concrete::value::{CValue, GapSpec, Slot, StrItem};
use crate::error::{CompileError, ConcreteError};
use crate::newtype::{ArgIndex, CatName, ParamTypeName, ParamValue, StateLimit};

/// The values that each parameter type holds.
pub type ParamTypes = BTreeMap<ParamTypeName, Vec<ParamValue>>;

/// Names the kind of a value.
pub fn shape_of(value: &CValue) -> ValueShape {
    match value {
        CValue::Text(_) => ValueShape::Text,
        CValue::Param(_) => ValueShape::Parameter,
        CValue::Record(_) => ValueShape::Record,
        CValue::Table(_) => ValueShape::Table,
        CValue::Closure { .. } => ValueShape::Function,
        CValue::Binder { .. } => ValueShape::Binder,
    }
}

/// Adds one step to the end of a path.
fn extend(path: &FieldPath, step: PathStep) -> FieldPath {
    FieldPath(
        path.0
            .iter()
            .cloned()
            .chain(std::iter::once(step))
            .collect(),
    )
}

/// Reads the values of a parameter type, or reports that it is undeclared.
fn values_of(params: &ParamTypes, name: &ParamTypeName) -> Result<Vec<ParamValue>, CompileError> {
    match params.get(name) {
        Some(values) => Ok(values.clone()),
        None => Err(CompileError::UnknownParamType { name: name.clone() }),
    }
}

/// Lists every part of a category that prints words.
pub fn string_paths(ty: &LinType, params: &ParamTypes) -> Result<Vec<FieldPath>, CompileError> {
    string_paths_from(ty, params, &FieldPath::default())
}

fn string_paths_from(
    ty: &LinType,
    params: &ParamTypes,
    path: &FieldPath,
) -> Result<Vec<FieldPath>, CompileError> {
    match ty {
        LinType::Str => Ok(vec![path.clone()]),
        LinType::Param(_) => Ok(Vec::new()),
        LinType::Record(fields) => {
            let mut found: Vec<FieldPath> = Vec::new();
            for (name, inner) in fields {
                found.extend(string_paths_from(
                    inner,
                    params,
                    &extend(path, PathStep::Field(name.clone())),
                )?);
            }
            Ok(found)
        }
        LinType::Table(param_type, inner) => {
            let mut found: Vec<FieldPath> = Vec::new();
            for value in values_of(params, param_type)? {
                found.extend(string_paths_from(
                    inner,
                    params,
                    &extend(path, PathStep::Row(value)),
                )?);
            }
            Ok(found)
        }
    }
}

/// Lists every part of a category that carries a feature.
pub fn param_paths(
    ty: &LinType,
    params: &ParamTypes,
) -> Result<Vec<(FieldPath, ParamTypeName)>, CompileError> {
    param_paths_from(ty, params, &FieldPath::default())
}

fn param_paths_from(
    ty: &LinType,
    params: &ParamTypes,
    path: &FieldPath,
) -> Result<Vec<(FieldPath, ParamTypeName)>, CompileError> {
    match ty {
        LinType::Str => Ok(Vec::new()),
        LinType::Param(name) => {
            let _ = values_of(params, name)?;
            Ok(vec![(path.clone(), name.clone())])
        }
        LinType::Record(fields) => {
            let mut found: Vec<(FieldPath, ParamTypeName)> = Vec::new();
            for (name, inner) in fields {
                found.extend(param_paths_from(
                    inner,
                    params,
                    &extend(path, PathStep::Field(name.clone())),
                )?);
            }
            Ok(found)
        }
        LinType::Table(param_type, inner) => {
            let mut found: Vec<(FieldPath, ParamTypeName)> = Vec::new();
            for value in values_of(params, param_type)? {
                found.extend(param_paths_from(
                    inner,
                    params,
                    &extend(path, PathStep::Row(value)),
                )?);
            }
            Ok(found)
        }
    }
}

/// Lists every feature assignment that a category allows.
///
/// The count grows as the product of the sizes of the feature types, so the
/// caller supplies a limit and the function reports a category that passes it.
pub fn enumerate_states(
    cat: &CatName,
    ty: &LinType,
    params: &ParamTypes,
    limit: StateLimit,
) -> Result<Vec<ParamState>, CompileError> {
    let slots = param_paths(ty, params)?;
    let total = slots.iter().try_fold(1usize, |count, (_, param_type)| {
        let width = params.get(param_type).map(|values| values.len()).unwrap_or(0);
        count.checked_mul(width.max(1)).ok_or(())
    });
    let total = match total {
        Ok(value) => value,
        Err(()) => usize::MAX,
    };
    if total > limit.0 {
        return Err(CompileError::StateExplosion {
            cat: cat.clone(),
            count: total,
            limit,
        });
    }
    let mut states: Vec<ParamState> = vec![ParamState::default()];
    for (path, param_type) in slots {
        let values = values_of(params, &param_type)?;
        states = states
            .into_iter()
            .flat_map(|state| {
                let path = path.clone();
                values.iter().map(move |value| {
                    let mut pairs = state.0.clone();
                    pairs.push((path.clone(), value.clone()));
                    pairs.sort();
                    ParamState(pairs)
                })
            })
            .collect::<Vec<ParamState>>();
    }
    Ok(states)
}

/// Reads the feature value that a state gives one part.
fn state_at(state: &ParamState, path: &FieldPath) -> Option<ParamValue> {
    state
        .0
        .iter()
        .find(|(held, _)| held == path)
        .map(|(_, value)| value.clone())
}

/// Builds the value of an argument before the parser knows its words.
///
/// Every part that prints words becomes a slot. Every part that carries a
/// feature takes its value from the state the caller chose.
pub fn symbolic_value(
    ty: &LinType,
    params: &ParamTypes,
    state: &ParamState,
    arg: ArgIndex,
    gap: &Option<GapSpec>,
) -> Result<CValue, ConcreteError> {
    symbolic_value_from(ty, params, state, arg, gap, &FieldPath::default())
}

fn symbolic_value_from(
    ty: &LinType,
    params: &ParamTypes,
    state: &ParamState,
    arg: ArgIndex,
    gap: &Option<GapSpec>,
    path: &FieldPath,
) -> Result<CValue, ConcreteError> {
    match ty {
        LinType::Str => Ok(CValue::Text(vec![StrItem::Slot(Slot {
            arg,
            path: path.clone(),
            gap: gap.clone(),
        })])),
        LinType::Param(name) => match state_at(state, path) {
            Some(value) => Ok(CValue::Param(value)),
            None => Err(ConcreteError::UnknownParamTypeName { name: name.clone() }),
        },
        LinType::Record(fields) => {
            let mut built: Vec<(crate::newtype::FieldName, CValue)> = Vec::new();
            for (name, inner) in fields {
                built.push((
                    name.clone(),
                    symbolic_value_from(
                        inner,
                        params,
                        state,
                        arg,
                        gap,
                        &extend(path, PathStep::Field(name.clone())),
                    )?,
                ));
            }
            Ok(CValue::Record(built))
        }
        LinType::Table(param_type, inner) => {
            let values = match params.get(param_type) {
                Some(values) => values.clone(),
                None => {
                    return Err(ConcreteError::UnknownParamTypeName {
                        name: param_type.clone(),
                    })
                }
            };
            let mut rows: Vec<(ParamValue, CValue)> = Vec::new();
            for value in values {
                rows.push((
                    value.clone(),
                    symbolic_value_from(
                        inner,
                        params,
                        state,
                        arg,
                        gap,
                        &extend(path, PathStep::Row(value)),
                    )?,
                ));
            }
            Ok(CValue::Table(rows))
        }
    }
}

/// Reads the feature values that a finished value carries.
///
/// The walk follows the declared linearization type, so a value of the wrong
/// shape reports a mismatch instead of a wrong answer.
pub fn state_of_value(
    value: &CValue,
    ty: &LinType,
    params: &ParamTypes,
) -> Result<ParamState, ConcreteError> {
    let mut pairs: Vec<(FieldPath, ParamValue)> = Vec::new();
    collect_state(value, ty, params, &FieldPath::default(), &mut pairs)?;
    pairs.sort();
    Ok(ParamState(pairs))
}

fn collect_state(
    value: &CValue,
    ty: &LinType,
    params: &ParamTypes,
    path: &FieldPath,
    found: &mut Vec<(FieldPath, ParamValue)>,
) -> Result<(), ConcreteError> {
    match (ty, value) {
        (LinType::Str, CValue::Text(_)) => Ok(()),
        (LinType::Param(_), CValue::Param(held)) => {
            found.push((path.clone(), held.clone()));
            Ok(())
        }
        (LinType::Record(fields), CValue::Record(held)) => {
            for (name, inner) in fields {
                match held.iter().find(|(field, _)| field == name) {
                    Some((_, inner_value)) => collect_state(
                        inner_value,
                        inner,
                        params,
                        &extend(path, PathStep::Field(name.clone())),
                        found,
                    )?,
                    None => {
                        return Err(ConcreteError::MissingField {
                            field: name.clone(),
                            available: held.iter().map(|(field, _)| field.clone()).collect(),
                        })
                    }
                }
            }
            Ok(())
        }
        (LinType::Table(param_type, inner), CValue::Table(rows)) => {
            let values = match params.get(param_type) {
                Some(values) => values.clone(),
                None => {
                    return Err(ConcreteError::UnknownParamTypeName {
                        name: param_type.clone(),
                    })
                }
            };
            for value_name in values {
                match rows.iter().find(|(held, _)| *held == value_name) {
                    Some((_, row_value)) => collect_state(
                        row_value,
                        inner,
                        params,
                        &extend(path, PathStep::Row(value_name.clone())),
                        found,
                    )?,
                    None => {
                        return Err(ConcreteError::MissingRow {
                            value: value_name.clone(),
                        })
                    }
                }
            }
            Ok(())
        }
        (_, other) => Err(ConcreteError::ShapeMismatch {
            found: shape_of(other),
        }),
    }
}

/// Reads the word list that sits at one part of a value.
pub fn text_at_path(value: &CValue, path: &FieldPath) -> Result<Vec<StrItem>, ConcreteError> {
    let mut here = value;
    for step in &path.0 {
        here = match (step, here) {
            (PathStep::Field(name), CValue::Record(fields)) => {
                match fields.iter().find(|(field, _)| field == name) {
                    Some((_, inner)) => inner,
                    None => {
                        return Err(ConcreteError::MissingField {
                            field: name.clone(),
                            available: fields.iter().map(|(field, _)| field.clone()).collect(),
                        })
                    }
                }
            }
            (PathStep::Row(name), CValue::Table(rows)) => {
                match rows.iter().find(|(row, _)| row == name) {
                    Some((_, inner)) => inner,
                    None => return Err(ConcreteError::MissingRow { value: name.clone() }),
                }
            }
            (PathStep::Field(_), other) => {
                return Err(ConcreteError::NotARecord {
                    found: shape_of(other),
                })
            }
            (PathStep::Row(_), other) => {
                return Err(ConcreteError::NotATable {
                    found: shape_of(other),
                })
            }
        };
    }
    match here {
        CValue::Text(items) => Ok(items.clone()),
        other => Err(ConcreteError::NotAString {
            found: shape_of(other),
        }),
    }
}

/// Answers whether every word list inside a value is empty.
pub fn prints_nothing(value: &CValue) -> bool {
    match value {
        CValue::Text(items) => items.is_empty(),
        CValue::Param(_) => true,
        CValue::Record(fields) => fields.iter().all(|(_, inner)| prints_nothing(inner)),
        CValue::Table(rows) => rows.iter().all(|(_, inner)| prints_nothing(inner)),
        CValue::Closure { .. } => false,
        CValue::Binder { .. } => false,
    }
}
