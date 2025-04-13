//! The bracketlint formatter.

#![feature(impl_trait_in_assoc_type, try_trait_v2, let_chains)]

use adapters::ExternalLanguagesEngineAdaptor;
use bl_ast::{AstVisitorMutSelf, SourceId};
use bl_reporting::Reports;
use bl_workspace::Member;

pub use crate::options::FormatterOptions;

mod adapters;
mod backends;
mod diagnostics;
mod options;
mod visitor;

fn configured_formatter() -> impl ExternalLanguagesEngineAdaptor {
    backends::biome::BiomeFormatter::new()
}

/// A query for formatting the document of a [`bl_workspace::Member`].
pub struct FormatQuery<'q> {
    pub options: FormatterOptions,
    pub source: SourceId,
    pub member: &'q Member,
}

/// The result of formatting a [`bl_ast::Document`]. This returns the formatted
/// content and the diagnostics that were generated during the formatting.
pub struct FormatQueryResult {
    pub buffer: String,
    pub diagnostics: Reports,
}

/// A query that returns the result of formatting a [`bl_ast::Document`].
pub fn fmt_module(query: FormatQuery) -> FormatQueryResult {
    let FormatQuery { member, source, options } = query;

    let spanned = member.spanned();
    let Some(ref document) = member.document else {
        panic!("Attempted to format a module without a document.");
    };

    // We assume that the formatting will be near to the original length of the
    // document.
    let buffer = String::with_capacity(spanned.len());
    let engine = configured_formatter();
    let mut formatter = visitor::Formatter::new(engine, options, source, spanned, buffer);

    match formatter.visit_document(document.ast_ref()) {
        Ok(_) => FormatQueryResult { buffer: formatter.into_buffer(), diagnostics: Reports::new() },
        Err(error) => {
            FormatQueryResult { buffer: formatter.into_buffer(), diagnostics: Reports::from(error) }
        }
    }
}
