//! Language token keyword definitions.
use std::fmt;

use bl_workspace::Dialect;
use num_derive::FromPrimitive;
use phf::phf_map;
use strum_macros::AsRefStr;

/// Template language keywords. Most of them are shared by every dialect, and
/// [Keyword::from_ident] finds the ones that a dialect has.
#[derive(Debug, Copy, Clone, PartialEq, Eq, AsRefStr, FromPrimitive)]
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
    /// `include` - Includes another template
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
        )
    }

    /// The keyword that the identifier `name` is in `dialect`, if any. Liquid
    /// has keywords of its own, and spells `elif` as `elsif`.
    pub fn from_ident(name: &str, dialect: Dialect) -> Option<Keyword> {
        if name == dialect.elif_tag() {
            return Some(Keyword::Elif);
        }

        match dialect {
            Dialect::Liquid if name == "elif" => None,
            Dialect::Liquid => LIQUID_KEYWORDS.get(name).or_else(|| KEYWORDS.get(name)).copied(),
            _ => KEYWORDS.get(name).copied(),
        }
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

/// A static map of keywords to their enum variants using a
/// perfect hashing function to quickly lookup the keyword.
static KEYWORDS: phf::Map<&'static str, Keyword> = phf_map! {
    // Control flow
    "for" => Keyword::For,
    "endfor" => Keyword::EndFor,
    "if" => Keyword::If,
    "elif" => Keyword::Elif,
    "else" => Keyword::Else,
    "endif" => Keyword::EndIf,
    "break" => Keyword::Break,
    "continue" => Keyword::Continue,
    "empty" => Keyword::Empty,

    // Logical operators
    "and" => Keyword::And,
    "or" => Keyword::Or,
    "not" => Keyword::Not,
    "in" => Keyword::In,
    "as" => Keyword::As,
    "is" => Keyword::Is,

    // Context management
    "with" => Keyword::With,
    "endwith" => Keyword::EndWith,

    // Template structure
    "block" => Keyword::Block,
    "endblock" => Keyword::EndBlock,
    "extends" => Keyword::Extends,
    "include" => Keyword::Include,
    "load" => Keyword::Load,

    // Comments
    "comment" => Keyword::Comment,
    "endcomment" => Keyword::EndComment,
    "raw" => Keyword::Raw,
    "endraw" => Keyword::EndRaw,

    // Constants
    "True" => Keyword::True,
    "False" => Keyword::False,

    // Template importing
    "import" => Keyword::Import,

    // Modifiers
    "reversed" => Keyword::Reversed,
};

/// The keywords that only Liquid has, which are its `contains` operator and
/// some of its tags:
/// - https://shopify.github.io/liquid/basics/operators/
/// - https://shopify.github.io/liquid/tags/control-flow/
/// - https://shopify.github.io/liquid/tags/iteration/
static LIQUID_KEYWORDS: phf::Map<&'static str, Keyword> = phf_map! {
    "contains" => Keyword::Contains,
    "unless" => Keyword::Unless,
    "endunless" => Keyword::EndUnless,
    "case" => Keyword::Case,
    "when" => Keyword::When,
    "endcase" => Keyword::EndCase,
    "tablerow" => Keyword::TableRow,
    "endtablerow" => Keyword::EndTableRow,
    "capture" => Keyword::Capture,
    "endcapture" => Keyword::EndCapture,
    "render" => Keyword::Render,
};

impl TryFrom<&str> for Keyword {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match KEYWORDS.get(value) {
            Some(keyword) => Ok(*keyword),
            None => Err(()),
        }
    }
}
