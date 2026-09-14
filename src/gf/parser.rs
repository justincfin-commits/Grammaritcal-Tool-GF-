//! Reads the shape of a grammar source file from its tokens.
//!
//! This module holds behavior only. Each function takes the token list and a
//! position, and returns a value together with the next position. No function
//! changes the token list.
//!
//! The parser refuses to nest brackets deeper than `MAX_DEPTH`, so a hostile
//! file cannot drive it into an endless descent.

use crate::concrete::lintype::{LinType, ParamTypeDecl};
use crate::error::{ParseError, SourcePos};
use crate::gf::ast::{
    AbstractModule, CExpr, CPattern, CatDecl, ConcreteModule, FunDecl, LinCatDecl, LinDecl,
    OperDecl, SExpr,
};
use crate::gf::token::{Lexeme, Tok};
use crate::newtype::{
    CatName, FieldName, FunName, ModuleName, OperName, ParamTypeName, ParamValue, Token,
    UniverseLevel, VarName,
};

/// A value together with the position the parser reached.
type Parsed<T> = Result<(T, usize), ParseError>;

/// How deep the parser follows nested brackets.
const MAX_DEPTH: u32 = 64;

/// Names one token in a report.
fn describe(token: &Tok) -> String {
    match token {
        Tok::Name(name) => format!("the name {name}"),
        Tok::Number(value) => format!("the number {value}"),
        Tok::Text(words) => format!("the words {}", words.join(" ")),
        Tok::Universe(level) => format!("the universe Type{}", level.0),
        Tok::Abstract => "abstract".to_string(),
        Tok::Concrete => "concrete".to_string(),
        Tok::Of => "of".to_string(),
        Tok::Cat => "cat".to_string(),
        Tok::Fun => "fun".to_string(),
        Tok::Flags => "flags".to_string(),
        Tok::Param => "param".to_string(),
        Tok::Lincat => "lincat".to_string(),
        Tok::Lin => "lin".to_string(),
        Tok::Oper => "oper".to_string(),
        Tok::Table => "table".to_string(),
        Tok::Case => "case".to_string(),
        Tok::Str => "Str".to_string(),
        Tok::IdType => "Id".to_string(),
        Tok::Refl => "refl".to_string(),
        Tok::Eliminate => "J".to_string(),
        Tok::First => "fst".to_string(),
        Tok::Second => "snd".to_string(),
        Tok::Wildcard => "_".to_string(),
        Tok::Semicolon => "a semicolon".to_string(),
        Tok::Comma => "a comma".to_string(),
        Tok::Colon => "a colon".to_string(),
        Tok::Dot => "a full stop".to_string(),
        Tok::Select => "an exclamation mark".to_string(),
        Tok::Equals => "an equals sign".to_string(),
        Tok::Arrow => "an arrow".to_string(),
        Tok::TableArrow => "a table arrow".to_string(),
        Tok::Join => "the join operator".to_string(),
        Tok::Times => "the pair type operator".to_string(),
        Tok::Backslash => "a backslash".to_string(),
        Tok::OpenParen => "an opening round bracket".to_string(),
        Tok::CloseParen => "a closing round bracket".to_string(),
        Tok::OpenBrace => "an opening curly bracket".to_string(),
        Tok::CloseBrace => "a closing curly bracket".to_string(),
        Tok::OpenBracket => "an opening square bracket".to_string(),
        Tok::CloseBracket => "a closing square bracket".to_string(),
        Tok::Bar => "a vertical bar".to_string(),
        Tok::OpenAngle => "an opening angle bracket".to_string(),
        Tok::CloseAngle => "a closing angle bracket".to_string(),
    }
}

/// Reads the token at a position.
fn look(tokens: &[Lexeme], at: usize) -> Option<&Tok> {
    tokens.get(at).map(|lexeme| &lexeme.token)
}

/// Reads the position of the token at a position, or of the last token.
fn where_at(tokens: &[Lexeme], at: usize) -> SourcePos {
    match tokens.get(at).or_else(|| tokens.last()) {
        Some(lexeme) => lexeme.pos,
        None => SourcePos {
            line: crate::newtype::Line(1),
            column: crate::newtype::Column(1),
        },
    }
}

/// Steps past one token, or reports what the parser wanted.
fn expect(tokens: &[Lexeme], at: usize, want: &Tok, label: &str) -> Result<usize, ParseError> {
    match tokens.get(at) {
        Some(lexeme) if &lexeme.token == want => Ok(at + 1),
        Some(lexeme) => Err(ParseError::Expected {
            pos: lexeme.pos,
            expected: label.to_string(),
            found: describe(&lexeme.token),
        }),
        None => Err(ParseError::UnexpectedEnd {
            expected: label.to_string(),
        }),
    }
}

/// Reads one name.
fn expect_name(tokens: &[Lexeme], at: usize) -> Parsed<String> {
    match tokens.get(at) {
        Some(lexeme) => match &lexeme.token {
            Tok::Name(name) => Ok((name.clone(), at + 1)),
            other => Err(ParseError::Expected {
                pos: lexeme.pos,
                expected: "a name".to_string(),
                found: describe(other),
            }),
        },
        None => Err(ParseError::UnexpectedEnd {
            expected: "a name".to_string(),
        }),
    }
}

/// Stops the parser when a file nests brackets too deeply.
fn guard_depth(tokens: &[Lexeme], at: usize, depth: u32) -> Result<(), ParseError> {
    if depth > MAX_DEPTH {
        Err(ParseError::NestingTooDeep {
            pos: where_at(tokens, at),
        })
    } else {
        Ok(())
    }
}

/// Answers whether a token may start an atom of a type or a term.
fn starts_sexpr_atom(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Name(_)
            | Tok::Universe(_)
            | Tok::OpenParen
            | Tok::OpenAngle
            | Tok::IdType
            | Tok::Refl
            | Tok::Eliminate
            | Tok::First
            | Tok::Second
    )
}

/// Answers whether a round bracket opens a binder group, such as `(x : A)`.
fn opens_binder(tokens: &[Lexeme], at: usize) -> bool {
    if look(tokens, at) != Some(&Tok::OpenParen) {
        return false;
    }
    let mut here = at + 1;
    let mut seen_name = false;
    loop {
        match look(tokens, here) {
            Some(Tok::Name(_)) => {
                seen_name = true;
                here += 1;
            }
            Some(Tok::Wildcard) => {
                seen_name = true;
                here += 1;
            }
            Some(Tok::Colon) => return seen_name,
            _ => return false,
        }
    }
}

/// Reads one or more names that a binder group introduces.
fn parse_binder_names(tokens: &[Lexeme], at: usize) -> Parsed<Vec<VarName>> {
    let mut names: Vec<VarName> = Vec::new();
    let mut here = at;
    loop {
        match look(tokens, here) {
            Some(Tok::Name(name)) => {
                names.push(VarName(name.clone()));
                here += 1;
            }
            Some(Tok::Wildcard) => {
                names.push(VarName("_".to_string()));
                here += 1;
            }
            _ => break,
        }
    }
    if names.is_empty() {
        return Err(ParseError::Expected {
            pos: where_at(tokens, here),
            expected: "a bound name".to_string(),
            found: look(tokens, here).map(describe).unwrap_or_default(),
        });
    }
    Ok((names, here))
}

/// Reads one atom of a type or a term.
fn parse_sexpr_atom(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<SExpr> {
    guard_depth(tokens, at, depth)?;
    match look(tokens, at) {
        Some(Tok::Name(name)) => Ok((SExpr::Name(name.clone()), at + 1)),
        Some(Tok::Universe(level)) => Ok((SExpr::Universe(*level), at + 1)),
        Some(Tok::OpenParen) => {
            let (inner, next) = parse_sexpr(tokens, at + 1, depth + 1)?;
            let next = expect(tokens, next, &Tok::CloseParen, "a closing round bracket")?;
            Ok((inner, next))
        }
        Some(Tok::OpenAngle) => {
            let (first, next) = parse_sexpr(tokens, at + 1, depth + 1)?;
            let next = expect(tokens, next, &Tok::Comma, "a comma")?;
            let (second, next) = parse_sexpr(tokens, next, depth + 1)?;
            let next = expect(tokens, next, &Tok::CloseAngle, "a closing angle bracket")?;
            Ok((
                SExpr::Pair {
                    first: Box::new(first),
                    second: Box::new(second),
                },
                next,
            ))
        }
        Some(Tok::First) => {
            let (inner, next) = parse_sexpr_atom(tokens, at + 1, depth + 1)?;
            Ok((SExpr::First(Box::new(inner)), next))
        }
        Some(Tok::Second) => {
            let (inner, next) = parse_sexpr_atom(tokens, at + 1, depth + 1)?;
            Ok((SExpr::Second(Box::new(inner)), next))
        }
        Some(Tok::IdType) => {
            let (ty, next) = parse_sexpr_atom(tokens, at + 1, depth + 1)?;
            let (left, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
            let (right, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
            Ok((
                SExpr::Id {
                    ty: Box::new(ty),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                next,
            ))
        }
        Some(Tok::Refl) => {
            let (ty, next) = parse_sexpr_atom(tokens, at + 1, depth + 1)?;
            let (point, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
            Ok((
                SExpr::Refl {
                    ty: Box::new(ty),
                    point: Box::new(point),
                },
                next,
            ))
        }
        Some(Tok::Eliminate) => parse_eliminate(tokens, at + 1, depth + 1),
        Some(other) => Err(ParseError::Expected {
            pos: where_at(tokens, at),
            expected: "a type or a term".to_string(),
            found: describe(other),
        }),
        None => Err(ParseError::UnexpectedEnd {
            expected: "a type or a term".to_string(),
        }),
    }
}

/// Reads an identity elimination, which names its universe first.
fn parse_eliminate(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<SExpr> {
    guard_depth(tokens, at, depth)?;
    let (level, next) = match look(tokens, at) {
        Some(Tok::Number(value)) => (UniverseLevel(*value), at + 1),
        Some(other) => {
            return Err(ParseError::Expected {
                pos: where_at(tokens, at),
                expected: "the universe level of the motive".to_string(),
                found: describe(other),
            })
        }
        None => {
            return Err(ParseError::UnexpectedEnd {
                expected: "the universe level of the motive".to_string(),
            })
        }
    };
    let (ty, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
    let (left, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
    let (motive, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
    let (base, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
    let (right, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
    let (proof, next) = parse_sexpr_atom(tokens, next, depth + 1)?;
    Ok((
        SExpr::Eliminate {
            level,
            ty: Box::new(ty),
            left: Box::new(left),
            motive: Box::new(motive),
            base: Box::new(base),
            right: Box::new(right),
            proof: Box::new(proof),
        },
        next,
    ))
}

/// Reads a run of atoms and folds them into applications.
fn parse_sexpr_app(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<SExpr> {
    guard_depth(tokens, at, depth)?;
    let (head, mut here) = parse_sexpr_atom(tokens, at, depth + 1)?;
    let mut result = head;
    while let Some(token) = look(tokens, here) {
        if !starts_sexpr_atom(token) || opens_binder(tokens, here) {
            break;
        }
        let (argument, next) = parse_sexpr_atom(tokens, here, depth + 1)?;
        result = SExpr::App {
            function: Box::new(result),
            argument: Box::new(argument),
        };
        here = next;
    }
    Ok((result, here))
}

/// Reads one type or term.
pub fn parse_sexpr(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<SExpr> {
    guard_depth(tokens, at, depth)?;
    if look(tokens, at) == Some(&Tok::Backslash) {
        let start = if look(tokens, at + 1) == Some(&Tok::Backslash) {
            at + 2
        } else {
            at + 1
        };
        let (names, next) = parse_lambda_names(tokens, start)?;
        let next = match look(tokens, next) {
            Some(Tok::Arrow) | Some(Tok::TableArrow) => next + 1,
            _ => expect(tokens, next, &Tok::Arrow, "an arrow after the bound names")?,
        };
        let (body, next) = parse_sexpr(tokens, next, depth + 1)?;
        return Ok((
            SExpr::Lam {
                names,
                body: Box::new(body),
            },
            next,
        ));
    }
    if opens_binder(tokens, at) {
        let (names, next) = parse_binder_names(tokens, at + 1)?;
        let next = expect(tokens, next, &Tok::Colon, "a colon")?;
        let (domain, next) = parse_sexpr(tokens, next, depth + 1)?;
        let next = expect(tokens, next, &Tok::CloseParen, "a closing round bracket")?;
        return match look(tokens, next) {
            Some(Tok::Arrow) => {
                let (codomain, next) = parse_sexpr(tokens, next + 1, depth + 1)?;
                Ok((
                    SExpr::Pi {
                        names,
                        domain: Box::new(domain),
                        codomain: Box::new(codomain),
                    },
                    next,
                ))
            }
            Some(Tok::Times) => {
                let (second, next) = parse_sexpr(tokens, next + 1, depth + 1)?;
                Ok((
                    SExpr::Sigma {
                        names,
                        first: Box::new(domain),
                        second: Box::new(second),
                    },
                    next,
                ))
            }
            _ => Err(ParseError::Expected {
                pos: where_at(tokens, next),
                expected: "an arrow or a pair type operator after a binder".to_string(),
                found: look(tokens, next).map(describe).unwrap_or_default(),
            }),
        };
    }
    let (left, next) = parse_sexpr_app(tokens, at, depth + 1)?;
    match look(tokens, next) {
        Some(Tok::Arrow) => {
            let (codomain, next) = parse_sexpr(tokens, next + 1, depth + 1)?;
            Ok((
                SExpr::Pi {
                    names: vec![VarName("_".to_string())],
                    domain: Box::new(left),
                    codomain: Box::new(codomain),
                },
                next,
            ))
        }
        Some(Tok::Times) => {
            let (second, next) = parse_sexpr(tokens, next + 1, depth + 1)?;
            Ok((
                SExpr::Sigma {
                    names: vec![VarName("_".to_string())],
                    first: Box::new(left),
                    second: Box::new(second),
                },
                next,
            ))
        }
        _ => Ok((left, next)),
    }
}

/// Reads the names a function binds, separated by commas.
fn parse_lambda_names(tokens: &[Lexeme], at: usize) -> Parsed<Vec<VarName>> {
    let (first, mut here) = expect_name(tokens, at)?;
    let mut names = vec![VarName(first)];
    while look(tokens, here) == Some(&Tok::Comma) {
        let (next_name, next) = expect_name(tokens, here + 1)?;
        names.push(VarName(next_name));
        here = next;
    }
    Ok((names, here))
}

/// Reads one linearization type.
fn parse_lintype(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<LinType> {
    guard_depth(tokens, at, depth)?;
    let (left, next) = parse_lintype_atom(tokens, at, depth + 1)?;
    if look(tokens, next) == Some(&Tok::TableArrow) {
        let (right, after) = parse_lintype(tokens, next + 1, depth + 1)?;
        return match left {
            LinType::Param(name) => Ok((LinType::Table(name, Box::new(right)), after)),
            _ => Err(ParseError::Expected {
                pos: where_at(tokens, at),
                expected: "a parameter type on the left of a table arrow".to_string(),
                found: "another linearization type".to_string(),
            }),
        };
    }
    Ok((left, next))
}

/// Reads one atom of a linearization type.
fn parse_lintype_atom(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<LinType> {
    guard_depth(tokens, at, depth)?;
    match look(tokens, at) {
        Some(Tok::Str) => Ok((LinType::Str, at + 1)),
        Some(Tok::Name(name)) => Ok((LinType::Param(ParamTypeName(name.clone())), at + 1)),
        Some(Tok::OpenParen) => {
            let (inner, next) = parse_lintype(tokens, at + 1, depth + 1)?;
            let next = expect(tokens, next, &Tok::CloseParen, "a closing round bracket")?;
            Ok((inner, next))
        }
        Some(Tok::OpenBrace) => {
            let mut fields: Vec<(FieldName, LinType)> = Vec::new();
            let mut here = at + 1;
            while look(tokens, here) != Some(&Tok::CloseBrace) {
                let (field, next) = expect_name(tokens, here)?;
                let next = expect(tokens, next, &Tok::Colon, "a colon")?;
                let (ty, next) = parse_lintype(tokens, next, depth + 1)?;
                fields.push((FieldName(field), ty));
                here = match look(tokens, next) {
                    Some(Tok::Semicolon) => next + 1,
                    _ => next,
                };
                if here == next && look(tokens, here) != Some(&Tok::CloseBrace) {
                    return Err(ParseError::Expected {
                        pos: where_at(tokens, here),
                        expected: "a semicolon or a closing curly bracket".to_string(),
                        found: look(tokens, here).map(describe).unwrap_or_default(),
                    });
                }
            }
            Ok((LinType::Record(fields), here + 1))
        }
        Some(other) => Err(ParseError::Expected {
            pos: where_at(tokens, at),
            expected: "a linearization type".to_string(),
            found: describe(other),
        }),
        None => Err(ParseError::UnexpectedEnd {
            expected: "a linearization type".to_string(),
        }),
    }
}

/// Answers whether a token may start an atom of a linearization expression.
fn starts_cexpr_atom(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Text(_)
            | Tok::OpenBracket
            | Tok::Name(_)
            | Tok::OpenBrace
            | Tok::OpenParen
            | Tok::Table
            | Tok::Case
    )
}

/// Reads the rows of a table or the branches of a case.
fn parse_branches(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<Vec<(CPattern, CExpr)>> {
    let mut here = expect(tokens, at, &Tok::OpenBrace, "an opening curly bracket")?;
    let mut branches: Vec<(CPattern, CExpr)> = Vec::new();
    while look(tokens, here) != Some(&Tok::CloseBrace) {
        let (pattern, next) = match look(tokens, here) {
            Some(Tok::Wildcard) => (CPattern::Wildcard, here + 1),
            Some(Tok::Name(name)) => (CPattern::Value(ParamValue(name.clone())), here + 1),
            Some(other) => {
                return Err(ParseError::Expected {
                    pos: where_at(tokens, here),
                    expected: "a parameter value or an underscore".to_string(),
                    found: describe(other),
                })
            }
            None => {
                return Err(ParseError::UnexpectedEnd {
                    expected: "a parameter value or an underscore".to_string(),
                })
            }
        };
        let next = expect(tokens, next, &Tok::TableArrow, "a table arrow")?;
        let (body, next) = parse_cexpr(tokens, next, depth + 1)?;
        branches.push((pattern, body));
        here = match look(tokens, next) {
            Some(Tok::Semicolon) => next + 1,
            _ => next,
        };
        if here == next && look(tokens, here) != Some(&Tok::CloseBrace) {
            return Err(ParseError::Expected {
                pos: where_at(tokens, here),
                expected: "a semicolon or a closing curly bracket".to_string(),
                found: look(tokens, here).map(describe).unwrap_or_default(),
            });
        }
    }
    Ok((branches, here + 1))
}

/// Reads one atom of a linearization expression.
fn parse_cexpr_atom(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<CExpr> {
    guard_depth(tokens, at, depth)?;
    match look(tokens, at) {
        Some(Tok::Text(words)) => Ok((
            CExpr::Text(words.iter().map(|word| Token(word.clone())).collect()),
            at + 1,
        )),
        Some(Tok::OpenBracket) => {
            let next = expect(tokens, at + 1, &Tok::CloseBracket, "a closing square bracket")?;
            Ok((CExpr::Empty, next))
        }
        Some(Tok::Name(name)) => Ok((CExpr::Name(name.clone()), at + 1)),
        Some(Tok::OpenParen) => {
            let (inner, next) = parse_cexpr(tokens, at + 1, depth + 1)?;
            let next = expect(tokens, next, &Tok::CloseParen, "a closing round bracket")?;
            Ok((inner, next))
        }
        Some(Tok::Table) => {
            let (branches, next) = parse_branches(tokens, at + 1, depth + 1)?;
            Ok((CExpr::Table(branches), next))
        }
        Some(Tok::Case) => {
            let (subject, next) = parse_cexpr(tokens, at + 1, depth + 1)?;
            let next = expect(tokens, next, &Tok::Of, "the word of")?;
            let (branches, next) = parse_branches(tokens, next, depth + 1)?;
            Ok((CExpr::Case(Box::new(subject), branches), next))
        }
        Some(Tok::OpenBrace) => {
            let mut fields: Vec<(FieldName, CExpr)> = Vec::new();
            let mut here = at + 1;
            while look(tokens, here) != Some(&Tok::CloseBrace) {
                let (field, next) = expect_name(tokens, here)?;
                let next = expect(tokens, next, &Tok::Equals, "an equals sign")?;
                let (body, next) = parse_cexpr(tokens, next, depth + 1)?;
                fields.push((FieldName(field), body));
                here = match look(tokens, next) {
                    Some(Tok::Semicolon) => next + 1,
                    _ => next,
                };
                if here == next && look(tokens, here) != Some(&Tok::CloseBrace) {
                    return Err(ParseError::Expected {
                        pos: where_at(tokens, here),
                        expected: "a semicolon or a closing curly bracket".to_string(),
                        found: look(tokens, here).map(describe).unwrap_or_default(),
                    });
                }
            }
            Ok((CExpr::Record(fields), here + 1))
        }
        Some(other) => Err(ParseError::Expected {
            pos: where_at(tokens, at),
            expected: "a linearization expression".to_string(),
            found: describe(other),
        }),
        None => Err(ParseError::UnexpectedEnd {
            expected: "a linearization expression".to_string(),
        }),
    }
}

/// Reads an atom together with the field projections that follow it, and
/// nothing else. The row after an exclamation mark reads this way, so that
/// `t ! x.n` selects with `x.n` and `t ! a ! b` still selects twice in order.
fn parse_cexpr_projected(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<CExpr> {
    guard_depth(tokens, at, depth)?;
    let (head, mut here) = parse_cexpr_atom(tokens, at, depth + 1)?;
    let mut result = head;
    while look(tokens, here) == Some(&Tok::Dot) {
        let (field, next) = expect_name(tokens, here + 1)?;
        result = CExpr::Project(Box::new(result), FieldName(field));
        here = next;
    }
    Ok((result, here))
}

/// Reads an atom together with the field projections and row selections that
/// follow it.
fn parse_cexpr_postfix(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<CExpr> {
    guard_depth(tokens, at, depth)?;
    let (head, mut here) = parse_cexpr_atom(tokens, at, depth + 1)?;
    let mut result = head;
    loop {
        match look(tokens, here) {
            Some(Tok::Dot) => {
                let (field, next) = expect_name(tokens, here + 1)?;
                result = CExpr::Project(Box::new(result), FieldName(field));
                here = next;
            }
            Some(Tok::Select) => {
                let (row, next) = parse_cexpr_projected(tokens, here + 1, depth + 1)?;
                result = CExpr::Select(Box::new(result), Box::new(row));
                here = next;
            }
            _ => return Ok((result, here)),
        }
    }
}

/// Reads a run of postfix expressions and folds them into applications.
fn parse_cexpr_app(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<CExpr> {
    guard_depth(tokens, at, depth)?;
    let (head, mut here) = parse_cexpr_postfix(tokens, at, depth + 1)?;
    let mut result = head;
    while let Some(token) = look(tokens, here) {
        if !starts_cexpr_atom(token) {
            break;
        }
        let (argument, next) = parse_cexpr_postfix(tokens, here, depth + 1)?;
        result = CExpr::App(Box::new(result), Box::new(argument));
        here = next;
    }
    Ok((result, here))
}

/// Reads one linearization expression.
pub fn parse_cexpr(tokens: &[Lexeme], at: usize, depth: u32) -> Parsed<CExpr> {
    guard_depth(tokens, at, depth)?;
    if look(tokens, at) == Some(&Tok::Backslash) {
        let start = if look(tokens, at + 1) == Some(&Tok::Backslash) {
            at + 2
        } else {
            at + 1
        };
        let (names, next) = parse_lambda_names(tokens, start)?;
        let next = match look(tokens, next) {
            Some(Tok::Arrow) | Some(Tok::TableArrow) => next + 1,
            _ => expect(tokens, next, &Tok::Arrow, "an arrow after the bound names")?,
        };
        let (body, next) = parse_cexpr(tokens, next, depth + 1)?;
        return Ok((CExpr::Lam(names, Box::new(body)), next));
    }
    let (left, next) = parse_cexpr_app(tokens, at, depth + 1)?;
    if look(tokens, next) == Some(&Tok::Join) {
        let (right, after) = parse_cexpr(tokens, next + 1, depth + 1)?;
        return Ok((CExpr::Join(Box::new(left), Box::new(right)), after));
    }
    Ok((left, next))
}

/// Reads the binder groups that a category declaration writes.
fn parse_cat_args(tokens: &[Lexeme], at: usize) -> Parsed<Vec<(VarName, SExpr)>> {
    let mut here = at;
    let mut args: Vec<(VarName, SExpr)> = Vec::new();
    while opens_binder(tokens, here) {
        let (names, next) = parse_binder_names(tokens, here + 1)?;
        let next = expect(tokens, next, &Tok::Colon, "a colon")?;
        let (ty, next) = parse_sexpr(tokens, next, 0)?;
        let next = expect(tokens, next, &Tok::CloseParen, "a closing round bracket")?;
        args.extend(names.into_iter().map(|name| (name, ty.clone())));
        here = next;
    }
    Ok((args, here))
}

/// Reads the abstract half of a grammar.
pub fn parse_abstract(tokens: &[Lexeme]) -> Result<AbstractModule, ParseError> {
    let at = expect(tokens, 0, &Tok::Abstract, "the word abstract")?;
    let (name, at) = expect_name(tokens, at)?;
    let at = expect(tokens, at, &Tok::Equals, "an equals sign")?;
    let mut here = expect(tokens, at, &Tok::OpenBrace, "an opening curly bracket")?;
    let mut cats: Vec<CatDecl> = Vec::new();
    let mut funs: Vec<FunDecl> = Vec::new();
    let mut start: Option<CatName> = None;
    while look(tokens, here) != Some(&Tok::CloseBrace) {
        match look(tokens, here) {
            Some(Tok::Cat) => {
                let pos = where_at(tokens, here);
                let (cat_name, next) = expect_name(tokens, here + 1)?;
                let (args, next) = parse_cat_args(tokens, next)?;
                let next = expect(tokens, next, &Tok::Semicolon, "a semicolon")?;
                cats.push(CatDecl {
                    name: CatName(cat_name),
                    args,
                    pos,
                });
                here = next;
            }
            Some(Tok::Fun) => {
                let pos = where_at(tokens, here);
                let (first, next) = expect_name(tokens, here + 1)?;
                let mut names = vec![first];
                let mut next = next;
                while look(tokens, next) == Some(&Tok::Comma) {
                    let (another, after) = expect_name(tokens, next + 1)?;
                    names.push(another);
                    next = after;
                }
                let next = expect(tokens, next, &Tok::Colon, "a colon")?;
                let (ty, next) = parse_sexpr(tokens, next, 0)?;
                let next = expect(tokens, next, &Tok::Semicolon, "a semicolon")?;
                funs.extend(names.into_iter().map(|fun_name| FunDecl {
                    name: FunName(fun_name),
                    ty: ty.clone(),
                    pos,
                }));
                here = next;
            }
            Some(Tok::Flags) => {
                let (flag, next) = expect_name(tokens, here + 1)?;
                let next = expect(tokens, next, &Tok::Equals, "an equals sign")?;
                let (value, next) = expect_name(tokens, next)?;
                let next = expect(tokens, next, &Tok::Semicolon, "a semicolon")?;
                if flag == "startcat" {
                    start = Some(CatName(value));
                }
                here = next;
            }
            Some(other) => {
                return Err(ParseError::Expected {
                    pos: where_at(tokens, here),
                    expected: "cat, fun, flags, or a closing curly bracket".to_string(),
                    found: describe(other),
                })
            }
            None => {
                return Err(ParseError::UnexpectedEnd {
                    expected: "a closing curly bracket".to_string(),
                })
            }
        }
    }
    Ok(AbstractModule {
        name: ModuleName(name),
        start,
        cats,
        funs,
    })
}

/// Reads a run of plain names, which a rule uses for its arguments.
fn parse_plain_names(tokens: &[Lexeme], at: usize) -> (Vec<VarName>, usize) {
    let mut names: Vec<VarName> = Vec::new();
    let mut here = at;
    while let Some(Tok::Name(name)) = look(tokens, here) {
        names.push(VarName(name.clone()));
        here += 1;
    }
    (names, here)
}

/// Steps past a type annotation that the tool records but does not check.
fn skip_annotation(tokens: &[Lexeme], at: usize) -> Result<usize, ParseError> {
    let mut here = at;
    loop {
        match look(tokens, here) {
            Some(Tok::Equals) => return Ok(here),
            Some(Tok::Semicolon) | None => {
                return Err(ParseError::Expected {
                    pos: where_at(tokens, here),
                    expected: "an equals sign after a type annotation".to_string(),
                    found: look(tokens, here).map(describe).unwrap_or_default(),
                })
            }
            Some(_) => here += 1,
        }
    }
}

/// Reads the concrete half of a grammar.
pub fn parse_concrete(tokens: &[Lexeme]) -> Result<ConcreteModule, ParseError> {
    let at = expect(tokens, 0, &Tok::Concrete, "the word concrete")?;
    let (name, at) = expect_name(tokens, at)?;
    let at = expect(tokens, at, &Tok::Of, "the word of")?;
    let (abstract_name, at) = expect_name(tokens, at)?;
    let at = expect(tokens, at, &Tok::Equals, "an equals sign")?;
    let mut here = expect(tokens, at, &Tok::OpenBrace, "an opening curly bracket")?;
    let mut params: Vec<ParamTypeDecl> = Vec::new();
    let mut lincats: Vec<LinCatDecl> = Vec::new();
    let mut lins: Vec<LinDecl> = Vec::new();
    let mut opers: Vec<OperDecl> = Vec::new();
    while look(tokens, here) != Some(&Tok::CloseBrace) {
        match look(tokens, here) {
            Some(Tok::Param) => {
                let (type_name, next) = expect_name(tokens, here + 1)?;
                let next = expect(tokens, next, &Tok::Equals, "an equals sign")?;
                let (first, mut next) = expect_name(tokens, next)?;
                let mut values = vec![ParamValue(first)];
                while look(tokens, next) == Some(&Tok::Bar) {
                    let (another, after) = expect_name(tokens, next + 1)?;
                    values.push(ParamValue(another));
                    next = after;
                }
                let next = expect(tokens, next, &Tok::Semicolon, "a semicolon")?;
                params.push(ParamTypeDecl {
                    name: ParamTypeName(type_name),
                    values,
                });
                here = next;
            }
            Some(Tok::Lincat) => {
                let pos = where_at(tokens, here);
                let (cat_name, next) = expect_name(tokens, here + 1)?;
                let next = expect(tokens, next, &Tok::Equals, "an equals sign")?;
                let (ty, next) = parse_lintype(tokens, next, 0)?;
                let next = expect(tokens, next, &Tok::Semicolon, "a semicolon")?;
                lincats.push(LinCatDecl {
                    cat: CatName(cat_name),
                    ty,
                    pos,
                });
                here = next;
            }
            Some(Tok::Lin) => {
                let pos = where_at(tokens, here);
                let (fun_name, next) = expect_name(tokens, here + 1)?;
                let (args, next) = parse_plain_names(tokens, next);
                let next = expect(tokens, next, &Tok::Equals, "an equals sign")?;
                let (body, next) = parse_cexpr(tokens, next, 0)?;
                let next = expect(tokens, next, &Tok::Semicolon, "a semicolon")?;
                lins.push(LinDecl {
                    fun: FunName(fun_name),
                    args,
                    body,
                    pos,
                });
                here = next;
            }
            Some(Tok::Oper) => {
                let pos = where_at(tokens, here);
                let (oper_name, next) = expect_name(tokens, here + 1)?;
                let (args, next) = parse_plain_names(tokens, next);
                let next = match look(tokens, next) {
                    Some(Tok::Colon) => skip_annotation(tokens, next + 1)?,
                    _ => next,
                };
                let next = expect(tokens, next, &Tok::Equals, "an equals sign")?;
                let (body, next) = parse_cexpr(tokens, next, 0)?;
                let next = expect(tokens, next, &Tok::Semicolon, "a semicolon")?;
                opers.push(OperDecl {
                    name: OperName(oper_name),
                    args,
                    body,
                    pos,
                });
                here = next;
            }
            Some(other) => {
                return Err(ParseError::Expected {
                    pos: where_at(tokens, here),
                    expected: "param, lincat, lin, oper, or a closing curly bracket".to_string(),
                    found: describe(other),
                })
            }
            None => {
                return Err(ParseError::UnexpectedEnd {
                    expected: "a closing curly bracket".to_string(),
                })
            }
        }
    }
    Ok(ConcreteModule {
        name: ModuleName(name),
        abstract_name: ModuleName(abstract_name),
        params,
        lincats,
        lins,
        opers,
    })
}
