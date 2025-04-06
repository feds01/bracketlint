use bl_ast::{
    AstVisitorMutSelf, SourceId, SpannedSource, ast_visitor_mut_self_default_impl, walk_mut_self,
};

use crate::{
    FmtOptions,
    adapters::{ExternalLanguagesEngineAdaptor, FormatterContext, HasHTMLParsing},
    diagnostics::FmtError,
};

pub(crate) struct Formatter<'fmt, EngineAdaptor: ExternalLanguagesEngineAdaptor> {
    /// The engine that is used to format the document. This is used
    /// to format external languages, and in general to pass context
    /// around about the formatter.
    adaptor: EngineAdaptor,

    /// The source that is used to format the document.
    source: SpannedSource<'fmt>,

    /// The buffer that is used to store the formatted document.
    buffer: String,

    /// Any options that are used to format the document.
    options: FmtOptions,

    /// The context of the formatter.
    ctx: FormatterContext,
}

pub enum TagKind {
    Block,
    Inline,
    Comment,
}

impl TagKind {
    pub fn left(&self) -> &'static str {
        match self {
            TagKind::Block => "{%",
            TagKind::Inline => "{{",
            TagKind::Comment => "{#",
        }
    }

    pub fn right(&self) -> &'static str {
        match self {
            TagKind::Block => "%}",
            TagKind::Inline => "}}",
            TagKind::Comment => "#}",
        }
    }
}

impl<'fmt, Adaptor: ExternalLanguagesEngineAdaptor> Formatter<'fmt, Adaptor> {
    pub fn new(
        adaptor: Adaptor,
        options: FmtOptions,
        id: SourceId,
        source: SpannedSource<'fmt>,
        buffer: String,
    ) -> Self {
        Self { adaptor, buffer, source, options, ctx: FormatterContext { indent: 0, source: id } }
    }

    /// Convert the formatter into the buffer.
    pub fn into_buffer(self) -> String {
        self.buffer
    }

    /// Apply an indent to the buffer.
    #[inline(always)]
    pub fn add_indent(&mut self) {
        let size = self.ctx.indent * self.options.indent_size;

        self.buffer.push_str(" ".repeat(size as usize).as_str());
    }

    /// Push a line into the buffer.
    #[inline(always)]
    pub fn push_line(&mut self, line: &str) {
        self.add_indent();
        self.buffer.push_str(line);
        self.buffer.push('\n');
    }

    /// Push a hunk on the current line.
    #[inline(always)]
    pub fn push_hunk(&mut self, hunk: &str) {
        self.buffer.push_str(hunk);
    }

    /// Run a function with an increased indent level.
    ///
    /// Assume that the formatter function is called within a block, meaning
    /// that we format the inner parts of `F` with an increased indent
    /// level.
    pub fn with_block<F>(&mut self, f: F) -> Result<(), FmtError>
    where
        F: FnOnce(&mut Self) -> Result<(), FmtError>,
    {
        // We increment the indent level before calling the function, and decrement it
        self.ctx.indent += 1;
        f(self)?;
        self.ctx.indent -= 1;

        Ok(())
    }

    fn within_tag<F: FnOnce(&mut Self) -> Result<(), FmtError>>(
        &mut self,
        kind: TagKind,
        f: F,
    ) -> Result<(), FmtError> {
        self.add_indent();
        self.push_hunk(kind.left());

        // Run F without an indent level.
        f(self)?;

        // Add the closing tag.
        self.push_hunk(kind.right());
        Ok(())
    }
}

impl<E: ExternalLanguagesEngineAdaptor> AstVisitorMutSelf for Formatter<'_, E> {
    type Error = FmtError;
    type DocumentRet = ();

    fn visit_document(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Document>,
    ) -> Result<Self::DocumentRet, Self::Error> {
        let _ = walk_mut_self::walk_document(self, node)?;
        Ok(())
    }
}
