//! Parse error → miette conversion for Keel.
//!
//! Converts a `Vec<Simple<Token>>` from chumsky into a single rich
//! [`miette::Report`] with source-span labels, suitable for display.

use chumsky::prelude::*;
use miette::NamedSource;

use crate::lexer::Token;

pub(super) fn into_miette(
    errors: Vec<Simple<Token>>,
    named_src: &NamedSource<String>,
) -> miette::Report {
    let first = &errors[0];
    let labels: Vec<_> = errors
        .iter()
        .map(|e| miette::LabeledSpan::at(e.span(), e.to_string()))
        .collect();
    miette::miette!(labels = labels, "Parse error: {}", first).with_source_code(named_src.clone())
}
