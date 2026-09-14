//! Tests for the core type theory.

use std::collections::BTreeMap;

use gf_mltt::error::TypeError;
use gf_mltt::mltt::check::{check, empty_context, infer_closed};
use gf_mltt::mltt::signature::{check_signature, Signature};
use gf_mltt::mltt::eval::{convertible, eval, normalize};
use gf_mltt::mltt::term::{Binder, ConstName, Term};
use gf_mltt::mltt::value::Env;
use gf_mltt::newtype::{CatName, DeBruijnIndex, EvalFuel, FunName, UniverseLevel, VarName};

fn fuel() -> EvalFuel {
    EvalFuel(4096)
}

fn binder(name: &str) -> Binder {
    Binder(VarName(name.to_string()))
}

fn cat(name: &str) -> Term {
    Term::Const(ConstName::Category(CatName(name.to_string())))
}

fn fun(name: &str) -> Term {
    Term::Const(ConstName::Function(FunName(name.to_string())))
}

fn universe(level: u32) -> Term {
    Term::Universe(UniverseLevel(level))
}

fn var(index: usize) -> Term {
    Term::Var(DeBruijnIndex(index))
}

fn pi(name: &str, domain: Term, codomain: Term) -> Term {
    Term::Pi {
        binder: binder(name),
        domain: Box::new(domain),
        codomain: Box::new(codomain),
    }
}

fn arrow(domain: Term, codomain: Term) -> Term {
    pi("_", domain, codomain)
}

fn lam(name: &str, body: Term) -> Term {
    Term::Lam {
        binder: binder(name),
        body: Box::new(body),
    }
}

fn app(function: Term, argument: Term) -> Term {
    Term::App {
        function: Box::new(function),
        argument: Box::new(argument),
    }
}

fn sigma(name: &str, first: Term, second: Term) -> Term {
    Term::Sigma {
        binder: binder(name),
        first: Box::new(first),
        second: Box::new(second),
    }
}

fn pair(first: Term, second: Term) -> Term {
    Term::Pair {
        first: Box::new(first),
        second: Box::new(second),
    }
}

fn identity(ty: Term, left: Term, right: Term) -> Term {
    Term::Id {
        ty: Box::new(ty),
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn refl(ty: Term, point: Term) -> Term {
    Term::Refl {
        ty: Box::new(ty),
        point: Box::new(point),
    }
}

/// Builds a signature that holds two categories and two constructors.
fn sample_signature() -> Signature {
    let entries: Vec<(ConstName, Term)> = vec![
        (
            ConstName::Category(CatName("Prop".to_string())),
            universe(0),
        ),
        (ConstName::Category(CatName("Ind".to_string())), universe(0)),
        (
            ConstName::Function(FunName("Barks".to_string())),
            arrow(cat("Ind"), cat("Prop")),
        ),
        (ConstName::Function(FunName("John".to_string())), cat("Ind")),
        (ConstName::Function(FunName("Fido".to_string())), cat("Ind")),
    ];
    Signature {
        order: entries.iter().map(|(name, _)| name.clone()).collect(),
        types: entries.into_iter().collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn a_universe_sits_in_the_next_universe() {
    let signature = Signature::default();
    let found = infer_closed(&signature, &universe(0), fuel()).unwrap();
    assert_eq!(found, universe(1));
}

#[test]
fn the_signature_checks() {
    let signature = sample_signature();
    assert_eq!(check_signature(&signature, fuel()), vec![]);
}

#[test]
fn a_constructor_applies_to_an_argument() {
    let signature = sample_signature();
    let term = app(fun("Barks"), fun("John"));
    let found = infer_closed(&signature, &term, fuel()).unwrap();
    assert_eq!(found, cat("Prop"));
}

#[test]
fn the_checker_rejects_an_argument_of_the_wrong_type() {
    let signature = sample_signature();
    let term = app(fun("Barks"), fun("Barks"));
    let failure = infer_closed(&signature, &term, fuel()).unwrap_err();
    match failure {
        TypeError::Mismatch { expected, found } => {
            assert_eq!(expected, cat("Ind"));
            assert_eq!(found, arrow(cat("Ind"), cat("Prop")));
        }
        other => panic!("wanted a mismatch, found {other:?}"),
    }
}

#[test]
fn the_polymorphic_identity_function_checks() {
    let signature = Signature::default();
    let ty = pi("A", universe(0), arrow(var(0), var(1)));
    let term = lam("A", lam("x", var(0)));
    let context = empty_context();
    let expected = eval(&Env::Empty, &ty, fuel()).unwrap();
    check(&signature, &context, &term, expected.as_ref(), fuel()).unwrap();
}

#[test]
fn a_pi_type_over_a_universe_sits_one_level_up() {
    let signature = Signature::default();
    let ty = pi("A", universe(0), arrow(var(0), var(1)));
    let found = infer_closed(&signature, &ty, fuel()).unwrap();
    assert_eq!(found, universe(1));
}

#[test]
fn evaluation_fires_a_redex() {
    let signature = sample_signature();
    let term = app(lam("x", var(0)), fun("John"));
    let normal = normalize(&Env::Empty, &term, fuel()).unwrap();
    assert_eq!(normal, fun("John"));
    let found = infer_closed(&signature, &term, fuel());
    assert!(found.is_err(), "an unannotated function carries no type on its own");
}

#[test]
fn conversion_treats_a_function_as_equal_to_its_expansion() {
    let plain = eval(&Env::Empty, &fun("Barks"), fuel()).unwrap();
    let expanded = eval(&Env::Empty, &lam("x", app(fun("Barks"), var(0))), fuel()).unwrap();
    let same = convertible(
        gf_mltt::newtype::DeBruijnLevel(0),
        plain.as_ref(),
        expanded.as_ref(),
        fuel(),
    )
    .unwrap();
    assert!(same, "eta expansion must not change a function");
}

#[test]
fn a_pair_checks_against_a_pair_type() {
    let signature = sample_signature();
    let ty = sigma("x", cat("Ind"), cat("Prop"));
    let value = pair(fun("John"), app(fun("Barks"), fun("John")));
    let expected = eval(&Env::Empty, &ty, fuel()).unwrap();
    check(&signature, &empty_context(), &value, expected.as_ref(), fuel()).unwrap();
    let first = normalize(&Env::Empty, &Term::First(Box::new(value)), fuel()).unwrap();
    assert_eq!(first, fun("John"));
}

#[test]
fn a_reflexive_proof_carries_an_identity_type() {
    let signature = sample_signature();
    let proof = refl(cat("Ind"), fun("John"));
    let found = infer_closed(&signature, &proof, fuel()).unwrap();
    assert_eq!(found, identity(cat("Ind"), fun("John"), fun("John")));
}

#[test]
fn the_checker_rejects_a_proof_of_a_false_identity() {
    let signature = sample_signature();
    let wanted = identity(cat("Ind"), fun("John"), fun("Fido"));
    let expected = eval(&Env::Empty, &wanted, fuel()).unwrap();
    let proof = refl(cat("Ind"), fun("John"));
    let failure = check(
        &signature,
        &empty_context(),
        &proof,
        expected.as_ref(),
        fuel(),
    )
    .unwrap_err();
    assert!(matches!(failure, TypeError::Mismatch { .. }));
}

#[test]
fn identity_elimination_reduces_on_a_reflexive_proof() {
    let signature = sample_signature();
    let elimination = Term::J {
        level: UniverseLevel(0),
        ty: Box::new(cat("Ind")),
        left: Box::new(fun("John")),
        motive: Box::new(lam("b", lam("p", cat("Prop")))),
        base: Box::new(app(fun("Barks"), fun("John"))),
        right: Box::new(fun("John")),
        proof: Box::new(refl(cat("Ind"), fun("John"))),
    };
    let found = infer_closed(&signature, &elimination, fuel()).unwrap();
    assert_eq!(found, cat("Prop"));
    let normal = normalize(&Env::Empty, &elimination, fuel()).unwrap();
    assert_eq!(normal, app(fun("Barks"), fun("John")));
}

#[test]
fn identity_elimination_rejects_a_motive_of_the_wrong_shape() {
    let signature = sample_signature();
    let elimination = Term::J {
        level: UniverseLevel(0),
        ty: Box::new(cat("Ind")),
        left: Box::new(fun("John")),
        motive: Box::new(lam("b", cat("Prop"))),
        base: Box::new(app(fun("Barks"), fun("John"))),
        right: Box::new(fun("John")),
        proof: Box::new(refl(cat("Ind"), fun("John"))),
    };
    assert!(infer_closed(&signature, &elimination, fuel()).is_err());
}

#[test]
fn the_checker_stops_when_the_fuel_runs_out() {
    let signature = sample_signature();
    let term = app(fun("Barks"), fun("John"));
    let failure = infer_closed(&signature, &term, EvalFuel(2)).unwrap_err();
    assert_eq!(failure, TypeError::OutOfFuel);
}
