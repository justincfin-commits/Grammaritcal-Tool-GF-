//! Linearization type data.
//!
//! This module holds data only. The module `crate::concrete::shape` holds the
//! behavior that walks these types.
//!
//! A linearization type says what shape the words of one category take. `Str`
//! is a run of words. A record holds named parts. A table holds one value per
//! parameter value. A parameter field carries a feature such as number, which
//! the words do not show but the rules read.

use crate::newtype::{FieldName, ParamTypeName, ParamValue};

/// The shape the words of one category take.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinType {
    /// A run of words.
    Str,
    /// A feature that the rules read and the words do not show.
    Param(ParamTypeName),
    /// Named parts, held in the order the source file writes them.
    Record(Vec<(FieldName, LinType)>),
    /// One value for each value of a parameter type.
    Table(ParamTypeName, Box<LinType>),
}

/// One step into a linearization type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathStep {
    /// Enter a named part.
    Field(FieldName),
    /// Enter one row of a table.
    Row(ParamValue),
}

/// A route from the top of a linearization type down to one leaf.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FieldPath(pub Vec<PathStep>);

/// One assignment of a parameter value to every parameter leaf of a
/// linearization type. The pairs stay sorted by path, so two states that agree
/// compare equal.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ParamState(pub Vec<(FieldPath, ParamValue)>);

/// What kind of value an expression produced. Errors carry this so that a
/// report can name the mismatch without printing the whole value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueShape {
    /// A run of words.
    Text,
    /// A parameter value.
    Parameter,
    /// A record.
    Record,
    /// A table.
    Table,
    /// A helper that still waits for arguments.
    Function,
    /// A higher-order argument that still waits for its bound variable.
    Binder,
}

/// One parameter type declaration, such as `param Num = Sg | Pl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamTypeDecl {
    /// The name of the parameter type.
    pub name: ParamTypeName,
    /// The values the type holds, in the order the source file writes them.
    pub values: Vec<ParamValue>,
}
