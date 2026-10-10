//! Definitions for a diagnostic [Reporter] for the Bracketlint. [Reporter]
//! provides the necessary context for a [Report] to be rendered in a
//! human-readable format. This is done via the [annotate_snippets] crate and
//! its API.

use std::{fmt, ops::Range};

use annotate_snippets::{Annotation, AnnotationKind, Element, Level, Renderer, Snippet};
use bl_ast::HasSource;

use crate::{Report, ReportCodeBlock, ReportElement, Reports};

/// Facilitates the creation of lists of [Report]s in a declarative way.
#[derive(Debug)]
pub struct Reporter<'a, S: HasSource> {
    /// The renderer that the reporter will use to render the reports.
    renderer: Option<&'a Renderer>,

    /// The list of reports that the reporter will render.
    reports: Reports,

    /// The source map that the reporter will use to access source information.
    sources: &'a S,
}

/// The default renderer for the reporter.
const DEFAULT_RENDERER: Renderer = Renderer::styled();

impl<'a, S: HasSource> Reporter<'a, S> {
    pub fn new(sources: &'a S, reports: Vec<Report>) -> Self {
        Self { reports, renderer: None, sources }
    }

    pub fn with_renderer(&mut self, renderer: &'a Renderer) -> &mut Self {
        self.renderer = Some(renderer);
        self
    }
}

impl<S: HasSource> fmt::Display for Reporter<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let renderer = self.renderer.unwrap_or(&DEFAULT_RENDERER);

        for report in self.reports.iter() {
            let level = Level::from(report.kind);
            let mut message = level.primary_title(&report.title);

            // If we have an associated code, we can add it to the message.
            if let Some(ref code) = report.error_code {
                message = message.id(code);
            }

            let mut elements: Vec<Element<'_>> = vec![];

            for element in &report.contents {
                match element {
                    ReportElement::CodeBlock(ReportCodeBlock { notes }) => {
                        for (span, note) in notes {
                            elements.push(
                                snippet(
                                    self.sources.contents(span.id),
                                    self.sources.path(span.id),
                                    span.range.start()..(span.range.end() + 1),
                                    note,
                                )
                                .into(),
                            );
                        }
                    }
                    ReportElement::Note(report_note) => {
                        elements.push(Level::INFO.message(&report_note.message).into());
                    }
                }
            }

            let report = &[message.elements(elements)];

            writeln!(f, "{}", renderer.render(report))?;
        }

        Ok(())
    }
}

/// A snippet of `contents` that labels the `annotated` part of it.
///
/// ##Note: A snippet only shows the lines that its annotations are on, but
/// `annotate_snippets` splits all of its source into lines first. Giving it
/// only those lines, rather than the whole file, saves doing that for each
/// note.
fn snippet<'a>(
    contents: &'a str,
    path: &'a str,
    annotated: Range<usize>,
    label: &'a str,
) -> Snippet<'a, Annotation<'a>> {
    let lines = lines_around(contents, &annotated);
    let line_start = 1 + bytecount::count(&contents.as_bytes()[..lines.start], b'\n');
    let annotated = (annotated.start - lines.start)..(annotated.end - lines.start);

    Snippet::source(&contents[lines])
        .line_start(line_start)
        .path(path)
        .fold(true)
        .annotation(AnnotationKind::Context.span(annotated).label(label))
}

/// The range of the whole lines of `contents` that `annotated` is on, as
/// `annotate_snippets` places it.
fn lines_around(contents: &str, annotated: &Range<usize>) -> Range<usize> {
    let bytes = contents.as_bytes();
    let start = annotated.start.min(bytes.len());

    // An annotation that takes in a newline ends on the next line, so the
    // window runs to the end of the line that the annotation ends on.
    let end = annotated.end.max(start).min(bytes.len());

    // `annotate_snippets` puts the end of a file that ends with a newline on
    // its last line, rather than on an empty line after it.
    let before = match bytes.last() {
        Some(b'\n') if start == bytes.len() => start - 1,
        _ => start,
    };
    let line_start = memchr::memrchr(b'\n', &bytes[..before]).map_or(0, |index| index + 1);
    let line_end =
        memchr::memchr(b'\n', &bytes[end..]).map_or(bytes.len(), |index| end + index + 1);
    line_start..line_end
}

#[cfg(test)]
mod tests {
    use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};

    use super::snippet;

    /// A snippet of only the annotated lines must render as one of the whole
    /// file does, for every span, including those that take in a newline or
    /// reach the end of the file.
    #[test]
    fn snippet_renders_as_one_of_the_whole_file() {
        let sources = [
            "first\nsecond line\n\nfourth\n",
            "first\nsecond line\n\nfourth",
            "one line",
            "",
            "\n\n",
            "a\r\nbé\r\n\r\nü",
        ];
        let render = |snippet: Snippet<'_, _>| {
            Renderer::plain().render(&[Level::ERROR.primary_title("title").element(snippet)])
        };

        for contents in sources {
            let boundaries = (0..=contents.len()).filter(|&index| contents.is_char_boundary(index));

            for start in boundaries {
                // The reporter's spans end one byte after the last one that they
                // take in, so they can reach one byte past the end of the file.
                let ends = (start + 1..=contents.len() + 1)
                    .filter(|&end| end > contents.len() || contents.is_char_boundary(end));

                for end in ends {
                    let whole = Snippet::source(contents)
                        .path("file")
                        .fold(true)
                        .annotation(AnnotationKind::Context.span(start..end).label("here"));

                    assert_eq!(
                        render(snippet(contents, "file", start..end, "here")),
                        render(whole),
                        "{contents:?} at {start}..{end}"
                    );
                }
            }
        }
    }
}
