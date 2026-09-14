//! Turns abstract declarations into constants of the core type theory.
//!
//! This module holds behavior only.
//!
//! A category declaration becomes a constant whose type ends in a universe.
//! `cat Ind (k : Sort)` becomes `Ind : (k : Sort) -> Type0`. A constructor
//! declaration keeps the type the source file writes.
//!
//! Elaboration replaces every name with a de Bruijn index or a constant. A
//! binder that names several variables, such as `(x y : A) -> B`, elaborates
//! its domain once for each name, in the scope that name sees. That gives each
//! copy the right index without a separate shifting pass.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::LowerError;
use crate::gf::ast::{AbstractModule, SExpr};
use crate::mltt::signature::Signature;
use crate::mltt::term::{Binder, ConstName, Term};
use crate::newtype::{CatName, DeBruijnIndex, FunName, ModuleName, UniverseLevel, VarName};

/// How deep elaboration follows a nested type.
const MAX_DEPTH: u32 = 512;

/// The names a grammar declares, together with the kind of each name.
#[derive(Clone, Debug, Default)]
pub struct DeclaredNames {
    /// Every category name.
    pub categories: BTreeSet<String>,
    /// Every constructor name.
    pub functions: BTreeSet<String>,
}

/// The abstract half of a grammar after lowering.
#[derive(Clone, Debug)]
pub struct AbstractGrammar {
    /// The module name.
    pub name: ModuleName,
    /// Every declared constant with its type.
    pub signature: Signature,
    /// The category a sentence must produce.
    pub start: CatName,
    /// The categories, in declaration order.
    pub categories: Vec<CatName>,
    /// The constructors, in declaration order.
    pub functions: Vec<FunName>,
    /// How many arguments each category takes.
    pub category_arity: BTreeMap<CatName, usize>,
}

/// Reads the index a name carries in the current scope.
fn index_in_scope(scope: &[VarName], name: &str) -> Option<DeBruijnIndex> {
    scope
        .iter()
        .rev()
        .position(|bound| bound.0 == name)
        .map(DeBruijnIndex)
}

/// Replaces every name in one source expression with an index or a constant.
pub fn elaborate(
    names: &DeclaredNames,
    scope: &[VarName],
    expr: &SExpr,
    depth: u32,
) -> Result<Term, LowerError> {
    if depth > MAX_DEPTH {
        return Err(LowerError::NestingTooDeep);
    }
    match expr {
        SExpr::Name(name) => match index_in_scope(scope, name) {
            Some(index) => Ok(Term::Var(index)),
            None => {
                if names.categories.contains(name) {
                    Ok(Term::Const(ConstName::Category(CatName(name.clone()))))
                } else if names.functions.contains(name) {
                    Ok(Term::Const(ConstName::Function(FunName(name.clone()))))
                } else {
                    Err(LowerError::UnknownIdentifier { name: name.clone() })
                }
            }
        },
        SExpr::Universe(level) => Ok(Term::Universe(*level)),
        SExpr::Pi {
            names: bound,
            domain,
            codomain,
        } => elaborate_binder(names, scope, bound, domain, codomain, depth, true),
        SExpr::Sigma {
            names: bound,
            first,
            second,
        } => elaborate_binder(names, scope, bound, first, second, depth, false),
        SExpr::Lam { names: bound, body } => elaborate_lambda(names, scope, bound, body, depth),
        SExpr::App { function, argument } => Ok(Term::App {
            function: Box::new(elaborate(names, scope, function, depth + 1)?),
            argument: Box::new(elaborate(names, scope, argument, depth + 1)?),
        }),
        SExpr::Pair { first, second } => Ok(Term::Pair {
            first: Box::new(elaborate(names, scope, first, depth + 1)?),
            second: Box::new(elaborate(names, scope, second, depth + 1)?),
        }),
        SExpr::First(inner) => Ok(Term::First(Box::new(elaborate(
            names,
            scope,
            inner,
            depth + 1,
        )?))),
        SExpr::Second(inner) => Ok(Term::Second(Box::new(elaborate(
            names,
            scope,
            inner,
            depth + 1,
        )?))),
        SExpr::Id { ty, left, right } => Ok(Term::Id {
            ty: Box::new(elaborate(names, scope, ty, depth + 1)?),
            left: Box::new(elaborate(names, scope, left, depth + 1)?),
            right: Box::new(elaborate(names, scope, right, depth + 1)?),
        }),
        SExpr::Refl { ty, point } => Ok(Term::Refl {
            ty: Box::new(elaborate(names, scope, ty, depth + 1)?),
            point: Box::new(elaborate(names, scope, point, depth + 1)?),
        }),
        SExpr::Eliminate {
            level,
            ty,
            left,
            motive,
            base,
            right,
            proof,
        } => Ok(Term::J {
            level: *level,
            ty: Box::new(elaborate(names, scope, ty, depth + 1)?),
            left: Box::new(elaborate(names, scope, left, depth + 1)?),
            motive: Box::new(elaborate(names, scope, motive, depth + 1)?),
            base: Box::new(elaborate(names, scope, base, depth + 1)?),
            right: Box::new(elaborate(names, scope, right, depth + 1)?),
            proof: Box::new(elaborate(names, scope, proof, depth + 1)?),
        }),
    }
}

/// Elaborates a binder that names several variables at one type.
fn elaborate_binder(
    names: &DeclaredNames,
    scope: &[VarName],
    bound: &[VarName],
    domain: &SExpr,
    body: &SExpr,
    depth: u32,
    dependent_function: bool,
) -> Result<Term, LowerError> {
    if depth > MAX_DEPTH {
        return Err(LowerError::NestingTooDeep);
    }
    match bound.split_first() {
        None => elaborate(names, scope, body, depth + 1),
        Some((first, rest)) => {
            let domain_term = elaborate(names, scope, domain, depth + 1)?;
            let inner_scope: Vec<VarName> = scope
                .iter()
                .cloned()
                .chain(std::iter::once(first.clone()))
                .collect();
            let body_term = elaborate_binder(
                names,
                &inner_scope,
                rest,
                domain,
                body,
                depth + 1,
                dependent_function,
            )?;
            if dependent_function {
                Ok(Term::Pi {
                    binder: Binder(first.clone()),
                    domain: Box::new(domain_term),
                    codomain: Box::new(body_term),
                })
            } else {
                Ok(Term::Sigma {
                    binder: Binder(first.clone()),
                    first: Box::new(domain_term),
                    second: Box::new(body_term),
                })
            }
        }
    }
}

/// Elaborates a function that binds several variables.
fn elaborate_lambda(
    names: &DeclaredNames,
    scope: &[VarName],
    bound: &[VarName],
    body: &SExpr,
    depth: u32,
) -> Result<Term, LowerError> {
    if depth > MAX_DEPTH {
        return Err(LowerError::NestingTooDeep);
    }
    match bound.split_first() {
        None => elaborate(names, scope, body, depth + 1),
        Some((first, rest)) => {
            let inner_scope: Vec<VarName> = scope
                .iter()
                .cloned()
                .chain(std::iter::once(first.clone()))
                .collect();
            Ok(Term::Lam {
                binder: Binder(first.clone()),
                body: Box::new(elaborate_lambda(names, &inner_scope, rest, body, depth + 1)?),
            })
        }
    }
}

/// Builds the type of a category from the arguments it takes.
fn category_type(
    names: &DeclaredNames,
    scope: &[VarName],
    args: &[(VarName, SExpr)],
    depth: u32,
) -> Result<Term, LowerError> {
    if depth > MAX_DEPTH {
        return Err(LowerError::NestingTooDeep);
    }
    match args.split_first() {
        None => Ok(Term::Universe(UniverseLevel(0))),
        Some(((name, ty), rest)) => {
            let domain = elaborate(names, scope, ty, depth + 1)?;
            let inner_scope: Vec<VarName> = scope
                .iter()
                .cloned()
                .chain(std::iter::once(name.clone()))
                .collect();
            Ok(Term::Pi {
                binder: Binder(name.clone()),
                domain: Box::new(domain),
                codomain: Box::new(category_type(names, &inner_scope, rest, depth + 1)?),
            })
        }
    }
}

/// Collects every name the abstract module declares and reports repeats.
pub fn collect_names(module: &AbstractModule) -> Result<DeclaredNames, Vec<LowerError>> {
    let mut names = DeclaredNames::default();
    let mut failures: Vec<LowerError> = Vec::new();
    for declaration in &module.cats {
        if !names.categories.insert(declaration.name.0.clone()) {
            failures.push(LowerError::DuplicateDeclaration {
                name: declaration.name.0.clone(),
            });
        }
    }
    for declaration in &module.funs {
        let repeated = names.categories.contains(&declaration.name.0)
            || !names.functions.insert(declaration.name.0.clone());
        if repeated {
            failures.push(LowerError::DuplicateDeclaration {
                name: declaration.name.0.clone(),
            });
        }
    }
    if failures.is_empty() {
        Ok(names)
    } else {
        Err(failures)
    }
}

/// Turns an abstract module into a signature of the core type theory.
///
/// The caller may name the start category. When the caller names none, the
/// module must carry a `flags startcat` line.
pub fn lower_abstract(
    module: &AbstractModule,
    chosen_start: Option<&CatName>,
) -> Result<AbstractGrammar, Vec<LowerError>> {
    let names = collect_names(module)?;
    let mut order: Vec<ConstName> = Vec::new();
    let mut types: BTreeMap<ConstName, Term> = BTreeMap::new();
    let mut categories: Vec<CatName> = Vec::new();
    let mut functions: Vec<FunName> = Vec::new();
    let mut category_arity: BTreeMap<CatName, usize> = BTreeMap::new();
    let mut failures: Vec<LowerError> = Vec::new();

    for declaration in &module.cats {
        match category_type(&names, &[], &declaration.args, 0) {
            Ok(ty) => {
                let name = ConstName::Category(declaration.name.clone());
                order.push(name.clone());
                types.insert(name, ty);
                categories.push(declaration.name.clone());
                category_arity.insert(declaration.name.clone(), declaration.args.len());
            }
            Err(failure) => failures.push(failure),
        }
    }
    for declaration in &module.funs {
        match elaborate(&names, &[], &declaration.ty, 0) {
            Ok(ty) => {
                let name = ConstName::Function(declaration.name.clone());
                order.push(name.clone());
                types.insert(name, ty);
                functions.push(declaration.name.clone());
            }
            Err(failure) => failures.push(failure),
        }
    }

    let start = match chosen_start.or(module.start.as_ref()) {
        Some(name) => {
            if names.categories.contains(&name.0) {
                Some(name.clone())
            } else {
                failures.push(LowerError::UnknownStartCategory { name: name.clone() });
                None
            }
        }
        None => {
            failures.push(LowerError::MissingStartCategory);
            None
        }
    };

    match (failures.is_empty(), start) {
        (true, Some(start)) => Ok(AbstractGrammar {
            name: module.name.clone(),
            signature: Signature { order, types },
            start,
            categories,
            functions,
            category_arity,
        }),
        _ => Err(failures),
    }
}
