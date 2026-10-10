//! The module contains all of the AST definitions for the templates that
//! `bracketlint` supports parsing.
//!
//! A tree lives in an arena, a [bumpalo::Bump] that the parser is given, and
//! each node refers to its children by reference into it. A tree isn't changed
//! once it is parsed, and none of its nodes need dropping, so the whole tree is
//! freed at once, with its arena.

use std::{fmt, ops::Deref};

use bl_macros::define_tree;

use crate::ByteRange;

/// A node of a tree, and its location in its source file.
#[derive(Debug, PartialEq)]
pub struct AstNode<'a, T> {
    /// The node, in the arena that the tree lives in.
    pub body: &'a T,

    /// The location of the node in its source file.
    pub range: ByteRange,
}

/// A reference to a node, which a visitor is given. An [AstNode] is itself a
/// reference into the arena, so the two are the same.
pub type AstNodeRef<'a, T> = AstNode<'a, T>;

impl<T> Clone for AstNode<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for AstNode<'_, T> {}

impl<'a, T> AstNode<'a, T> {
    /// Create an [AstNode] of a `body` in the arena, at the given [ByteRange].
    pub fn new(body: &'a T, range: ByteRange) -> Self {
        Self { body, range }
    }

    /// Get an [AstNodeRef] to this node.
    pub fn ast_ref(&self) -> AstNodeRef<'a, T> {
        *self
    }

    /// Get a reference to the body of the node.
    pub fn body(&self) -> &'a T {
        self.body
    }

    /// Create an [AstNodeRef] by providing a body and copying over the
    /// [ByteRange] of this [AstNode].
    pub fn with_body<'u, U>(&self, body: &'u U) -> AstNodeRef<'u, U> {
        AstNode { body, range: self.range }
    }

    /// Get the location of this node in its source file.
    pub fn range(&self) -> ByteRange {
        self.range
    }
}

/// [AstNode] dereferences to its inner `body` type.
impl<T> Deref for AstNode<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.body
    }
}

/// A collection of [AstNode]s with an optional shared
/// span. This is often used to represent collections
/// of [AstNode]s when they are wrapped within some kind
/// of delimiter.
#[derive(Debug, PartialEq)]
pub struct AstNodes<'a, T> {
    /// The nodes that the [AstNodes] holds, in the arena that the tree lives
    /// in.
    pub nodes: &'a [AstNode<'a, T>],

    /// The location of the nodes in their source file, including any
    /// delimiters around them.
    range: ByteRange,
}

impl<T> Clone for AstNodes<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for AstNodes<'_, T> {}

impl<'a, T> AstNodes<'a, T> {
    /// Create an empty [AstNodes] at the given [ByteRange].
    pub fn empty(range: ByteRange) -> Self {
        Self::new(&[], range)
    }

    /// Create an [AstNodes] of `nodes` in the arena, at the given [ByteRange].
    pub fn new(nodes: &'a [AstNode<'a, T>], range: ByteRange) -> Self {
        Self { nodes, range }
    }

    /// Get the location of this [AstNodes] in its source file.
    pub fn range(&self) -> ByteRange {
        self.range
    }

    /// Iterate over each child as an [AstNodeRef].
    pub fn ast_ref_iter(&self) -> impl Iterator<Item = AstNodeRef<'a, T>> + 'a {
        self.nodes.iter().copied()
    }
}

impl<'a, T> Deref for AstNodes<'a, T> {
    type Target = [AstNode<'a, T>];

    fn deref(&self) -> &Self::Target {
        self.nodes
    }
}

define_tree! {
    opts! {{
        node_type_name: AstNode,
        nodes_type_name: AstNodes,
        visitor_trait_base_name: AstVisitor,
        visitor_node_ref_base_type_name: AstNodeRef,
        get_ref_from_node_function_base_name: ast_ref,
        ref_change_body_function_base_name: with_body,
        root_module: bl_ast::ast,
    }}

    /// All binary operators. Which of them a dialect has, and how tightly each
    /// one binds, depends on the dialect, see [crate::Dialect::infix_binding_power].
    #[derive(Copy, Clone, Debug, PartialEq)]
    #[node]
    pub enum BinOp {
        /// >
        Gt,
        /// >=
        GtEq,
        /// <
        Lt,
        /// <=
        LtEq,
        /// ==
        Eq,
        /// !=
        NotEq,
        /// `and``
        And,
        /// `or`
        Or,
        /// `not in`
        NotIn,
        /// `in`
        In,
        /// `is`
        Is,
        /// `is not`
        IsNot,
        /// `contains`, in Liquid
        Contains,
        /// `+`, in Jinja and Twig
        Add,
        /// `-`, in Jinja and Twig
        Sub,
        /// `*`, in Jinja and Twig
        Mul,
        /// `/`, in Jinja and Twig
        Div,
        /// `//`, in Jinja and Twig
        FloorDiv,
        /// `%`, in Jinja and Twig
        Mod,
        /// `**`, in Jinja and Twig
        Pow,
        /// `~`, which concatenates strings, in Jinja and Twig
        Concat,
    }

    impl fmt::Display for BinOp {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                BinOp::Gt => write!(f, ">"),
                BinOp::GtEq => write!(f, ">="),
                BinOp::Lt => write!(f, "<"),
                BinOp::LtEq => write!(f, "<="),
                BinOp::Eq => write!(f, "=="),
                BinOp::NotEq => write!(f, "!="),
                BinOp::And => write!(f, "and"),
                BinOp::Or => write!(f, "or"),
                BinOp::NotIn => write!(f, "not in"),
                BinOp::In => write!(f, "in"),
                BinOp::Is => write!(f, "is"),
                BinOp::IsNot => write!(f, "is not"),
                BinOp::Contains => write!(f, "contains"),
                BinOp::Add => write!(f, "+"),
                BinOp::Sub => write!(f, "-"),
                BinOp::Mul => write!(f, "*"),
                BinOp::Div => write!(f, "/"),
                BinOp::FloorDiv => write!(f, "//"),
                BinOp::Mod => write!(f, "%"),
                BinOp::Pow => write!(f, "**"),
                BinOp::Concat => write!(f, "~"),
            }
        }
    }

    /// Unary operators. How tightly each one binds depends on the dialect, see
    /// [crate::Dialect::prefix_binding_power].
    #[derive(Copy, Clone, Debug, PartialEq)]
    #[node]
    pub enum UnaryOp {
        /// `not`
        Not,
        /// -
        Neg,
    }

    impl fmt::Display for UnaryOp {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                UnaryOp::Not => write!(f, "not"),
                UnaryOp::Neg => write!(f, "-"),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct BoolLit {
        pub value: bool,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct FloatLit {
        pub value: f64,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct IntLit {
        pub value: i64,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct StrLit {
    }

    /// Liquid's `nil`, which can also be written `null`.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct NilLit {
    }

    /// Liquid's `empty` or `blank`, which values are only compared to, e.g.
    /// `x == empty`.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct EmptyLit {
        pub kind: EmptyKind,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum EmptyKind {
        /// `empty`, which an empty string, array or hash is equal to.
        Empty,
        /// `blank`, which `empty` values and strings of only whitespace are
        /// equal to.
        Blank,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Lit {
        Bool(BoolLit),
        Float(FloatLit),
        Int(IntLit),
        Str(StrLit),
        Nil(NilLit),
        Empty(EmptyLit),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct LitExpr {
        pub lit: Lit
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct ArrayExpr {
        children: Children!(Expr),
    }

    /// A binary operation, e.g. `a == b`. Jinja chains comparisons, so in
    /// Jinja `(a == b) > c` means `a == b and b > c`.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct BinExpr {
        pub lhs: Child!(Expr),
        pub rhs: Child!(Expr),
        pub op: Child!(BinOp),
    }

    pub type Identifier = u32;

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Name {
        data: Identifier
    }

    impl Name {
        pub fn new(data: Identifier) -> Self {
            Self { data }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct VarExpr {
        pub name: Name,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct UnaryExpr {
        pub op: Child!(UnaryOp),
        pub expr: Child!(Expr),
    }

    /// An argument to a function, filter or a custom taf call.
    ///
    /// ##Note: It is an invariant for both `name` and `value` to
    /// be [`None`] at the same time.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Arg {
        pub name: OptionalChild!(Name),
        pub value: OptionalChild!(Expr),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct CallExpr {
        pub subject: Child!(Expr),
        pub args: Children!(Arg),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct MacroCallExpr {
        name: Child!(Name),
        namespace: Child!(Name),
        args: Children!(Arg),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct FilteredExpr {
        pub subject: Child!(Expr),
        pub filter: Child!(Filter),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Filter {
        pub name: Child!(Name),
        pub args: Children!(Arg),
    }

    /// An "access" expression, when a field is being accessed
    /// from a subject, i.e.
    ///
    /// ```html
    /// {{ x.y }}
    /// ```
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct AccessExpr {
        pub subject: Child!(Expr),
        pub field: Child!(Name),
    }

    /// An "index" expression, when an index is being accessed
    /// from a subject, i.e.
    /// ```html
    /// {{ x[0] }}
    /// ```
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct IndexExpr {
        pub subject: Child!(Expr),
        pub index: Child!(Expr),
    }

    /// A range of integers, i.e. `(1..n)` in Liquid.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct RangeExpr {
        pub start: Child!(Expr),
        pub end: Child!(Expr),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Expr {
        Unary(UnaryExpr),
        Range(RangeExpr),
        Lit(LitExpr),
        Array(ArrayExpr),
        Bin(BinExpr),
        Var(VarExpr),
        Call(CallExpr),
        MacroCall(MacroCallExpr),
        Access(AccessExpr),
        Index(IndexExpr),
        FilteredExpr(FilteredExpr)
    }

    impl Expr<'_> {
        pub fn is_var(&self) -> bool {
            matches!(self, Expr::Var(_))
        }
    }

    /// A `block` tag, which can be used to define a block of code that can be
    /// overridden by a child template.
    ///
    /// ```html
    /// {% block name %}
    /// {% endblock %}
    /// ```
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Block {
        pub label: OptionalChild!(Name),
        pub block_body: Child!(Body),

        /// The label repeated in `{% endblock label %}`, if there is one.
        pub end_label: OptionalChild!(Name),

        /// The whitespace control of the opening and the closing tag.
        pub trim: TrimTag,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct MacroDef {
        name: Child!(Name),
        args: Children!(Name),
        block_body: Child!(Body),
    }

    /// Directly insert the contents of another file into the current template.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Include {
        pub template: Child!(Expr),

        /// Optionally, this also has a notion of arguments in the form of key
        /// value pairs, which follow the scheme `<key ~ ident> = <value ~ expr>`.
        /// For example:
        ///
        /// ```html
        /// {% include "file" with x = 10 %}
        /// ```
        pub context: Children!(Arg),
    }

    /// A Liquid [`render`] tag, which renders another template with variables of
    /// its own, i.e.
    ///
    /// ```liquid
    /// {% render "card", product: product %}
    /// {% render "card" with featured as product %}
    /// {% render "card" for products as product %}
    /// ```
    ///
    /// [`render`]: https://shopify.github.io/liquid/tags/template/#render
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Render {
        pub template: Child!(Expr),

        /// The value after `with`, which the template sees as a variable.
        pub with_value: OptionalChild!(Expr),

        /// The value after `for`, which the template is rendered for each item
        /// of.
        pub for_value: OptionalChild!(Expr),

        /// The name after `as`, which the template sees the value as.
        pub alias: OptionalChild!(Name),
        pub args: Children!(Arg),
    }

    /// A Liquid [`capture`] block, which renders its body into a variable, i.e.
    ///
    /// ```liquid
    /// {% capture greeting %}Hello {{ name }}{% endcapture %}
    /// ```
    ///
    /// [`capture`]: https://shopify.github.io/liquid/tags/variable/#capture
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Capture {
        pub name: Child!(Name),
        pub block_body: Child!(Body),

        /// The whitespace control of the opening and the closing tag.
        pub trim: TrimTag,
    }

    /// A Liquid `liquid` tag, which holds a tag on each of its lines, i.e.
    ///
    /// ```liquid
    /// {% liquid
    ///   assign total = cart.total_price
    ///   echo total
    /// %}
    /// ```
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct LiquidTag {
        pub block_body: Child!(Body),
    }

    /// Extend the current template with the contents of another file.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Extends {
        pub template: Child!(Expr),
        pub trim: TrimMarker,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Import {
        pub template: Child!(StrLit),
        pub names: Children!(Name),
    }

    /// A tag to set a value in place, i.e.
    ///
    /// ```html
    /// {% set x = 10 %}
    /// ```
    ///
    /// or
    ///
    /// ```html
    /// {% with x = 10 %}
    /// {% endwith %}
    /// ```
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct With {
        pub assignments: Children!(Assignment),
        pub block_body: Child!(Body),
        pub kind: AssignmentKind,

        /// The whitespace control of the opening and the closing tag.
        pub trim: TrimTag,
    }

    #[derive(Clone, Debug, PartialEq, Copy)]
    pub enum AssignmentKind {
        /// A `set` block, which is used to set a value in place.
        Set,

        /// A `with` block, which is used to set a value in place, and then
        /// restore the original value after the block has been executed.
        With,
    }

    /// A tag to set a value in place, i.e.
    ///
    /// This represents a single assignment in the form of `name = value` which
    /// can either be present within a `with` block or a `set` block.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Assignment {
        pub name: Child!(Name),
        pub value: Child!(Expr),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Super {
    }

    /// A generic tag, which can be used to represent user specific tags, and built-in tags, like
    /// `block` or `extends`.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct GenericTag {
        pub name: Child!(Name),
        pub args: Children!(Arg),
    }

    /// A tag that has some combination of unprocessable characters, this is used to represent
    /// tags that are not recognized by the parser, and are not part of the standard tags like
    /// `block` or `extends`.
    ///
    /// This is different from an error since it might be a valid tag, but it is not recognized
    /// by the parser. It might be a custom tag which defines a custom parser.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct UnprocessableTag {

    }

    /// A hunk of text, the [ByteRange] of this node exactly represents the range
    /// of text that this node represents.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Text {
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Var {
        name: Child!(Name),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Body {
        pub contents: Children!(Statement),
    }

    /// The whitespace control markers written next to a tag's delimiters, e.g.
    /// the `-` in `{%- if x -%}`, which strip the whitespace on that side of
    /// the tag. Each side holds its marker, if it has one.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct TrimMarker {
        pub left: Option<char>,
        pub right: Option<char>,
    }

    /// The whitespace control markers of a block's opening and closing tags,
    /// e.g. `{%- with x = 1 %}` and `{% endwith -%}`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct TrimTag {
        pub start: TrimMarker,
        pub end: TrimMarker,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum ClauseKind {
        If,
        Elif,
        /// The opening clause of a Liquid `unless` block, which renders when
        /// its condition is false.
        Unless,
    }

    #[derive(Debug, PartialEq, Clone)]
    #[node]
    pub struct IfClause {
        /// The opening tag of the `if` block.
        pub kind: ClauseKind,

        /// The condition of the `if` block.
        pub condition: Child!(Expr),
        /// The body of the `if-statement`
        pub clause_body: Child!(Body),

        /// The whitespace control of the clause's tag.
        pub trim: TrimMarker,
    }

    /// An `if` block consisting of the condition, block and an optional else clause
    /// e.g. `{% if x %} ...  {% else %}  y {% endif %}`. A Liquid `unless` block
    /// is one too, whose first clause is [ClauseKind::Unless].
    #[derive(Debug, PartialEq, Clone)]
    #[node]
    pub struct If {
        pub clauses: Children!(IfClause),
        /// The else clause.
        pub otherwise: OptionalChild!(Body),

        /// The whitespace control of the `{% else %}` and `{% endif %}` tags.
        pub else_trim: TrimMarker,
        pub end_trim: TrimMarker,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum LoopKind {
        For,
        /// A Liquid `tablerow` loop, which renders a table row per item.
        TableRow,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct For {
        pub kind: LoopKind,
        pub target: Child!(ForTarget),
        pub iterator: Child!(Expr),
        pub guard: OptionalChild!(Expr),
        pub reverse_modifier: OptionalChild!(Name),

        /// The parameters of a Liquid loop, e.g. `limit: 2` or `cols: 3`.
        pub params: Children!(Arg),
        pub loop_body: Child!(Body),

        /// The body after `{% empty %}`, or `{% else %}` outside Django, which
        /// renders when there is nothing to loop over.
        pub loop_empty: OptionalChild!(Body),

        /// The whitespace control of the opening, `{% empty %}` and closing
        /// tags.
        pub trim: TrimTag,
        pub empty_trim: TrimMarker,
    }

    /// A Liquid `case` block, which renders the first `when` clause with a value
    /// equal to its subject, or else its `else` clause.
    ///
    /// ```liquid
    /// {% case product.type %}
    ///     {% when "shirt", "hat" %} Apparel
    ///     {% else %} Other
    /// {% endcase %}
    /// ```
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Case {
        pub subject: Child!(Expr),

        /// Anything between `{% case %}` and the first `{% when %}`, which
        /// Liquid doesn't render.
        pub leading: Child!(Body),
        pub branches: Children!(When),
        pub otherwise: OptionalChild!(Body),

        /// The whitespace control of the `{% case %}`, `{% else %}` and
        /// `{% endcase %}` tags.
        pub trim: TrimTag,
        pub else_trim: TrimMarker,
    }

    /// A `{% when %}` clause of a [`Case`] block, whose values are separated by
    /// `,` or `or`.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct When {
        pub values: Children!(Expr),
        pub clause_body: Child!(Body),
        pub trim: TrimMarker,
    }

    /// The target of a [`For`] loop, which can be a simple variable or a variable
    /// with an optional value, as in [Jinja].
    ///
    /// For example:
    ///
    /// ```html
    ///
    /// {% for item in items %}
    ///    {{ item }}
    /// {% endfor %}
    ///
    /// {% for key, value in items %}
    ///   {{ key }}: {{ value }}
    /// {% endfor %}
    /// ```
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#for
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct ForTarget {
        pub items : Children!(Name),
    }

    /// Control flow statement to skip the current iteration of a [`For`] loop.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Continue {
    }

    /// Control flow statement to stop the iteration of a [`For`] loop.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Break {
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Raw {
        pub block_body: Child!(Body),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Comment {
    }

    /// ##Note: The tags that are much larger than the others are behind a
    /// [AstNode], so that a [Tag], and every [Statement], stays small.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Tag {
        /// A generic tag, which hasn't been terminated.
        Generic(GenericTag),

        /// A tag that has some combination of characters that are unprocessable.
        Unprocessable(UnprocessableTag),

        /// The `{% block name %}` tag, ending with `{% endblock %}`
        Block(Child!(Block)),

        /// The `{% with x = 10 %}` tag, ending with `{% endwith %}`
        With(Child!(With)),

        /// Assignment, just like `with` but with a single inline assignment.
        ///
        /// ```html
        /// {% 10 as x %}
        /// ```
        Assignment(Assignment),

        /// The `{% macro name() %}` tag, ending with `{% endmacro %}`
        MacroDef(MacroDef),

        /// The `{% include "file" %}` tag
        Include(Include),

        /// The `{% extends "file" %}` tag
        Extends(Extends),

        /// The `{% import "file" %}` tag
        Import(Import),

        /// The `{% if condition %}` tag
        If(If),

        /// The `{% for item in items %}` tag
        For(Child!(For)),

        /// The Liquid `{% case subject %}` tag
        Case(Child!(Case)),

        /// The Liquid `{% render "file" %}` tag
        Render(Child!(Render)),

        /// The Liquid `{% capture name %}` tag, ending with `{% endcapture %}`
        Capture(Capture),

        /// The Liquid `{% liquid %}` tag, with a tag on each of its lines
        Liquid(LiquidTag),

        /// The `{% continue %}` tag
        Continue(Continue),

        /// The `{% break %}` tag
        Break(Break),

        /// The `{% raw %}` tag, ending with `{% endraw %}`
        Raw(Raw),
    }

    impl Tag<'_> {
        pub fn _continue() -> Self {
            Tag::Continue(Continue {})
        }

        pub fn _break() -> Self {
            Tag::Break(Break {})
        }

        pub fn kind(&self) -> &'static str {
            match self {
                Tag::Unprocessable(_) => "unprocessable",
                Tag::Generic(_) => "generic tag",
                Tag::Assignment(_) => "assignment",
                Tag::Block(_) => "block",
                Tag::With(_) => "with",
                Tag::MacroDef(_) => "macro",
                Tag::Include(_) => "include",
                Tag::Extends(_) => "extends",
                Tag::Import(_) => "import",
                Tag::If(_) => "if",
                Tag::For(_) => "for",
                Tag::Case(_) => "case",
                Tag::Render(_) => "render",
                Tag::Capture(_) => "capture",
                Tag::Liquid(_) => "liquid",
                Tag::Continue(_) => "continue",
                Tag::Break(_) => "break",
                Tag::Raw(_) => "raw",
            }
        }

        pub fn is_inline(&self) -> bool {
            matches!(self, Tag::Unprocessable(_) | Tag::Assignment(_) | Tag::Continue(_) | Tag::Break(_) | Tag::Generic(_) | Tag::Render(_) | Tag::Capture(_) | Tag::Liquid(_) )
        }

    }


    /// A statement level expression.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Inline {
        pub expr: Child!(Expr),
        pub trim: TrimMarker,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Statement {
        /// A hunk of text.
        Text(Text),

        /// A tag `{%  ... <tag> ... %}` which is eventually terminated by a `{% end<tag> %}` tag.
        Tag(Tag),

        /// A interpolated variable or function call `{{ var }}`
        Inline(Inline),

        /// Comment `{# comment #}` tag.
        Comment(Comment),
    }

    impl Statement<'_> {
        pub fn text() -> Self {
            Statement::Text(Text {})
        }

        pub fn is_inline(&self) -> bool {
            matches!(self, Statement::Inline(_))
        }

        pub fn is_tag(&self) -> bool {
            matches!(self, Statement::Tag(_))
        }

        pub fn is_comment(&self) -> bool {
            matches!(self, Statement::Comment(_))
        }

        pub fn is_text(&self) -> bool {
            matches!(self, Statement::Text(_))
        }

        pub fn kind(&self) -> &'static str {
            match self {
                Statement::Text(_) => "text",
                Statement::Tag(tag) => tag.kind(),
                Statement::Inline(_) => "inline",
                Statement::Comment(_) => "comment",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    #[node]
    pub struct Document {
        pub document: Child!(Body),
    }
}
