//! Constant declarations, and the check that the whole signature is well
//! formed.
//!
//! The data at the top of this module holds the constants a grammar declares.
//! The one function at the bottom checks that every declared type is a type.
//! Nothing here mutates a signature; the checker reads it and returns a list.

use std::collections::BTreeMap;

use crate::error::TypeError;
use crate::mltt::check::{check_is_type, empty_context};
use crate::mltt::term::{ConstName, Term};
use crate::newtype::EvalFuel;

/// One declaration in the grammar signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstDecl {
    /// The name the declaration introduces.
    pub name: ConstName,
    /// The declared type, as a closed term.
    pub ty: Term,
}

/// The full set of constants a grammar declares.
///
/// The order list keeps declaration order, so every report the tool prints
/// stays stable across runs. The types map holds each declared type.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Signature {
    /// The declared names, in the order the source file writes them.
    pub order: Vec<ConstName>,
    /// The declared types, keyed by name.
    pub types: BTreeMap<ConstName, Term>,
}

/// Reads the declared type of a constant.
pub fn lookup_constant<'a>(signature: &'a Signature, name: &ConstName) -> Option<&'a Term> {
    signature.types.get(name)
}

/// Checks that every declared type in a signature is a type.
///
/// The checker reads each declaration against the whole signature, so two
/// declarations may name each other in either order.
pub fn check_signature(signature: &Signature, fuel: EvalFuel) -> Vec<(ConstName, TypeError)> {
    signature
        .order
        .iter()
        .filter_map(|name| {
            let declared = lookup_constant(signature, name)?;
            match check_is_type(signature, &empty_context(), declared, fuel) {
                Ok(_) => None,
                Err(failure) => Some((name.clone(), failure)),
            }
        })
        .collect()
}
