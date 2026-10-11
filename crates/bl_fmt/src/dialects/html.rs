//! What the formatter knows of HTML: the elements that it keeps as they are
//! written, and those whose end tags can be left out.

/// The elements whose content can't be laid out as other HTML is: whitespace
/// matters in `<pre>` and `<textarea>`, and `<script>` and `<style>` hold
/// code. Like any element name in HTML, they are matched regardless of case.
pub const VERBATIM_ELEMENTS: [&str; 4] = ["pre", "textarea", "script", "style"];

/// Whether `name` is one of the [VERBATIM_ELEMENTS].
pub fn is_verbatim(name: &str) -> bool {
    VERBATIM_ELEMENTS.iter().any(|element| name.eq_ignore_ascii_case(element))
}

/// Whether whitespace matters in the element `name`, i.e. a `<pre>` or a
/// `<textarea>`, so that even the whitespace before its closing tag is kept
/// as it is written.
pub fn keeps_whitespace(name: &str) -> bool {
    ["pre", "textarea"].iter().any(|element| name.eq_ignore_ascii_case(element))
}

/// The open elements that an element `name` closes when it opens, since their
/// end tags can be left out, e.g. an `<li>` closes the `<li>` before it, as
/// [HTML] does.
///
/// [HTML]: https://html.spec.whatwg.org/multipage/syntax.html#syntax-tag-omission
pub fn implied_ends(name: &str) -> &'static [&'static str] {
    match name.to_ascii_lowercase().as_str() {
        "li" => &["li"],
        "dt" | "dd" => &["dt", "dd"],
        "p" => &["p"],
        "rt" | "rp" => &["rt", "rp"],
        "option" => &["option"],
        "optgroup" => &["option", "optgroup"],
        "td" | "th" => &["td", "th"],
        "tr" => &["td", "th", "tr"],
        "tbody" | "tfoot" => &["td", "th", "tr", "thead", "tbody"],
        _ => &[],
    }
}
