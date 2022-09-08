use crate::error::*;

use pest::{
    error::{Error, ErrorVariant},
    iterators::{Pair, Pairs},
    Parser,
};

#[derive(Parser)]
#[grammar = "../pest/grammar.pest"]
pub struct AyParser;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Statement {
    FunDec {
        name: String,
        args: Vec<String>,
        body: Vec<Statement>,
    },
    VarDec {
        names: Vec<String>,
        values: Vec<Expr>,
    },
    Expr(Expr),
    If {
        cond: Expr,
        then: Vec<Statement>,
        otherwise: Vec<Statement>,
    },
    Loop {
        cond: Option<Expr>,
        body: Vec<Statement>,
    },
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Expr {
    FunCall { name: String, args: Vec<Expr> },
    Number(i64),
    String(String),
    Ident(String),
    Negated(Box<Expr>),
}

/// Pushes new error onto stacktrace or returns pred(pair).
fn handle<F, T>(parent: &Pair<Rule>, pair: Pair<Rule>, pred: F) -> Result<T, Trace>
where
    F: FnOnce(Pair<Rule>) -> Result<T, Trace>,
{
    let (span, rule) = (parent.as_span(), parent.as_rule());
    pred(pair).map_err(|mut trace| {
        trace.push(
            Stage::Parsing,
            Error::new_from_span(
                ErrorVariant::ParsingError {
                    positives: vec![rule],
                    negatives: vec![],
                },
                span,
            ),
        );
        trace
    })
}

macro_rules! fields {
    ($pair:ident |> $($field:ident),*) => {
        $(
            let $field = $pair.next().unwrap();
        )+
    };
}

fn build_ast_from_expr(pair: Pair<Rule>) -> Result<Expr, Trace> {
    match pair.as_rule() {
        Rule::negation => {
            let mut children = pair.into_inner();
            fields!(children |> expr);

            // Desired expr is wrapped in a Rule::expr
            Ok(Expr::Negated(Box::new(build_ast_from_expr(
                expr.into_inner().next().unwrap(),
            )?)))
        }
        Rule::number => {
            let span = pair.as_span();
            let mut elems = span.as_str().split_whitespace();
            let number = elems.next().unwrap();
            let mult: i64 = match elems.next() {
                Some("melo") => 2,
                Some("pxelo") => 3,
                None => 1,
                _ => unimplemented!("We shouldn't be here"),
            };

            let result = i64::from_str_radix(number, 8).map_err(|_| {
                Trace::new(
                    Stage::Parsing,
                    Error::new_from_span(
                        ErrorVariant::ParsingError {
                            positives: vec![],
                            negatives: vec![],
                        },
                        span,
                    ),
                )
            })? * mult;

            Ok(Expr::Number(result))
        }
        Rule::string => Ok(Expr::String(pair.as_span().as_str().to_owned())),
        Rule::ident => Ok(Expr::Ident(pair.as_span().as_str().to_owned())),
        rule => Err(Trace::new(
            Stage::AstBuilding,
            Error::new_from_span(
                ErrorVariant::CustomError {
                    message: format!("Missing expression-generating rule `{:?}` handling", rule),
                },
                pair.as_span(),
            ),
        )),
    }
}

fn build_ast_from_statement(pair: Pair<Rule>) -> Result<Statement, Trace> {
    match pair.as_rule() {
        Rule::expr => Ok(Statement::Expr(build_ast_from_expr(
            pair.into_inner().next().unwrap(),
        )?)),
        Rule::var_dec => {
            let span = pair.as_span();

            let mut idents = Vec::<Pair<Rule>>::new();
            let mut values = Vec::<Pair<Rule>>::new();

            pair.into_inner().for_each(|child| {
                if child.as_rule() == Rule::ident {
                    idents.push(child);
                } else {
                    values.push(child);
                }
            });

            if idents.len() != values.len() {
                return Err(Trace::new(
                    Stage::Parsing,
                    Error::new_from_span(
                        ErrorVariant::ParsingError {
                            positives: vec![Rule::var_dec],
                            negatives: vec![],
                        },
                        span,
                    ),
                ));
            }

            Ok(Statement::VarDec {
                names: idents
                    .iter()
                    .map(|ident| ident.as_span().as_str().to_owned())
                    .collect(),

                values: values
                    .iter()
                    .map(|value| build_ast_from_expr(value.clone()))
                    .collect::<Result<Vec<Expr>, Trace>>()?,
            })
        }
        Rule::if_block => {
            let mut children = pair.clone().into_inner();
            fields!(children |> cond, then);

            let cond = build_ast_from_expr(cond.into_inner().next().unwrap())?;

            let then = then
                .into_inner()
                .map(|statement| handle(&pair, statement, build_ast_from_statement))
                .collect::<Result<Vec<Statement>, Trace>>()?;

            // The else case is not mandatory
            if let Some(otherwise) = children.next() {
                let otherwise = otherwise
                    .into_inner()
                    .map(|statement| handle(&pair, statement, build_ast_from_statement))
                    .collect::<Result<Vec<Statement>, Trace>>()?;

                Ok(Statement::If {
                    cond,
                    then,
                    otherwise,
                })
            } else {
                Ok(Statement::If {
                    cond,
                    then,
                    otherwise: vec![],
                })
            }
        }
        Rule::statement => Ok(build_ast_from_statement(pair.into_inner().next().unwrap())?),
        rule => Err(Trace::new(
            Stage::AstBuilding,
            Error::new_from_span(
                ErrorVariant::CustomError {
                    message: format!("Missing statement-generating rule `{:?}` handling", rule),
                },
                pair.as_span(),
            ),
        )),
    }
}

pub fn parse(source: &str) -> Result<Vec<Statement>, Trace> {
    let mut ast = vec![];

    let pairs = AyParser::parse(Rule::program, source)?;

    for pair in pairs.clone() {
        recursive_print(Some(&pair), 0);
    }

    for pair in pairs {
        match pair.as_rule() {
            Rule::statement => ast.push(build_ast_from_statement(pair)?),
            Rule::EOI => {}
            unknown_rule => Err(Error::new_from_span(
                ErrorVariant::CustomError {
                    message: format!("Unknown rule: {:?}", unknown_rule),
                },
                pair.as_span(),
            ))?,
        }
    }

    Ok(ast)
}

pub fn recursive_print(cur: Option<&Pair<Rule>>, depth: u8) {
    if let Some(node) = cur {
        let rule = node.as_rule();

        let indent = (0..depth)
            .map(|_| "\x1b[32m|   \x1b[0m")
            .collect::<String>();

        println!(
            "{}\x1b[1;33m{:?}\x1b[0m:'{}'",
            indent,
            rule,
            node.as_span()
                .as_str()
                .lines()
                .map(|line| line.trim())
                .collect::<String>()
        );

        for pair in node.clone().into_inner() {
            recursive_print(Some(&pair), depth + 1);
        }
    }
}
