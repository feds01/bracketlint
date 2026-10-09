//! The module contains all of the AST definitions for the templates that
//! `bracketlint` supports parsing.

use std::{
    fmt,
    ops::{Deref, DerefMut},
};

use bl_macros::define_tree;
use replace_with::replace_with_or_abort;
use thin_vec::{ThinVec, thin_vec};

use crate::ByteRange;

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode<T> {
    pub body: Box<T>,

    /// The location of the node in its source file.
    pub range: ByteRange,
}

impl<T> AstNode<T> {
    /// Create an [AstNode] at the given [ByteRange].
    pub fn new(body: T, range: ByteRange) -> Self {
        Self { body: Box::new(body), range }
    }

    /// Create an [AstNodeRef] from this [AstNode].
    pub fn ast_ref(&self) -> AstNodeRef<'_, T> {
        AstNodeRef { body: self.body.as_ref(), range: self.range }
    }

    /// Create an [AstNodeRefMut] from this [AstNode].
    pub fn ast_ref_mut(&mut self) -> AstNodeRefMut<'_, T> {
        AstNodeRefMut { body: self.body.as_mut(), range: self.range }
    }

    /// Create an [AstNodeRef] by providing a body and copying over the
    /// [ByteRange] of this [AstNode].
    pub fn with_body<'u, U>(&self, body: &'u U) -> AstNodeRef<'u, U> {
        AstNodeRef { body, range: self.range }
    }
}

#[derive(Debug)]
pub struct AstNodeRef<'t, T> {
    /// A reference to the body of the [AstNode].
    pub body: &'t T,

    /// The location of the node in its source file.
    pub range: ByteRange,
}

impl<T> Clone for AstNodeRef<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for AstNodeRef<'_, T> {}

impl<'t, T> AstNodeRef<'t, T> {
    /// Create a new [AstNodeRef<T>].
    pub fn new(body: &'t T, range: ByteRange) -> Self {
        AstNodeRef { body, range }
    }

    /// Get a reference to body of the [AstNodeRef].
    pub fn body(&self) -> &'t T {
        self.body
    }

    /// Utility function to copy over the [ByteRange] from
    /// another [AstNodeRef] with a provided body.
    pub fn with_body<'u, U>(&self, body: &'u U) -> AstNodeRef<'u, U> {
        AstNodeRef { body, range: self.range }
    }

    /// Get the location of this [AstNodeRef] in its source file.
    pub fn range(&self) -> ByteRange {
        self.range
    }
}

/// [AstNode] dereferences to its inner `body` type.
impl<T> Deref for AstNodeRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.body()
    }
}

#[derive(Debug)]
pub struct AstNodeRefMut<'t, T> {
    /// A mutable reference to the body of the [AstNode].
    body: &'t mut T,

    /// The location of the node in its source file.
    pub range: ByteRange,
}

impl<'t, T> AstNodeRefMut<'t, T> {
    /// Create a new [AstNodeRefMut<T>].
    pub fn new(body: &'t mut T, range: ByteRange) -> Self {
        AstNodeRefMut { body, range }
    }

    /// Get a reference to body of the [AstNodeRefMut].
    pub fn body(&self) -> &T {
        self.body
    }

    /// Replace the body of the [AstNodeRefMut] with another body.
    pub fn replace(&mut self, f: impl FnOnce(T) -> T) {
        replace_with_or_abort(self.body, f);
    }

    /// Get a mutable reference to the body.
    pub fn body_mut(&mut self) -> &mut T {
        self.body
    }

    /// Get the location of this [AstNodeRefMut] in its source file.
    pub fn range(&self) -> ByteRange {
        self.range
    }

    /// Get this node as an immutable reference
    pub fn immutable(&self) -> AstNodeRef<'_, T> {
        AstNodeRef::new(self.body, self.range)
    }
}

impl<T> Deref for AstNodeRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.body()
    }
}

impl<T> DerefMut for AstNodeRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.body
    }
}

/// Helper trait to access a node from a structure that contains one.
pub trait OwnsAstNode<T> {
    /// Get a reference to [AstNode<T>].
    fn node(&self) -> &AstNode<T>;

    /// Get a mutable reference to [AstNode<T>].
    fn node_mut(&mut self) -> &mut AstNode<T>;

    /// Get a [AstNodeRef<T>].
    fn node_ref(&self) -> AstNodeRef<'_, T> {
        self.node().ast_ref()
    }

    /// Get a [AstNodeRefMut<T>].
    fn node_ref_mut(&mut self) -> AstNodeRefMut<'_, T> {
        self.node_mut().ast_ref_mut()
    }
}

/// A collection of [AstNode]s with an optional shared
/// span. This is often used to represent collections
/// of [AstNode]s when they are wrapped within some kind
/// of delimiter.
#[derive(Debug, PartialEq, Clone)]
pub struct AstNodes<T> {
    /// The nodes that the [AstNodes] holds.
    pub nodes: ThinVec<AstNode<T>>,

    /// The location of the nodes in their source file, including any
    /// delimiters around them.
    range: ByteRange,
}

impl<T> AstNodes<T> {
    /// Create a new [AstNodes].
    pub fn empty(range: ByteRange) -> Self {
        Self::new(thin_vec![], range)
    }

    /// Create an [AstNodes] with items at the given [ByteRange].
    pub fn new(nodes: ThinVec<AstNode<T>>, range: ByteRange) -> Self {
        Self { nodes, range }
    }

    /// Function to adjust the span location of [AstNodes] if it is initially
    /// incorrectly offset because there is a 'pre-conditional' token that must
    /// be parsed before parsing the nodes. This token could be something like a
    /// '<' or '(' which starts a tuple, or type bound
    pub fn set_range(&mut self, range: ByteRange) {
        self.range = range;
    }

    /// Get the location of this [AstNodes] in its source file.
    pub fn range(&self) -> ByteRange {
        self.range
    }

    /// Insert an item into the [AstNodes] at a particular index.
    pub fn insert(&mut self, item: AstNode<T>, index: usize) {
        self.nodes.insert(index, item);
    }

    /// Merge two [AstNodes] together, this will append the nodes of the
    /// other [AstNodes] to this one, and then return the new [AstNodes].
    ///
    /// **Note** this will automatically update the [ByteRange] of this node
    /// by extending it with the range of the other node.
    pub fn merge(&mut self, other: Self) {
        self.set_range(self.range.join(other.range));
        self.nodes.extend(other.nodes);
    }

    /// Iterate over each child whilst wrapping it in a [AstNodeRef].
    pub fn ast_ref_iter(&self) -> impl Iterator<Item = AstNodeRef<'_, T>> {
        self.nodes.iter().map(|x| x.ast_ref())
    }
}

impl<T> Deref for AstNodes<T> {
    type Target = [AstNode<T>];
    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}
impl<T> DerefMut for AstNodes<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
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

    impl Expr {
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

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Tag {
        /// A generic tag, which hasn't been terminated.
        Generic(GenericTag),

        /// A tag that has some combination of characters that are unprocessable.
        Unprocessable(UnprocessableTag),

        /// The `{% block name %}` tag, ending with `{% endblock %}`
        Block(Block),

        /// The `{% with x = 10 %}` tag, ending with `{% endwith %}`
        With(With),

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
        For(For),

        /// The Liquid `{% case subject %}` tag
        Case(Case),

        /// The Liquid `{% render "file" %}` tag
        Render(Render),

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

    impl Tag {
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

    impl Statement {
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
