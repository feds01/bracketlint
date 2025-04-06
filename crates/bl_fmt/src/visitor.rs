use crate::{FmtOptions, adapters::ExternalLanguagesEngine, diagnostics::FmtError};

pub(crate) struct Formatter<'fmt, Engine: ExternalLanguagesEngine> {
    /// The engine that is used to format the document. This is used
    /// to format external languages, and in general to pass context
    /// around about the formatter.
    engine: Engine,

    /// The source that is used to format the document.
    source: SpannedSource<'fmt>,

    /// The buffer that is used to store the formatted document.
    buffer: String,

    /// Any options that are used to format the document.
    options: FmtOptions,

    /// The indent level.
    indent: usize,
}

impl<'fmt, Engine: ExternalLanguagesEngine> Formatter<'fmt, Engine> {
    pub fn new(
        engine: Engine,
        options: FmtOptions,
        source: SpannedSource<'fmt>,
        buffer: String,
    ) -> Self {
        Self { engine, buffer, source, indent: 0, options }
    }

    /// Convert the formatter into the buffer.
    pub fn into_buffer(self) -> String {
        self.buffer
    }

    /// Apply an indent to the buffer.
    #[inline(always)]
    pub fn add_indent(&mut self) {
        self.buffer.push_str(" ".repeat(self.indent * self.options.indent_size).as_str());
    }

    /// Push a line into the buffer.
    #[inline(always)]
    pub fn push_line(&mut self, line: &str) {
        self.add_indent();
        self.buffer.push_str(line);
        self.buffer.push('\n');
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
        self.indent += 1;
        f(self)?;
        self.indent -= 1;

        Ok(())
    }
}

