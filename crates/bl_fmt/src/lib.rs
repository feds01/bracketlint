//! The bracketlint formatter.

#![feature(impl_trait_in_assoc_type, try_trait_v2, try_trait_v2_residual)]

use adapters::ExternalLanguagesEngineAdaptor;
use bl_ast::{AstNodeRef, AstVisitorMutSelf, Document, SourceId};
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

/// A query for formatting the parsed document of a [`bl_workspace::Member`].
pub struct FormatQuery<'q> {
    pub options: FormatterOptions,
    pub source: SourceId,
    pub member: &'q Member,
    pub document: AstNodeRef<'q, Document<'q>>,
}

/// The result of formatting a [`bl_ast::Document`]. This returns the formatted
/// content and the diagnostics that were generated during the formatting.
pub struct FormatQueryResult {
    pub buffer: String,
    pub diagnostics: Reports,
}

/// A query that returns the result of formatting a [`bl_ast::Document`].
pub fn fmt_module(query: FormatQuery) -> FormatQueryResult {
    let FormatQuery { member, source, options, document } = query;

    let spanned = member.spanned();

    // We assume that the formatting will be near to the original length of the
    // document.
    let buffer = String::with_capacity(spanned.len());
    let engine = configured_formatter();
    let mut formatter =
        visitor::Formatter::new(engine, options, source, spanned, member.dialect, buffer);

    match formatter.visit_document(document) {
        Ok(_) => FormatQueryResult { buffer: formatter.into_buffer(), diagnostics: Reports::new() },
        Err(error) => {
            FormatQueryResult { buffer: formatter.into_buffer(), diagnostics: Reports::from(error) }
        }
    }
}
