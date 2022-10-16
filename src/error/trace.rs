use super::{error::Error, span::Span};

use crate::parsing::*;

use pest::{
    error::{Error as PestError, ErrorVariant, LineColLocation},
    iterators::{Pair, Pairs},
};

use std::fmt::Debug;

#[derive(Debug)]
pub enum Stage {
    Unknown,
    Parsing,
    AstBuilding,
    Binding,
    Typing,
    Compiling,
}

pub trait TraceError: Debug {
    fn from_span(span: Span, message: &str) -> Self
    where
        Self: Sized;
    fn line_col(&self) -> LineColLocation;
    fn line(&self) -> &str;
    fn message(&self) -> &str;
}

#[derive(Debug, Default)]
pub struct Trace {
    stack: Vec<(Stage, Box<dyn TraceError>)>,
}

impl<T: TraceError + 'static> From<(Stage, T)> for Trace {
    fn from((stage, err): (Stage, T)) -> Self {
        Trace {
            stack: vec![(stage, Box::new(err))],
        }
    }
}

impl<T: TraceError + 'static> From<T> for Trace {
    fn from(err: T) -> Self {
        Trace {
            stack: vec![(Stage::Unknown, Box::new(err))],
        }
    }
}

impl Trace {
    pub fn new<T: TraceError + 'static>(stage: Stage, err: T) -> Self {
        Trace {
            stack: vec![(stage, Box::new(err))],
        }
    }

    pub fn new_from_pair(pair: &Pair<Rule>, message: String) -> Self {
        let mut res = Trace::default();
        res.push_pest_error(Stage::Parsing, pair, message);
        res
    }

    pub fn push<T: TraceError + 'static>(&mut self, stage: Stage, err: T) {
        self.stack.push((stage, Box::new(err)))
    }

    pub fn push_pest_error(&mut self, stage: Stage, pair: &Pair<Rule>, message: String) {
        self.stack.push((
            stage,
            Box::new(Error::from_span(pair.as_span().into(), message.as_ref())),
        ))
    }
}

impl std::fmt::Display for Trace {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Deepest error first\n{}",
            self.stack
                .iter()
                .map(|(stage, err)| {
                    let line_nbr = match err.line_col() {
                        LineColLocation::Pos((y, _)) => y,
                        LineColLocation::Span((ys, _), _) => ys,
                    };

                    let line_nbr_len = line_nbr.to_string().len();

                    let padding = " ".repeat(line_nbr_len);

                    let arrow = format!("{}>", "-".repeat(line_nbr_len));

                    let coords = match err.line_col() {
                        LineColLocation::Pos((y, x)) => format!("{y}:{x}"),
                        LineColLocation::Span((ys, xs), (ye, xe)) => {
                            format!("{ys}:{xs} -> {ye}:{xe}")
                        }
                    };

                    let underline = match err.line_col() {
                        LineColLocation::Pos((_, x)) => format!("{}^", " ".repeat(x)),
                        LineColLocation::Span((ys, xs), (ye, xe)) => {
                            if ys == ye {
                                format!("{}^{}^", " ".repeat(xs), "-".repeat(xe - xs - 1))
                            } else {
                                format!("{}^{}", " ".repeat(xs), "-".repeat(err.line().len() - xs))
                            }
                        }
                    };

                    // ---> STAGE | COORDS
                    //    |
                    // NBR| LINE
                    //    | UNDERLINE
                    //    = ERROR
                    format!(
                        "{arrow} {stage:?} | {coords}\n\
                         {padding}|\n\
                         {}\n\
                         {padding}|{underline}\n\
                         {padding}= {}\n",
                        // Line number and line
                        format_args!("{}| {}", line_nbr, err.line()),
                        // Error
                        err.message()
                    )
                })
                .collect::<String>(),
        )
    }
}
