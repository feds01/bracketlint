//! Language token keyword definitions.
use std::fmt;

use strum::AsRefStr;

/// Template language keywords. Most of them are shared by every dialect, and
/// [crate::Dialect::keyword] finds the ones that a dialect has.
#[derive(Debug, Copy, Clone, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "lowercase")]
pub enum Keyword {
    /// `for` - Begins a for loop block
    /// ```django
    /// {% for user in users %}
    ///     {{ user.name }}
    /// {% endfor %}
    /// ```
    For,
    /// `endfor` - Ends a for loop block
    EndFor,
    /// `if` - Begins a conditional block
    /// ```django
    /// {% if user.is_active %}
    ///     {{ user.name }}
    /// {% endif %}
    /// ```
    If,
    /// `elif` - Else if condition in an if block
    /// ```django
    /// {% if user.is_admin %}
    ///     Admin user
    /// {% elif user.is_staff %}
    ///     Staff user
    /// {% endif %}
    /// ```
    Elif,
    /// `else` - Else condition in an if block
    /// ```django
    /// {% if user.is_active %}
    ///     Active user
    /// {% else %}
    ///     Inactive user
    /// {% endif %}
    /// ```
    Else,
    /// `endif` - Ends an if block
    EndIf,
    /// `break` - Exits from a for loop
    /// ```django
    /// {% for item in items %}
    ///     {% if item.is_last %}
    ///         {% break %}
    ///     {% endif %}
    /// {% endfor %}
    /// ```
    Break,
    /// `continue` - Skips to next iteration in a for loop
    /// ```django
    /// {% for item in items %}
    ///     {% if item.hidden %}
    ///         {% continue %}
    ///     {% endif %}
    /// {% endfor %}
    /// ```
    Continue,
    /// `empty` - Content to display when a for loop has no items
    /// ```django
    /// {% for item in items %}
    ///     {{ item }}
    /// {% empty %}
    ///     No items found.
    /// {% endfor %}
    /// ```
    Empty,
    /// `and` - Logical AND operator
    /// ```django
    /// {% if user.is_active and user.email %}
    ///     {{ user.email }}
    /// {% endif %}
    /// ```
    And,
    /// `or` - Logical OR operator
    /// ```django
    /// {% if user.is_admin or user.is_staff %}
    ///     Has permissions
    /// {% endif %}
    /// ```
    Or,
    /// `not` - Logical NOT operator
    /// ```django
    /// {% if not user.is_active %}
    ///     Account inactive
    /// {% endif %}
    /// ```
    Not,
    /// `in` - Membership operator
    /// ```django
    /// {% if user in active_users %}
    ///     User is active
    /// {% endif %}
    /// ```
    In,
    /// `is` equality operator
    Is,
    /// `as` - Variable assignment operator
    /// ```django
    /// {% with total=business.employees.count %}
    ///     {{ total }} employee{{ total|pluralize }}
    /// {% endwith %}
    /// ```
    As,
    /// `with` - Creates a block with additional context
    /// ```django
    /// {% with total=business.employees.count %}
    ///     {{ total }} employee{{ total|pluralize }}
    /// {% endwith %}
    /// ```
    With,
    /// `endwith` - Ends a with block
    EndWith,
    /// `block` - Defines a block that can be overridden by child templates
    /// ```django
    /// {% block content %}
    ///     Default content
    /// {% endblock %}
    /// ```
    Block,
    /// `endblock` - Ends a block definition
    EndBlock,
    /// `extends` - Specifies a parent template
    /// ```django
    /// {% extends "base.html" %}
    /// ```
    Extends,
    /// `include` - Includes another template. It is only a keyword at the
    /// start of a tag, so that `include` is still a name elsewhere, e.g.
    /// Jekyll's `{{ include.title }}`
    /// ```django
    /// {% include "navbar.html" %}
    /// ```
    Include,
    /// `load` - Loads a custom template tag library
    /// ```django
    /// {% load static %}
    /// ```
    Load,
    /// `comment` - Begins a comment block (will not be rendered)
    /// ```django
    /// {% comment %}
    ///     This is a comment
    /// {% endcomment %}
    /// ```
    Comment,
    /// `endcomment` - Ends a comment block
    EndComment,
    /// `raw` - Begins a block that should not be parsed
    /// ```django
    /// {% raw %}
    ///     {% this will not be parsed %}
    /// {% endraw %}
    /// ```
    Raw,
    /// `endraw` - Ends a raw block
    EndRaw,
    /// `doc` - Begins a Liquid block of documentation, which is neither
    /// rendered nor run
    /// ```liquid
    /// {% doc %}
    ///   @param {string} name - The name to greet.
    /// {% enddoc %}
    /// ```
    Doc,
    /// `enddoc` - Ends a `doc` block
    EndDoc,
    /// `True` - Boolean true constant
    /// ```django
    /// {% if user.is_active == True %}
    ///     Active user
    /// {% endif %}
    /// ```
    #[strum(serialize = "True")]
    True,
    /// `False` - Boolean false constant
    /// ```django
    /// {% if user.is_active == False %}
    ///     Inactive user
    /// {% endif %}
    /// ```
    #[strum(serialize = "False")]
    False,
    /// `import` - Imports macros or variables from another template
    /// ```django
    /// {% import 'forms.html' as forms %}
    /// ```
    Import,

    /// Reversed keyword for the `for` keyword.
    /// ```django
    /// {% for user in users reversed %}
    ///    {{ user.name }}
    /// {% endfor %}
    /// ```
    Reversed,

    /// `contains` - Checks whether a string has a substring, or an array has
    /// an item, in Liquid.
    /// ```liquid
    /// {% if product.title contains "Pack" %}
    /// ```
    Contains,
    /// `unless` - Begins a Liquid block that renders when its condition is
    /// false, and can have `elsif` and `else` clauses like an `if` block
    /// ```liquid
    /// {% unless user %}
    ///     Guest
    /// {% endunless %}
    /// ```
    Unless,
    /// `endunless` - Ends an `unless` block
    EndUnless,
    /// `case` - Begins a Liquid block that renders the `when` clause that
    /// matches its subject
    /// ```liquid
    /// {% case product.type %}
    ///     {% when "shirt", "hat" %} Apparel
    ///     {% else %} Other
    /// {% endcase %}
    /// ```
    Case,
    /// `when` - A clause of a `case` block
    When,
    /// `endcase` - Ends a `case` block
    EndCase,
    /// `tablerow` - Begins a Liquid loop that renders a table row per item
    /// ```liquid
    /// {% tablerow product in products cols: 2 %}
    ///     {{ product.title }}
    /// {% endtablerow %}
    /// ```
    TableRow,
    /// `endtablerow` - Ends a `tablerow` loop
    EndTableRow,
    /// `capture` - Begins a Liquid block that renders its body into a variable
    /// ```liquid
    /// {% capture greeting %}Hello {{ name }}{% endcapture %}
    /// ```
    Capture,
    /// `endcapture` - Ends a `capture` block
    EndCapture,
    /// `render` - Renders another Liquid template, with variables of its own
    /// ```liquid
    /// {% render "product-card", product: product %}
    /// ```
    Render,
    /// `liquid` - Begins a Liquid tag that holds a tag on each of its lines.
    /// The lexer only makes this keyword at the start of the tag, so that
    /// `liquid` is still a name elsewhere
    /// ```liquid
    /// {% liquid
    ///   assign total = cart.total_price
    ///   echo total
    /// %}
    /// ```
    Liquid,
}
impl Keyword {
    pub fn identifier_like(&self) -> bool {
        matches!(
            self,
            Keyword::Block
                | Keyword::Empty
                | Keyword::Load
                | Keyword::Import
                | Keyword::Comment
                | Keyword::Contains
                | Keyword::Unless
                | Keyword::Case
                | Keyword::When
                | Keyword::TableRow
                | Keyword::Capture
                | Keyword::Render
                | Keyword::Doc
                | Keyword::Include
        )
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

/// A name of up to 15 bytes, packed into an integer, so that comparing it with
/// a keyword is one integer comparison rather than a call to `memcmp`. Every
/// keyword is that short, so a longer name packs to [Name::LONG], which isn't
/// one.
///
/// The name is read as two chunks of the same size, from its start and from its
/// end, which between them hold each of its bytes, and which overlap when it is
/// shorter than both. Reading them takes two loads, rather than a copy of each
/// byte. Its length goes in the top byte, so that no two names pack to the same
/// integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Name(u128);

impl Name {
    /// Any name that is longer than 15 bytes.
    const LONG: Self = Self(u128::MAX);

    pub(crate) const fn new(name: &str) -> Self {
        let bytes = name.as_bytes();
        let chunks = match bytes.len() {
            0 => 0,
            1 => bytes[0] as u128,
            2..=3 => match (bytes.first_chunk(), bytes.last_chunk()) {
                (Some(start), Some(end)) => {
                    u16::from_le_bytes(*start) as u128 | (u16::from_le_bytes(*end) as u128) << 16
                }
                _ => unreachable!(),
            },
            4..=7 => match (bytes.first_chunk(), bytes.last_chunk()) {
                (Some(start), Some(end)) => {
                    u32::from_le_bytes(*start) as u128 | (u32::from_le_bytes(*end) as u128) << 32
                }
                _ => unreachable!(),
            },
            // The first byte of the end chunk is in the start chunk too, so it is
            // shifted out to leave the top byte for the length.
            8..=15 => match (bytes.first_chunk(), bytes.last_chunk()) {
                (Some(start), Some(end)) => {
                    u64::from_le_bytes(*start) as u128
                        | ((u64::from_le_bytes(*end) >> 8) as u128) << 64
                }
                _ => unreachable!(),
            },
            _ => return Self::LONG,
        };

        Self(chunks | (bytes.len() as u128) << 120)
    }
}

/// Define a function that finds the keyword that a [Name] is, from a table of
/// each keyword's text.
macro_rules! keyword_table {
    ($(#[$attr:meta])* fn $function:ident { $($text:literal => $keyword:ident,)* }) => {
        $(#[$attr])*
        #[allow(non_upper_case_globals)]
        pub(crate) fn $function(name: Name) -> Option<Keyword> {
            // Each keyword's text, packed, named after the keyword.
            $(const $keyword: u128 = Name::new($text).0;)*

            match name.0 {
                $($keyword => Some(Keyword::$keyword),)*
                _ => None,
            }
        }
    };
}

keyword_table! {
    /// The keyword that the identifier `name` is in every dialect, if any.
    fn keyword {
        // Control flow
        "for" => For,
        "endfor" => EndFor,
        "if" => If,
        "elif" => Elif,
        "else" => Else,
        "endif" => EndIf,
        "break" => Break,
        "continue" => Continue,
        "empty" => Empty,

        // Logical operators
        "and" => And,
        "or" => Or,
        "not" => Not,
        "in" => In,
        "as" => As,
        "is" => Is,

        // Context management
        "with" => With,
        "endwith" => EndWith,

        // Template structure
        "block" => Block,
        "endblock" => EndBlock,
        "extends" => Extends,
        "include" => Include,
        "load" => Load,

        // Comments
        "comment" => Comment,
        "endcomment" => EndComment,
        "raw" => Raw,
        "endraw" => EndRaw,

        // Constants
        "True" => True,
        "False" => False,

        // Template importing
        "import" => Import,

        // Modifiers
        "reversed" => Reversed,
    }
}

keyword_table! {
    /// The keywords that only Liquid has, which are its [`contains`] operator and
    /// some of its tags: [control flow], [iteration], [`capture`], [`render`] and
    /// [`doc`].
    ///
    /// [`contains`]: https://shopify.github.io/liquid/basics/operators/#contains
    /// [control flow]: https://shopify.github.io/liquid/tags/control-flow/
    /// [iteration]: https://shopify.github.io/liquid/tags/iteration/
    /// [`capture`]: https://shopify.github.io/liquid/tags/variable/#capture
    /// [`render`]: https://shopify.github.io/liquid/tags/template/#render
    /// [`doc`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/doc.rb#L30
    fn liquid_keyword {
        "contains" => Contains,
        "unless" => Unless,
        "endunless" => EndUnless,
        "case" => Case,
        "when" => When,
        "endcase" => EndCase,
        "tablerow" => TableRow,
        "endtablerow" => EndTableRow,
        "capture" => Capture,
        "endcapture" => EndCapture,
        "render" => Render,
        "doc" => Doc,
        "enddoc" => EndDoc,
    }
}

impl TryFrom<&str> for Keyword {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        keyword(Name::new(value)).ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::Name;

    #[test]
    fn names_pack_apart() {
        let names = ["", "a", "a\0", "for", "for\0", "fo", "endtablerow", "elsif", "elif"];
        for (index, name) in names.iter().enumerate() {
            for other in &names[index + 1..] {
                assert_ne!(Name::new(name), Name::new(other), "{name:?} and {other:?}");
            }
        }

        assert_eq!(Name::new("sixteen_bytes_ab"), Name::LONG);
        assert_ne!(Name::new("fifteen_bytes_a"), Name::LONG);
    }
}
