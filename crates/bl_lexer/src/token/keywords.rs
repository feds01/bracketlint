//! Language token keyword definitions.
use std::fmt;

use num_derive::FromPrimitive;
use phf::phf_map;
use strum_macros::AsRefStr;

/// Django template language keywords.
/// Each variant represents a keyword that can appear in a Django template.
#[derive(Debug, Copy, Clone, PartialEq, Eq, AsRefStr, FromPrimitive)]
#[strum(serialize_all = "snake_case")]
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
    /// `csrf_token` - Generates a CSRF token for forms
    /// ```django
    /// <form method="post">
    ///     {% csrf_token %}
    ///     {{ form }}
    /// </form>
    /// ```
    CsrfToken,
    /// `url` - Generates a URL for a given view
    /// ```django
    /// <a href="{% url 'view-name' arg1 arg2 %}">Link</a>
    /// ```
    Url,
    /// `static` - Generates URL for static files
    /// ```django
    /// <img src="{% static 'images/logo.png' %}">
    /// ```
    Static,
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

    // Special tags
    "csrf_token" => Keyword::CsrfToken,
    "url" => Keyword::Url,
    "static" => Keyword::Static,

    // Constants
    "True" => Keyword::True,
    "False" => Keyword::False,

    // Template importing
    "import" => Keyword::Import,
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
