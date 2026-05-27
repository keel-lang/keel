//! Parse error → miette conversion for Keel.
//!
//! Converts a `Vec<Simple<Token>>` from chumsky into a single rich
//! [`miette::Report`] with source-span labels, suitable for display.

use chumsky::prelude::*;
use miette::NamedSource;

use crate::lexer::{Span, Token};

pub(super) fn into_miette(
    errors: Vec<Simple<Token>>,
    named_src: &NamedSource<String>,
) -> miette::Report {
    let err = &errors[0];
    let span: Span = err.span();
    miette::miette!(
        labels = vec![miette::LabeledSpan::at(span, err.to_string())],
        "Parse error: {}",
        err
    )
    .with_source_code(named_src.clone())
}
