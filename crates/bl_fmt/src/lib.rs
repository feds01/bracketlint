//! The bracketlint formatter.

#![feature(impl_trait_in_assoc_type, try_trait_v2)]

use adapters::ExternalLanguagesEngineAdaptor;
use bl_ast::{AstVisitorMutSelf, SourceId};
use bl_reporting::Reports;
use bl_workspace::Member;

mod adapters;
mod backends;
mod diagnostics;
mod visitor;

fn configured_formatter() -> impl ExternalLanguagesEngineAdaptor {
    backends::biome::BiomeFormatter::new()
}

/// Various options for the formatter.
#[derive(Debug, Clone, Copy)]
pub struct FmtOptions {
    indent_size: u16,
}

impl Default for FmtOptions {
    fn default() -> Self {
        Self { indent_size: 4 }
    }
}

/// A query for formatting the document of a [`bl_workspace::Member`].
pub struct FmtQuery<'q> {
    pub options: FmtOptions,
    pub source: SourceId,
    pub member: &'q Member,
}

/// The result of formatting a [`bl_ast::Document`]. This returns the formatted
/// content and the diagnostics that were generated during the formatting.
pub struct FmtQueryResult {
    pub buffer: String,
    pub diagnostics: Reports,
}

/// A query that returns the result of formatting a [`bl_ast::Document`].
pub fn fmt_module(query: FmtQuery) -> FmtQueryResult {
    let FmtQuery { member, source, options } = query;

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
        Ok(_) => FmtQueryResult { buffer: formatter.into_buffer(), diagnostics: Reports::new() },
        Err(error) => {
            FmtQueryResult { buffer: formatter.into_buffer(), diagnostics: Reports::from(error) }
        }
    }
}
