//! Definitions of the template dialects that bracketlint understands.

use std::path::Path;

use serde::Deserialize;
use strum::{Display, EnumString, IntoStaticStr, VariantArray};

use crate::{
    BinOp, UnaryOp,
    keywords::{Keyword, KeywordTable},
};

/// The template language that a file is written in. It is either given with
/// `--dialect`, or picked from the file extension, see [Dialect::from_path].
///
/// The lowercase name of a dialect, e.g. `liquid`, is what `--dialect` accepts.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Display,
    EnumString,
    IntoStaticStr,
    VariantArray,
    Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    /// [Django] templates, used when nothing else identifies the dialect.
    ///
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/language/
    #[default]
    Django,
    /// [Jinja] templates.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/
    Jinja,
    /// [Liquid] templates.
    ///
    /// [Liquid]: https://shopify.github.io/liquid/
    Liquid,
    /// [Nunjucks] templates, which are close to Jinja's.
    ///
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html
    Nunjucks,
    /// [Twig] templates.
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html
    Twig,
}

impl Dialect {
    /// All of the dialects.
    pub const ALL: &'static [Dialect] = <Dialect as VariantArray>::VARIANTS;

    /// The name of the dialect, as accepted by `--dialect`.
    pub fn name(self) -> &'static str {
        self.into()
    }

    /// The name of the dialect as it is written in prose, e.g. `Liquid`.
    pub const fn title(self) -> &'static str {
        match self {
            Dialect::Django => "Django",
            Dialect::Jinja => "Jinja",
            Dialect::Liquid => "Liquid",
            Dialect::Nunjucks => "Nunjucks",
            Dialect::Twig => "Twig",
        }
    }

    /// The file extension that identifies the dialect, if it has its own.
    /// Django templates use `.html`, which every dialect uses. [Nunjucks]
    /// templates use `.njk`.
    ///
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#file-extensions
    pub const fn extension(self) -> Option<&'static str> {
        match self {
            Dialect::Django => None,
            Dialect::Jinja => Some("jinja"),
            Dialect::Liquid => Some("liquid"),
            Dialect::Nunjucks => Some("njk"),
            Dialect::Twig => Some("twig"),
        }
    }

    /// The dialect that the extension of `path` identifies, if any.
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        Self::ALL.iter().copied().find(|dialect| dialect.extension() == Some(extension))
    }

    /// Whether `{# ... #}` is a comment, as in [Django], [Jinja], [Nunjucks]
    /// and [Twig]. Liquid has no such comment, so it is text there.
    ///
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/language/#comments
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#comments
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#comments
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#comments
    pub fn has_hash_comments(self) -> bool {
        matches!(self, Dialect::Django | Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether `{% raw %}` blocks exist, as in [Jinja], [Liquid] and
    /// [Nunjucks], whose contents are output as they are written. [Django] and
    /// [Twig] have `{% verbatim %}` instead. Nunjucks takes `{% verbatim %}`
    /// too, which is read as a tag like any other here.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#escaping
    /// [Liquid]: https://shopify.github.io/liquid/tags/template/#raw
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#raw
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#verbatim
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/verbatim.html
    pub fn has_raw_blocks(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Liquid | Dialect::Nunjucks)
    }

    /// The tags that open and close a block whose contents are output as they
    /// are written, which are `raw` and `endraw` in [Jinja], [Liquid] and
    /// [Nunjucks], and `verbatim` and `endverbatim` in [Django] and [Twig], see
    /// [Dialect::has_raw_blocks].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#escaping
    /// [Liquid]: https://shopify.github.io/liquid/tags/template/#raw
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#raw
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#verbatim
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/verbatim.html
    pub fn raw_tags(self) -> (&'static str, &'static str) {
        if self.has_raw_blocks() { ("raw", "endraw") } else { ("verbatim", "endverbatim") }
    }

    /// Whether `{% doc %}` blocks exist, as in [Liquid], whose contents are
    /// documentation that is neither rendered nor run. Liquid reads them as
    /// text up to `{% enddoc %}`, so they needn't be valid Liquid.
    ///
    /// [Liquid]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/doc.rb#L38-L55
    pub fn has_doc_blocks(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a `{% %}` tag that starts with `#` is a comment, as in
    /// [Liquid], e.g. `{% # note %}`.
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/template/#inline-comments
    pub fn has_inline_comments(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether the `{% liquid %}` tag exists, as in [Liquid], which holds a
    /// tag on each of its lines, without their delimiters.
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/template/#liquid
    pub fn has_liquid_tag(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a name can end with a `?`, as Shopify's properties do, e.g.
    /// `product.gift_card?`. [Liquid's lexer] reads a name as
    /// `[a-zA-Z_][\w-]*\??`.
    ///
    /// [Liquid's lexer]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/lexer.rb#L23
    pub fn names_can_end_with_question_mark(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// The markers that control the whitespace around a tag when they are
    /// written next to its delimiters, e.g. the `-` in `{%- if x -%}`, as in
    /// [Jinja], [Liquid], [Nunjucks] and [Twig]. Django has none.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#whitespace-control
    /// [Liquid]: https://shopify.github.io/liquid/basics/whitespace/
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#whitespace-control
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#whitespace-control
    pub fn trim_markers(self) -> &'static [char] {
        match self {
            Dialect::Django => &[],
            Dialect::Jinja => &['-', '+'],
            Dialect::Liquid | Dialect::Nunjucks => &['-'],
            Dialect::Twig => &['-', '~'],
        }
    }

    /// Whether a filter takes a list of arguments, which can be positional or
    /// keyword, e.g. `f: a, b, key: c`, as in [Liquid]. [Django] takes a single
    /// argument, e.g. `f:a`, and Jinja, Nunjucks and Twig take theirs in
    /// parentheses, see
    /// [Dialect::filter_args_in_parentheses].
    ///
    /// [Liquid]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/variable.rb#L17
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/language/#filters
    pub fn filter_supports_arg_list(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether `{{ }}` takes binary operators, as a tag's condition does, e.g.
    /// `{{ a == b }}`. Jinja's [`Parser.subparse`], Nunjucks'
    /// [`Parser.parseNodes`] and Twig's [`Parser::subparse`] parse it as any
    /// expression. Django's [`FilterExpression`] and Liquid's
    /// [`Variable#strict_parse`] read a single value with its filters.
    ///
    /// [`Parser.subparse`]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L1019-L1022
    /// [`Parser.parseNodes`]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L1317-L1318
    /// [`Parser::subparse`]: https://github.com/twigphp/Twig/blob/v3.30.0/src/Parser.php#L187-L190
    /// [`FilterExpression`]: https://github.com/django/django/blob/6.1.2/django/template/base.py#L739-L752
    /// [`Variable#strict_parse`]: https://github.com/Shopify/liquid/blob/v5.13.0/lib/liquid/variable.rb#L66-L79
    pub fn variable_supports_operators(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether a tag's arguments are separated by commas, as in Liquid's
    /// [`cycle`] and [`render`], e.g. `{% cycle "a", "b" %}` or
    /// `{% include "card", product: product %}`.
    ///
    /// [`cycle`]: https://shopify.github.io/liquid/tags/iteration/#cycle
    /// [`render`]: https://shopify.github.io/liquid/tags/template/#render-parameters
    pub fn requires_comma_separated_args(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a tag's named arguments are written `name: value`, as in
    /// [Liquid], rather than `name=value`, as in [Django].
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/template/#render-parameters
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#include
    pub fn named_args_use_colon(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a range of integers can be written `(1..n)`, as in [Liquid].
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/iteration/#range
    pub fn has_ranges(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a value can be called with arguments in parentheses, e.g.
    /// `range(3)` or `post.render(user, full=true)`, as in [Jinja], [Nunjucks]
    /// and [Twig].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#other-operators
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#function-calls
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#functions
    pub fn has_calls(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether a named argument of a call can be written `name: value`, as in
    /// [Twig], e.g. `date(format: 'r')`, as well as `name=value`.
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#named-arguments
    pub fn call_args_named_with_colon(self) -> bool {
        matches!(self, Dialect::Twig)
    }

    /// Whether an argument of a call can be unpacked into several, as in
    /// [Jinja]: the items of `*args` into positional arguments, and the pairs of
    /// `**kwargs` into named ones, e.g. `url_for('.export', **request.args)`.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L899-L906
    pub fn call_args_can_be_unpacked(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether a filter takes its arguments in parentheses, as a call does,
    /// e.g. `join(', ')`, as in [Jinja], [Nunjucks] and [Twig]. Django and
    /// Liquid write them after a `:` instead, see
    /// [Dialect::filter_supports_arg_list].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#filters
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#filters
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#filters
    pub fn filter_args_in_parentheses(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether parentheses group an expression, e.g. `(a or b) and c`, as in
    /// [Jinja], [Nunjucks] and [Twig]. Django has no grouping, and Liquid's
    /// parentheses hold a range, see [Dialect::has_ranges].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#logic
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#logic
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#operators
    pub fn has_grouping(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether items separated by commas in parentheses are a tuple, e.g.
    /// `('a', 'b')`, as in [Jinja], which a `for` loop can also unpack into,
    /// e.g. `{% for (a, b), c in items %}`.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
    pub fn has_tuples(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether items separated by commas in `{{ }}` are a tuple without its
    /// parentheses, e.g. `{{ a, b }}`, as in [Jinja].
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L1019-L1022
    pub fn variables_take_bare_tuples(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether strings written one after another are one string, as Python's
    /// are, e.g. `"a" "b"` is `"ab"`, as in [Jinja].
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L662-L669
    pub fn adjacent_strings_concatenate(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether items separated by commas in brackets are a list, e.g.
    /// `['a', 'b']`, as in [Jinja], an array in [Nunjucks], and a sequence in
    /// [Twig].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#expressions
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    pub fn has_lists(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether pairs of keys and values in braces are a mapping, e.g.
    /// `{'a': 1}`, as in [Jinja] and [Nunjucks], where it is a dict, and
    /// [Twig].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#expressions
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    pub fn has_mappings(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether a key of a mapping can be a name, which is the name's string
    /// rather than a variable, e.g. `{for: id}`, as in [Nunjucks] and [Twig].
    /// A key is an expression in Jinja, so `{name: 1}` takes the variable's
    /// value.
    ///
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/compiler.js#L318-L319
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    pub fn mapping_keys_can_be_names(self) -> bool {
        matches!(self, Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether the value of a mapping's pair can be left out when it is the
    /// variable that its key names, e.g. `{post}` for `{post: post}`, as in
    /// [Twig], see [Dialect::mapping_keys_can_be_names].
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    pub fn mapping_values_can_be_left_out(self) -> bool {
        matches!(self, Dialect::Twig)
    }

    /// Whether `===` and `!==` are comparisons, as in [Nunjucks], which compare
    /// as JavaScript's do, without converting either side.
    ///
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#comparisons
    pub fn has_strict_comparisons(self) -> bool {
        matches!(self, Dialect::Nunjucks)
    }

    /// Whether a string is translated when it is written `_("...")`, as in
    /// [Django], e.g. `{{ value|default:_("None") }}`. It is a string, rather
    /// than a call.
    ///
    /// [Django]: https://docs.djangoproject.com/en/stable/topics/i18n/translation/#string-literals-passed-to-tags-and-filters
    pub fn has_translated_strings(self) -> bool {
        matches!(self, Dialect::Django)
    }

    /// Whether `then if condition else otherwise` is an expression, as in
    /// [Jinja] and [Nunjucks], whose `else otherwise` can be left out.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#if-expression
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#if-expression
    pub fn has_inline_if(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks)
    }

    /// Whether `?` is an operator, as in [Twig]: the conditional
    /// `condition ? then : otherwise`, whose `: otherwise` can be left out,
    /// and the binary `a ?: b` and `a ?? b`.
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#other-operators
    pub fn has_conditional_operators(self) -> bool {
        matches!(self, Dialect::Twig)
    }

    /// Whether a subscript is an expression, which can have operators, e.g.
    /// `items[i + 1]`, as in [Jinja], [Nunjucks] and [Twig], rather than a
    /// value, as in Django and Liquid. Jinja and Twig also take a slice there,
    /// see [Dialect::has_slices].
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L840-L870
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L711-L721
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Infix/SquareBracketExpressionParser.php#L38-L45
    pub fn subscripts_take_operators(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether a subscript can be a slice, e.g. `items[1:]`, as in [Jinja],
    /// whose slices can also have a step, e.g. `items[::2]`, and [Twig].
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L840-L870
    /// [Twig]: https://twig.symfony.com/doc/3.x/filters/slice.html
    pub fn has_slices(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Twig)
    }

    /// Whether the template that `extends`, `include` and `import` name is an
    /// expression, which can have operators, e.g.
    /// `{% include "icons/" ~ icon ~ ".svg" %}`, as in [Jinja], [Nunjucks] and
    /// Twig's [`include`], [`extends`] and [`import`], which can also name
    /// `_self`. Django and Liquid read a value with its filters.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L328-L366
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L285-L437
    /// [`include`]: https://twig.symfony.com/doc/3.x/tags/include.html
    /// [`extends`]: https://twig.symfony.com/doc/3.x/tags/extends.html#conditional-inheritance
    /// [`import`]: https://github.com/twigphp/Twig/blob/v3.30.0/src/TokenParser/ImportTokenParser.php#L31
    pub fn template_names_take_operators(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether `import` can end with `with context` or `without context`, which
    /// decides whether the template that it imports sees this one's variables,
    /// as in [Jinja] and [Nunjucks], e.g.
    /// `{% import "forms.html" as forms with context %}`. Jinja's `include`
    /// takes them too, see [Dialect::include_takes_context].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#import-context-behavior
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L263-L302
    pub fn import_takes_context(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks)
    }

    /// Whether `include` can end with `with context` or `without context`, as
    /// in [Jinja], e.g. `{% include "header.html" without context %}`. Twig's
    /// `with` takes the variables to pass instead, so `with context` passes
    /// the variable `context` there.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#import-context-behavior
    pub fn include_takes_context(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether a block can be `scoped`, which gives it the variables of the
    /// scope that it is in, or `required`, which a child template has to
    /// override, as in [Jinja], e.g. `{% block item scoped required %}`.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L296-L300
    pub fn has_block_modifiers(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether a value can be an arrow function, as in [Twig], e.g.
    /// `x => x.name` or `(a, b) => a + b`, which filters such as `map` take.
    ///
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Infix/ArrowExpressionParser.php#L28-L32
    pub fn has_arrow_functions(self) -> bool {
        matches!(self, Dialect::Twig)
    }

    /// Whether a test's argument can be written after it without parentheses,
    /// e.g. `loop.index is divisibleby 3`, as in [Jinja], which takes a single
    /// value there.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L971-L986
    pub fn test_arg_needs_no_parentheses(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether the name of a test can be two words, as Twig's [`same as`] and
    /// [`divisible by`] are, e.g. `x is same as(false)`.
    ///
    /// [`same as`]: https://twig.symfony.com/doc/3.x/tests/sameas.html
    /// [`divisible by`]: https://twig.symfony.com/doc/3.x/tests/divisibleby.html
    pub fn has_two_word_tests(self) -> bool {
        matches!(self, Dialect::Twig)
    }

    /// Whether the iterator of a `for` loop is an expression, which can have
    /// operators, e.g. `{% for i in 1..n %}`, as in [Jinja], [Nunjucks] and
    /// [Twig]. Django and Liquid read a value with its filters.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L240-L243
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L191
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/TokenParser/ForTokenParser.php#L39-L40
    pub fn loop_iterator_takes_operators(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig)
    }

    /// Whether the filter of a `for` loop is an expression, which can have
    /// operators, e.g. `{% for e in errors if e is string %}`, as in [Jinja].
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L245-L246
    pub fn loop_filter_takes_operators(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether the assignments of a `{% with %}` block are separated by
    /// commas, and their values are expressions, which can have operators,
    /// e.g. `{% with classes = classes or [], level = 1 %}`, as in [Jinja].
    /// Django's are separated by spaces, and their values are a value with
    /// its filters.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L273-L284
    pub fn with_assignments_take_operators(self) -> bool {
        matches!(self, Dialect::Jinja)
    }

    /// Whether a block can be written as a single tag whose content is the
    /// value of an expression, e.g. `{% block title page_title|title %}`, as
    /// in [Twig].
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/extends.html#block-shortcuts
    pub fn has_block_shortcuts(self) -> bool {
        matches!(self, Dialect::Twig)
    }

    /// Whether a `{% with %}` block takes an expression whose value is a
    /// mapping of its variables, e.g. `{% with {a: 1} only %}`, as in [Twig],
    /// rather than assignments, as in Django and Jinja.
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/with.html
    pub fn with_takes_an_expression(self) -> bool {
        matches!(self, Dialect::Twig)
    }

    /// Whether [`nil`], `null`, `blank` and [`empty`] are literals, along with
    /// `true` and `false`, e.g. `{% if items == empty %}`. The other dialects
    /// read them as names like any other.
    ///
    /// [`nil`]: https://shopify.github.io/liquid/basics/types/#nil
    /// [`empty`]: https://shopify.github.io/liquid/basics/types/#emptydrop
    pub fn has_nil_and_empty_literals(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a loop takes parameters after its iterator, as in [Liquid],
    /// which can be separated by commas, e.g.
    /// `{% for item in items limit: 2, offset: 1 %}`.
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/iteration/#for-parameters
    pub fn has_loop_params(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether an assignment can be written `value as name`, as in [Django],
    /// e.g. `{% with business.employees.count as total %}`, as well as
    /// `name=value`.
    ///
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#with
    pub fn has_as_assignments(self) -> bool {
        matches!(self, Dialect::Django)
    }

    /// How the dialect spells `elif`, which is `elsif` in [Liquid], `elseif` in
    /// [Twig], and `elif` in [Django], [Jinja] and [Nunjucks]. Nunjucks also
    /// spells it `elseif`, see [Dialect::elif_tags].
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/control-flow/#elsif--else
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/if.html
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#if
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#if
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#if
    pub const fn elif_tag(self) -> &'static str {
        self.elif_tags()[0]
    }

    /// Each way that the dialect spells `elif`, see [Dialect::elif_tag], which
    /// is the first. [Nunjucks] spells it `elseif` too.
    ///
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#if
    pub const fn elif_tags(self) -> &'static [&'static str] {
        match self {
            Dialect::Liquid => &["elsif"],
            Dialect::Twig => &["elseif"],
            Dialect::Django | Dialect::Jinja => &["elif"],
            Dialect::Nunjucks => &["elif", "elseif"],
        }
    }

    /// The tag that starts the body of a loop for when there is nothing to
    /// loop over, which is `empty` in [Django], and `else` in [Jinja],
    /// [Liquid], [Nunjucks] and [Twig].
    ///
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#for
    /// [Liquid]: https://shopify.github.io/liquid/tags/iteration/#else
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#for
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/for.html#the-else-clause
    pub fn empty_loop_tag(self) -> &'static str {
        if self.is_django() { "empty" } else { "else" }
    }

    /// The keyword that the identifier `name` is in the dialect, if any.
    /// Liquid has keywords of its own, and spells `elif` as `elsif`.
    pub fn keyword(self, name: &str) -> Option<Keyword> {
        // Each dialect's keywords are in a table of its own, built at compile time.
        static DJANGO: KeywordTable = KeywordTable::of(Dialect::Django);
        static JINJA: KeywordTable = KeywordTable::of(Dialect::Jinja);
        static LIQUID: KeywordTable = KeywordTable::of(Dialect::Liquid);
        static NUNJUCKS: KeywordTable = KeywordTable::of(Dialect::Nunjucks);
        static TWIG: KeywordTable = KeywordTable::of(Dialect::Twig);

        let table = match self {
            Dialect::Django => &DJANGO,
            Dialect::Jinja => &JINJA,
            Dialect::Liquid => &LIQUID,
            Dialect::Nunjucks => &NUNJUCKS,
            Dialect::Twig => &TWIG,
        };
        table.get(name)
    }

    /// How tightly the binary operator `op` binds, as the binding powers of
    /// its left and right side, or `None` if the dialect has no such operator.
    /// An operator with a higher binding power groups first, so `a or b and c`
    /// is `a or (b and c)`.
    ///
    /// Each dialect follows its own engine, see the comment on each arm. Only
    /// Jinja, Nunjucks and Twig have arithmetic and `~`, and only Liquid has
    /// `contains`.
    pub fn infix_binding_power(self, op: BinOp) -> Option<(u8, u8)> {
        let power = match self {
            // Django's own precedences, from [`smartif.py`]. `in` and `not in`
            // bind looser than the other comparisons, so `a in b == c` is
            // `a in (b == c)`.
            //
            // [`smartif.py`]: https://github.com/django/django/blob/5.2.18/django/template/smartif.py#L97-L111
            Dialect::Django => match op {
                BinOp::Or => left_assoc(6),
                BinOp::And => left_assoc(7),
                BinOp::In | BinOp::NotIn => left_assoc(9),
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::Is
                | BinOp::IsNot => left_assoc(10),
                BinOp::Contains
                | BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::FloorDiv
                | BinOp::Mod
                | BinOp::Pow
                | BinOp::Concat
                | BinOp::Coalesce
                | BinOp::Elvis
                | BinOp::Range
                | BinOp::Matches
                | BinOp::StartsWith
                | BinOp::EndsWith
                | BinOp::StrictEq
                | BinOp::StrictNotEq => return None,
            },

            // [Jinja's parser] has a function for each level, from `parse_or`
            // to `parse_pow`, rather than numbers, so these go up in tens.
            // Jinja chains comparisons like Python, so `a == b > c` means
            // `a == b and b > c`, which is `(a == b) > c` here. Unlike in
            // Python, `**` is left-associative, so `a ** b ** c` is
            // `(a ** b) ** c`. A test binds tighter than any operator.
            //
            // [Jinja's parser]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L536-L624
            Dialect::Jinja => match op {
                BinOp::Or => left_assoc(10),
                BinOp::And => left_assoc(20),
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::In
                | BinOp::NotIn => left_assoc(40),
                BinOp::Add | BinOp::Sub => left_assoc(50),
                BinOp::Concat => left_assoc(60),
                BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Mod => left_assoc(70),
                BinOp::Pow => left_assoc(80),
                BinOp::Is | BinOp::IsNot => left_assoc(100),
                BinOp::Contains
                | BinOp::Coalesce
                | BinOp::Elvis
                | BinOp::Range
                | BinOp::Matches
                | BinOp::StartsWith
                | BinOp::EndsWith
                | BinOp::StrictEq
                | BinOp::StrictNotEq => return None,
            },

            // Liquid has no precedence between `and` and `or`, and evaluates
            // them from right to left, so `a and b or c` is `a and (b or c)`,
            // see its [order of operations]. Liquid does arithmetic with
            // filters such as `plus` instead.
            //
            // [order of operations]: https://shopify.github.io/liquid/basics/operators/#order-of-operations
            Dialect::Liquid => match op {
                BinOp::And | BinOp::Or => right_assoc(2),
                BinOp::In | BinOp::NotIn => left_assoc(8),
                BinOp::Eq | BinOp::NotEq | BinOp::Is | BinOp::IsNot => right_assoc(8),
                BinOp::Gt | BinOp::GtEq | BinOp::Lt | BinOp::LtEq | BinOp::Contains => {
                    left_assoc(9)
                }
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::FloorDiv
                | BinOp::Mod
                | BinOp::Pow
                | BinOp::Concat
                | BinOp::Coalesce
                | BinOp::Elvis
                | BinOp::Range
                | BinOp::Matches
                | BinOp::StartsWith
                | BinOp::EndsWith
                | BinOp::StrictEq
                | BinOp::StrictNotEq => return None,
            },

            // [Nunjucks' parser] has a function for each level too, from
            // `parseOr` to `parsePow`, but orders them differently: `in` and
            // then `is` bind looser than the comparisons, and each arithmetic
            // operator has a level of its own, so `a - b + c` is
            // `(a - b) + c`, but `a + b - c` is `a + (b - c)`. Every level is
            // left-associative, `**` too.
            //
            // [Nunjucks' parser]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L776-L992
            Dialect::Nunjucks => match op {
                BinOp::Or => left_assoc(10),
                BinOp::And => left_assoc(20),
                BinOp::In | BinOp::NotIn => left_assoc(40),
                BinOp::Is | BinOp::IsNot => left_assoc(50),
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::StrictEq
                | BinOp::StrictNotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq => left_assoc(60),
                BinOp::Concat => left_assoc(70),
                BinOp::Add => left_assoc(80),
                BinOp::Sub => left_assoc(90),
                BinOp::Mul => left_assoc(100),
                BinOp::Div => left_assoc(110),
                BinOp::FloorDiv => left_assoc(120),
                BinOp::Mod => left_assoc(130),
                BinOp::Pow => left_assoc(140),
                BinOp::Contains
                | BinOp::Coalesce
                | BinOp::Elvis
                | BinOp::Range
                | BinOp::Matches
                | BinOp::StartsWith
                | BinOp::EndsWith => return None,
            },

            // Twig's own precedences, from [`CoreExtension`]. A test binds
            // tighter than the comparisons, `not` and the arithmetic below
            // `**`. Twig 3.15 deprecates `~` at 40 in favour of 27, below `+`
            // and `-`, but 40 is what it still parses with. It does the same
            // with `??` at 300, which is more than a `u8` holds, so it is 250
            // here, which still binds tighter than `**` and looser than `-`.
            //
            // [`CoreExtension`]: https://github.com/twigphp/Twig/blob/v3.30.0/src/Extension/CoreExtension.php#L343-L391
            Dialect::Twig => match op {
                BinOp::Elvis => right_assoc(5),
                BinOp::Or => left_assoc(10),
                BinOp::And => left_assoc(15),
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::In
                | BinOp::NotIn
                | BinOp::Matches
                | BinOp::StartsWith
                | BinOp::EndsWith => left_assoc(20),
                BinOp::Range => left_assoc(25),
                BinOp::Add | BinOp::Sub => left_assoc(30),
                BinOp::Concat => left_assoc(40),
                BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Mod => left_assoc(60),
                BinOp::Is | BinOp::IsNot => left_assoc(100),
                BinOp::Pow => right_assoc(200),
                BinOp::Coalesce => right_assoc(250),
                BinOp::Contains | BinOp::StrictEq | BinOp::StrictNotEq => return None,
            },
        };

        Some(power)
    }

    /// How tightly a conditional expression binds, e.g. [Jinja]'s
    /// `a if b else c` and [Twig]'s `b ? a : c`: looser than any binary
    /// operator, so `a or b if c` is `(a or b) if c`, see
    /// [Dialect::infix_binding_power].
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L521-L534
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Infix/ConditionalTernaryExpressionParser.php#L53-L56
    pub const CONDITIONAL_BINDING_POWER: u8 = 0;

    /// How tightly the prefix operator `op` binds: its operand takes every
    /// binary operator whose left binding power is at least this, see
    /// [Dialect::infix_binding_power]. `-` binds tighter than any binary
    /// operator, including `**`, so `-a ** b` is `(-a) ** b`.
    ///
    /// `not` binds tighter than `and` and `or` in every dialect. In Twig it
    /// also binds tighter than the comparisons, `+`, `-` and `~`, so
    /// `not a == b` is `(not a) == b` there and `not (a == b)` in the other
    /// dialects. The powers follow the same sources as
    /// [Dialect::infix_binding_power].
    pub fn prefix_binding_power(self, op: UnaryOp) -> u8 {
        match op {
            UnaryOp::Neg => u8::MAX,
            UnaryOp::Not => match self {
                Dialect::Django => 8,
                Dialect::Jinja | Dialect::Nunjucks => 30,
                Dialect::Liquid => 6,
                Dialect::Twig => 50,
            },
        }
    }

    /// Whether the dialect has `syntax`, by the rule that decides it.
    pub fn has(self, syntax: Syntax) -> bool {
        match syntax {
            Syntax::ArgCommas => self.requires_comma_separated_args(),
            Syntax::ColonArgs => self.named_args_use_colon(),
            Syntax::FilterArgLists => self.filter_supports_arg_list(),
            Syntax::HashComments => self.has_hash_comments(),
            Syntax::TrimMarker(marker) => self.trim_markers().contains(&char::from(marker)),
            Syntax::ElifTag(_) => {
                syntax.tag_name().is_some_and(|tag| self.elif_tags().contains(&tag))
            }
            Syntax::EmptyLoopTag(_) | Syntax::RawTag(_) | Syntax::EndRawTag(_) => {
                syntax.spelling(self) == syntax.tag_name()
            }
        }
    }

    /// `syntax` in a template of this dialect, if the dialect doesn't have it
    /// and another one does, e.g. Liquid's commas between a tag's arguments in
    /// Jinja. The template is then likely written in one of the dialects that
    /// have it.
    pub fn foreign(self, syntax: Syntax) -> Option<ForeignSyntax> {
        let foreign = !self.has(syntax) && syntax.dialects().next().is_some();
        foreign.then_some(ForeignSyntax { syntax, dialect: self })
    }

    /// Check if its currently Django dialect.
    #[inline]
    pub fn is_django(self) -> bool {
        self == Dialect::Django
    }

    /// Check if its currently Liquid dialect.
    #[inline]
    pub const fn is_liquid(self) -> bool {
        matches!(self, Dialect::Liquid)
    }
}

/// Syntax that only some dialects have, which a rule on [Dialect] decides. A
/// template that uses it in a dialect without it is likely written in one of
/// those, so a diagnostic at it names them, see [Dialect::foreign].
///
/// ##Note: A parse error holds the syntax that it is at, so the syntax is kept
/// to a few bytes: a tag is held as the dialect whose spelling it is, rather
/// than as a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    /// Commas between a tag's arguments, see
    /// [Dialect::requires_comma_separated_args].
    ArgCommas,

    /// Named arguments that are written `name: value`, see
    /// [Dialect::named_args_use_colon].
    ColonArgs,

    /// A filter's list of arguments, which can be positional or keyword, e.g.
    /// `f: a, key: b`, see [Dialect::filter_supports_arg_list].
    FilterArgLists,

    /// `{# #}` comments, see [Dialect::has_hash_comments].
    HashComments,

    /// A whitespace control marker, which is an ASCII character, e.g. the `~`
    /// in `{%~ if x %}`, see [Dialect::trim_markers].
    TrimMarker(u8),

    /// The tag that starts an `elif` clause, as the dialect spells it, e.g.
    /// `elsif` in Liquid, see [Dialect::elif_tag].
    ElifTag(Dialect),

    /// The tag that starts the body of a loop for when there is nothing to
    /// loop over, as the dialect spells it, e.g. `empty` in Django, see
    /// [Dialect::empty_loop_tag].
    EmptyLoopTag(Dialect),

    /// The tag that opens a block whose contents are output as they are
    /// written, as the dialect spells it, e.g. `verbatim` in Django, see
    /// [Dialect::raw_tags].
    RawTag(Dialect),

    /// The tag that closes a block whose contents are output as they are
    /// written, as the dialect spells it, e.g. `endverbatim` in Django.
    EndRawTag(Dialect),
}

impl Syntax {
    /// The tag that `name` is in some dialect, e.g. `elsif`, which starts an
    /// `elif` clause in Liquid.
    pub fn tag(name: &str) -> Option<Self> {
        Dialect::ALL
            .iter()
            .flat_map(|&dialect| {
                [
                    Syntax::ElifTag(dialect),
                    Syntax::EmptyLoopTag(dialect),
                    Syntax::RawTag(dialect),
                    Syntax::EndRawTag(dialect),
                ]
            })
            .find(|syntax| syntax.tag_name() == Some(name))
    }

    /// The name of the tag, as the dialect that it is held with spells it, if
    /// the syntax is a tag.
    pub fn tag_name(self) -> Option<&'static str> {
        match self {
            Syntax::ElifTag(dialect)
            | Syntax::EmptyLoopTag(dialect)
            | Syntax::RawTag(dialect)
            | Syntax::EndRawTag(dialect) => self.spelling(dialect),
            Syntax::ArgCommas
            | Syntax::ColonArgs
            | Syntax::FilterArgLists
            | Syntax::HashComments
            | Syntax::TrimMarker(_) => None,
        }
    }

    /// How `dialect` spells the tag, if the syntax is a tag.
    fn spelling(self, dialect: Dialect) -> Option<&'static str> {
        match self {
            Syntax::ElifTag(_) => Some(dialect.elif_tag()),
            Syntax::EmptyLoopTag(_) => Some(dialect.empty_loop_tag()),
            Syntax::RawTag(_) => Some(dialect.raw_tags().0),
            Syntax::EndRawTag(_) => Some(dialect.raw_tags().1),
            Syntax::ArgCommas
            | Syntax::ColonArgs
            | Syntax::FilterArgLists
            | Syntax::HashComments
            | Syntax::TrimMarker(_) => None,
        }
    }

    /// The dialects that have the syntax.
    pub fn dialects(self) -> impl Iterator<Item = Dialect> {
        Dialect::ALL.iter().copied().filter(move |dialect| dialect.has(self))
    }
}

/// [Syntax] in a template of a dialect that doesn't have it, see
/// [Dialect::foreign].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForeignSyntax {
    /// The syntax that the template uses.
    pub syntax: Syntax,

    /// The dialect that the template is parsed in.
    pub dialect: Dialect,
}

impl ForeignSyntax {
    /// A help message that names the dialects that have the syntax, and how to
    /// parse the template in one of them, e.g. "commas between a tag's
    /// arguments are Liquid syntax, pass `--dialect liquid` if this is a Liquid
    /// template".
    pub fn help(self) -> String {
        let dialects: Vec<_> = self.syntax.dialects().collect();
        let names = titles(&dialects);
        let write = if dialects.len() == 1 { "writes" } else { "write" };

        let syntax = match self.syntax {
            Syntax::ArgCommas => format!("commas between a tag's arguments are {names} syntax"),
            Syntax::ColonArgs => format!("`name: value` arguments are {names} syntax"),
            Syntax::FilterArgLists => {
                format!("a filter's list of arguments, e.g. `f: a, key: b`, is {names} syntax")
            }
            Syntax::HashComments => format!("`{{# #}}` comments are {names} syntax"),
            Syntax::TrimMarker(marker) => {
                let marker = char::from(marker);
                format!("the whitespace control marker `{marker}` is {names} syntax")
            }
            Syntax::ElifTag(_)
            | Syntax::EmptyLoopTag(_)
            | Syntax::RawTag(_)
            | Syntax::EndRawTag(_) => {
                let tag = self.syntax.tag_name().unwrap_or_default();
                let ours = self.syntax.spelling(self.dialect).unwrap_or_default();
                let in_loop =
                    if matches!(self.syntax, Syntax::EmptyLoopTag(_)) { " in a loop" } else { "" };
                format!("`{{% {tag} %}}`{in_loop} is how {names} {write} `{{% {ours} %}}`")
            }
        };

        let switch = match dialects.as_slice() {
            [dialect] => format!(
                "pass `--dialect {}` if this is a {} template",
                dialect.name(),
                dialect.title()
            ),
            _ => "pass `--dialect` with the template's dialect if it is one of them".to_string(),
        };

        format!("{syntax}, {switch}")
    }
}

/// The names of `dialects` in prose, e.g. `Django, Jinja and Twig`.
fn titles(dialects: &[Dialect]) -> String {
    let titles: Vec<_> = dialects.iter().map(|dialect| dialect.title()).collect();

    match titles.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => titles.concat(),
    }
}

/// The binding powers of a left-associative operator that binds with `power`:
/// its right side only takes operators that bind tighter, so `a == b == c` is
/// `(a == b) == c`.
const fn left_assoc(power: u8) -> (u8, u8) {
    (power, power + 1)
}

/// The binding powers of a right-associative operator that binds with `power`:
/// its right side also takes operators that bind as tightly, so `a or b or c`
/// is `a or (b or c)`.
const fn right_assoc(power: u8) -> (u8, u8) {
    (power, power)
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_from_path() {
        assert_eq!(Dialect::from_path(Path::new("product.liquid")), Some(Dialect::Liquid));
        assert_eq!(Dialect::from_path(Path::new("base.twig")), Some(Dialect::Twig));
        assert_eq!(Dialect::from_path(Path::new("templates/page.jinja")), Some(Dialect::Jinja));
        assert_eq!(Dialect::from_path(Path::new("template.njk")), Some(Dialect::Nunjucks));
        assert_eq!(Dialect::from_path(Path::new("index.html")), None);
        assert_eq!(Dialect::from_path(Path::new("Makefile")), None);
    }

    #[test]
    fn test_name_round_trip() {
        for &dialect in Dialect::ALL {
            assert_eq!(dialect.name().parse::<Dialect>(), Ok(dialect));
            assert_eq!(dialect.to_string(), dialect.name());
        }

        assert_eq!(Dialect::Liquid.name(), "liquid");
        assert_eq!("nunjucks".parse::<Dialect>(), Ok(Dialect::Nunjucks));
        assert!("mustache".parse::<Dialect>().is_err());
    }

    #[test]
    fn test_keyword() {
        assert_eq!(Dialect::Django.keyword("for"), Some(Keyword::For));
        assert_eq!(Dialect::Liquid.keyword("for"), Some(Keyword::For));
        assert_eq!(Dialect::Twig.keyword("user"), None);

        assert_eq!(Dialect::Jinja.keyword("elif"), Some(Keyword::Elif));
        assert_eq!(Dialect::Jinja.keyword("elsif"), None);
        assert_eq!(Dialect::Liquid.keyword("elsif"), Some(Keyword::Elif));
        assert_eq!(Dialect::Liquid.keyword("elif"), None);
        assert_eq!(Dialect::Twig.keyword("elseif"), Some(Keyword::Elif));
        assert_eq!(Dialect::Twig.keyword("elif"), None);

        assert_eq!(Dialect::Liquid.keyword("unless"), Some(Keyword::Unless));
        assert_eq!(Dialect::Django.keyword("unless"), None);
    }

    #[test]
    fn test_operators_of_each_dialect() {
        let arithmetic = [
            BinOp::Add,
            BinOp::Sub,
            BinOp::Mul,
            BinOp::Div,
            BinOp::FloorDiv,
            BinOp::Mod,
            BinOp::Pow,
            BinOp::Concat,
        ];

        for &dialect in Dialect::ALL {
            let has_arithmetic =
                matches!(dialect, Dialect::Jinja | Dialect::Nunjucks | Dialect::Twig);
            for op in arithmetic {
                let power = dialect.infix_binding_power(op);
                assert_eq!(power.is_some(), has_arithmetic, "`{op}` in {dialect}");
            }

            let power = dialect.infix_binding_power(BinOp::Contains);
            assert_eq!(power.is_some(), dialect.is_liquid(), "`contains` in {dialect}");

            let twig = [
                BinOp::Coalesce,
                BinOp::Elvis,
                BinOp::Range,
                BinOp::Matches,
                BinOp::StartsWith,
                BinOp::EndsWith,
            ];
            for op in twig {
                let power = dialect.infix_binding_power(op);
                assert_eq!(power.is_some(), dialect == Dialect::Twig, "`{op}` in {dialect}");
            }

            for op in [BinOp::StrictEq, BinOp::StrictNotEq] {
                let power = dialect.infix_binding_power(op);
                assert_eq!(power.is_some(), dialect == Dialect::Nunjucks, "`{op}` in {dialect}");
            }
        }
    }

    #[test]
    fn test_syntax_tag() {
        assert_eq!(Syntax::tag("elsif"), Some(Syntax::ElifTag(Dialect::Liquid)));
        assert_eq!(Syntax::tag("elseif"), Some(Syntax::ElifTag(Dialect::Twig)));
        assert_eq!(Syntax::tag("empty"), Some(Syntax::EmptyLoopTag(Dialect::Django)));
        assert_eq!(Syntax::tag("verbatim"), Some(Syntax::RawTag(Dialect::Django)));
        assert_eq!(Syntax::tag("endraw"), Some(Syntax::EndRawTag(Dialect::Jinja)));
        assert_eq!(Syntax::tag("cycle"), None);

        assert_eq!(Syntax::tag("elif").and_then(Syntax::tag_name), Some("elif"));
        assert_eq!(Syntax::ArgCommas.tag_name(), None);
    }

    #[test]
    fn test_foreign() {
        // Each dialect has its own spelling, and not the others'.
        assert!(Dialect::Jinja.foreign(Syntax::ElifTag(Dialect::Liquid)).is_some());
        assert!(Dialect::Liquid.foreign(Syntax::ElifTag(Dialect::Liquid)).is_none());
        assert!(Dialect::Liquid.foreign(Syntax::ElifTag(Dialect::Jinja)).is_some());
        assert!(Dialect::Django.foreign(Syntax::ElifTag(Dialect::Jinja)).is_none());
        assert!(Dialect::Django.foreign(Syntax::EndRawTag(Dialect::Twig)).is_none());
        assert!(Dialect::Twig.foreign(Syntax::RawTag(Dialect::Jinja)).is_some());

        // Nunjucks spells `elif` as Twig does too.
        assert!(Dialect::Nunjucks.foreign(Syntax::ElifTag(Dialect::Twig)).is_none());
        assert!(Dialect::Nunjucks.foreign(Syntax::ElifTag(Dialect::Jinja)).is_none());
        assert!(Dialect::Nunjucks.foreign(Syntax::ElifTag(Dialect::Liquid)).is_some());

        assert!(Dialect::Django.foreign(Syntax::ArgCommas).is_some());
        assert!(Dialect::Liquid.foreign(Syntax::ArgCommas).is_none());
        assert!(Dialect::Liquid.foreign(Syntax::HashComments).is_some());
        assert!(Dialect::Jinja.foreign(Syntax::TrimMarker(b'~')).is_some());
        assert!(Dialect::Twig.foreign(Syntax::TrimMarker(b'~')).is_none());

        // Syntax that no dialect has isn't another dialect's.
        assert!(Dialect::Jinja.foreign(Syntax::TrimMarker(b'*')).is_none());
    }

    #[test]
    fn test_foreign_help() {
        let help = |dialect: Dialect, syntax| dialect.foreign(syntax).unwrap().help();

        assert_eq!(
            help(Dialect::Jinja, Syntax::ArgCommas),
            "commas between a tag's arguments are Liquid syntax, pass `--dialect liquid` if this \
             is a Liquid template"
        );
        assert_eq!(
            help(Dialect::Jinja, Syntax::ElifTag(Dialect::Liquid)),
            "`{% elsif %}` is how Liquid writes `{% elif %}`, pass `--dialect liquid` if this is \
             a Liquid template"
        );
        assert_eq!(
            help(Dialect::Liquid, Syntax::ElifTag(Dialect::Jinja)),
            "`{% elif %}` is how Django, Jinja and Nunjucks write `{% elsif %}`, pass \
             `--dialect` with the template's dialect if it is one of them"
        );
        assert_eq!(
            help(Dialect::Jinja, Syntax::ElifTag(Dialect::Twig)),
            "`{% elseif %}` is how Nunjucks and Twig write `{% elif %}`, pass `--dialect` \
             with the template's dialect if it is one of them"
        );
        assert_eq!(
            help(Dialect::Jinja, Syntax::EndRawTag(Dialect::Django)),
            "`{% endverbatim %}` is how Django and Twig write `{% endraw %}`, pass `--dialect` \
             with the template's dialect if it is one of them"
        );
    }
}
