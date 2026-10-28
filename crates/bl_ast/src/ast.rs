//! The module contains all of the AST definitions for the templates that
//! `bracketlint` supports parsing.
//!
//! A tree lives in an arena, a [bl_arena::Arena] that the parser is given, and
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
        /// `===`, equal without converting either side, in [Nunjucks] and
        /// [Twig]
        ///
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#comparisons
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#comparisons
        StrictEq,
        /// `!==`, not equal without converting either side, in [Nunjucks] and
        /// [Twig]
        ///
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#comparisons
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#comparisons
        StrictNotEq,
        /// `<=>`, which compares its sides as `-1`, `0` or `1`, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#spaceship-operator
        Spaceship,
        /// `xor`, whether exactly one of its sides is true, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#logic
        Xor,
        /// `b-and`, the bitwise and, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#logic
        BitAnd,
        /// `b-or`, the bitwise or, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#logic
        BitOr,
        /// `b-xor`, the bitwise exclusive or, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#logic
        BitXor,
        /// `has some`, whether an arrow function holds for some item, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#iterable-operators
        HasSome,
        /// `has every`, whether an arrow function holds for every item, in
        /// [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#iterable-operators
        HasEvery,
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
        /// `contains`, in [Liquid]
        ///
        /// [Liquid]: https://shopify.github.io/liquid/basics/operators/#contains
        Contains,
        /// `+`, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#math
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#math
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#math
        Add,
        /// `-`, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#math
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#math
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#math
        Sub,
        /// `*`, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#math
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#math
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#math
        Mul,
        /// `/`, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#math
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#math
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#math
        Div,
        /// `//`, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#math
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#math
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#math
        FloorDiv,
        /// `%`, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#math
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#math
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#math
        Mod,
        /// `**`, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#math
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#math
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#math
        Pow,
        /// `~`, which concatenates strings, in [Jinja], [Nunjucks] and [Twig]
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#other-operators
        /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L897-L908
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#other-operators
        Concat,
        /// `??`, the value on its left unless it is null or undefined, in
        /// [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#other-operators
        Coalesce,
        /// `?:`, the value on its left if it is truthy, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#other-operators
        Elvis,
        /// `..`, the integers from its left to its right, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#other-operators
        Range,
        /// `matches`, whether a string matches a regular expression, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#containment-operators
        Matches,
        /// `starts with`, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#containment-operators
        StartsWith,
        /// `ends with`, in [Twig]
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#containment-operators
        EndsWith,
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
                BinOp::StrictEq => write!(f, "==="),
                BinOp::StrictNotEq => write!(f, "!=="),
                BinOp::Spaceship => write!(f, "<=>"),
                BinOp::Xor => write!(f, "xor"),
                BinOp::BitAnd => write!(f, "b-and"),
                BinOp::BitOr => write!(f, "b-or"),
                BinOp::BitXor => write!(f, "b-xor"),
                BinOp::HasSome => write!(f, "has some"),
                BinOp::HasEvery => write!(f, "has every"),
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
                BinOp::Coalesce => write!(f, "??"),
                BinOp::Elvis => write!(f, "?:"),
                BinOp::Range => write!(f, ".."),
                BinOp::Matches => write!(f, "matches"),
                BinOp::StartsWith => write!(f, "starts with"),
                BinOp::EndsWith => write!(f, "ends with"),
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

    /// Liquid's [`nil`], which can also be written [`null`].
    ///
    /// [`nil`]: https://shopify.github.io/liquid/basics/types/#nil
    /// [`null`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/expression.rb#L9-L10
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct NilLit {
    }

    /// Liquid's [`empty`] or [`blank`], which values are only compared to, e.g.
    /// `x == empty`.
    ///
    /// [`empty`]: https://shopify.github.io/liquid/basics/types/#emptydrop
    /// [`blank`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/expression.rb#L14-L15
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

    /// A list, e.g. `['a', 'b']` in [Jinja], which [Nunjucks] calls an array
    /// and [Twig] a sequence.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#expressions
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct ArrayExpr {
        pub children: Children!(Expr),
    }

    /// A conditional expression, which is `then` if `condition` is true, and
    /// `otherwise` if not, e.g. `then if condition else otherwise` in [Jinja]
    /// and [Nunjucks], and `condition ? then : otherwise` in [Twig]. Without
    /// `otherwise`, it is undefined in Jinja, and an empty string in Twig and
    /// in Nunjucks, which [compiles] it to `""`.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#if-expression
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#if-expression
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#other-operators
    /// [compiles]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/compiler.js#L332-L344
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct CondExpr {
        pub condition: Child!(Expr),
        pub then: Child!(Expr),
        pub otherwise: OptionalChild!(Expr),
    }

    /// A slice of a sequence, e.g. `items[1:]`, in [Jinja] and [Twig]. Each
    /// part can be left out, and only Jinja's slices have a step, e.g.
    /// `[::2]`.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L840-L870
    /// [Twig]: https://twig.symfony.com/doc/3.x/filters/slice.html
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct SliceExpr {
        pub start: OptionalChild!(Expr),
        pub stop: OptionalChild!(Expr),
        pub step: OptionalChild!(Expr),
    }

    /// An arrow function in [Twig], e.g. `x => x.name` or `(a, b) => a + b`.
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#other-operators
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct ArrowExpr {
        pub params: Children!(Name),
        pub expr: Child!(Expr),
    }

    /// A mapping, e.g. `{'a': 1, b: 2}` in [Jinja] and [Nunjucks], where it is
    /// a dict, and [Twig].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#expressions
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct MappingExpr {
        pub pairs: Children!(Pair),
    }

    /// A key and its value in a mapping, e.g. `'a': 1`. In [Twig], the value
    /// of a name is left out when it is the variable of the same name, e.g.
    /// `{post}`.
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Pair {
        pub key: Child!(Expr),
        pub value: OptionalChild!(Expr),
    }

    /// A tuple, e.g. `('a', 'b')` in [Jinja], or a group of several expressions
    /// in [Nunjucks], e.g. `(rows.unshift(row), rows)`, which it [compiles] to
    /// JavaScript's, whose value is the last one's.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L1147-L1176
    /// [compiles]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/compiler.js#L302-L304
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct TupleExpr {
        pub children: Children!(Expr),
    }

    /// A binary operation, e.g. `a == b`. [Jinja] chains comparisons, so in
    /// Jinja `(a == b) > c` means `a == b and b > c`.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L560-L581
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

        /// How the argument is unpacked into several, if it is, e.g. [Jinja]'s
        /// `**kwargs`.
        ///
        /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L899-L906
        pub unpack: Option<Unpack>,
    }

    /// How an argument of a call is unpacked into several, as in [Jinja].
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L899-L906
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Unpack {
        /// `*args`, whose items are positional arguments.
        Items,
        /// `**kwargs`, whose pairs are named arguments.
        Pairs,
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

    /// A range of integers, i.e. `(1..n)` in [Liquid].
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/iteration/#range
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct RangeExpr {
        pub start: Child!(Expr),
        pub end: Child!(Expr),
    }

    /// A path that names a file without quotes, e.g. `nav/links.html` in
    /// [Jekyll]'s `{% include nav/links.html %}`. It can have [variables] in
    /// it, e.g. `icons/{{ icon }}.svg`, and is kept as it is written.
    ///
    /// [Jekyll]: https://jekyllrb.com/docs/includes/#passing-parameters-to-includes
    /// [variables]: https://jekyllrb.com/docs/includes/#using-variables-names-for-the-include-file
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct PathExpr {
    }

    /// The markup of a tag or a `{{ }}` that doesn't parse, which is kept as it
    /// is written, e.g. `(a == b) or c` in `{% if (a == b) or c %}` in
    /// [Liquid], whose `warn` error mode reads it with its lax parser, see
    /// [crate::Dialect::markup_can_be_lax].
    ///
    /// [Liquid]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/parser_switching.rb#L38-L44
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct UnparsedExpr {}

    /// An expression. Parentheses that group one, e.g. `(a or b)` in
    /// [Jinja], [Nunjucks] and [Twig], have no node: the expression in them is
    /// in the tree in their place, without them in its span, and the span of
    /// the expression around it covers them.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#logic
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#logic
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#logic
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Expr {
        Unary(UnaryExpr),
        Range(RangeExpr),
        Lit(LitExpr),
        Array(ArrayExpr),
        Tuple(TupleExpr),
        Mapping(MappingExpr),
        Cond(CondExpr),
        Arrow(ArrowExpr),
        Slice(SliceExpr),
        Bin(BinExpr),
        Var(VarExpr),
        Call(CallExpr),
        MacroCall(MacroCallExpr),
        Access(AccessExpr),
        Index(IndexExpr),
        FilteredExpr(FilteredExpr),
        Path(PathExpr),
        Unparsed(UnparsedExpr)
    }

    impl Expr<'_> {
        pub fn is_var(&self) -> bool {
            matches!(self, Expr::Var(_))
        }
    }

    /// A [Twig] block whose content is the value of an expression, e.g.
    /// `{% block title page_title|title %}`, which has no `{% endblock %}`.
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/extends.html#block-shortcuts
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct BlockShortcut {
        pub label: Child!(Name),
        pub value: Child!(Expr),
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

        /// Whether the block is `scoped`, as [Jinja] can make it.
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#block-nesting-and-scope
        pub scoped: bool,

        /// Whether the block is `required`, as [Jinja] can make it.
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#required-blocks
        pub required: bool,

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

        /// Whether the template sees this one's variables, when the tag says
        /// so with `with context` or `without context`, as in [Jinja].
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#import-context-behavior
        pub with_context: Option<bool>,

        /// Whether the tag is Jekyll's [`include_relative`], which names a file
        /// relative to this template, rather than in the site's `_includes`
        /// directory.
        ///
        /// [`include_relative`]: https://jekyllrb.com/docs/includes/#including-files-relative-to-another-file
        pub relative: bool,
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

        /// The text after the name of the closing tag, which [Liquid] ignores,
        /// see [crate::Dialect::else_and_end_tags_ignore_markup].
        ///
        /// [Liquid]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/block.rb#L84
        pub end_markup: Option<ByteRange>,
    }

    /// A Liquid [`liquid`] tag, which holds a tag on each of its lines, i.e.
    ///
    /// ```liquid
    /// {% liquid
    ///   assign total = cart.total_price
    ///   echo total
    /// %}
    /// ```
    ///
    /// [`liquid`]: https://shopify.github.io/liquid/tags/template/#liquid
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
        pub template: Child!(Expr),
        pub names: Children!(Name),

        /// Whether the template sees this one's variables, when the tag says
        /// so with `with context` or `without context`, as in [Jinja] and
        /// [Nunjucks].
        ///
        /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#import-context-behavior
        /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L263-L302
        pub with_context: Option<bool>,
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

        /// The variables of the block in [Twig], which is an expression whose
        /// value is a mapping, e.g. `{% with {a: 1} %}`, rather than
        /// assignments.
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/tags/with.html
        pub value: OptionalChild!(Expr),

        /// Whether the block only has its own variables, as `only` makes it in
        /// [Twig], e.g. `{% with {a: 1} only %}`.
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/tags/with.html
        pub only: bool,

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
        /// The opening clause of a Liquid [`unless`] block, which renders when
        /// its condition is false.
        ///
        /// [`unless`]: https://shopify.github.io/liquid/tags/control-flow/#unless
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

        /// Whether the tag of an `elif` clause is spelled `elseif`, as [Twig]
        /// spells it, and [Nunjucks] can, which also spells it `elif`.
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/tags/if.html
        /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#if
        pub elseif: bool,
    }

    /// An `if` block consisting of the condition, block and an optional else clause
    /// e.g. `{% if x %} ...  {% else %}  y {% endif %}`. A Liquid [`unless`]
    /// block is one too, whose first clause is [ClauseKind::Unless].
    ///
    /// [`unless`]: https://shopify.github.io/liquid/tags/control-flow/#unless
    #[derive(Debug, PartialEq, Clone)]
    #[node]
    pub struct If {
        pub clauses: Children!(IfClause),
        /// The else clause.
        pub otherwise: OptionalChild!(Body),

        /// The whitespace control of the `{% else %}` and `{% endif %}` tags.
        pub else_trim: TrimMarker,
        pub end_trim: TrimMarker,

        /// The text after the names of the `{% else %}` and `{% endif %}` tags,
        /// which Liquid ignores after an [`else`] and an [end tag], e.g. `if b`
        /// in `{% else if b %}`, see
        /// [crate::Dialect::else_and_end_tags_ignore_markup].
        ///
        /// [`else`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/if.rb#L73-L78
        /// [end tag]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/block.rb#L84
        pub else_markup: Option<ByteRange>,
        pub end_markup: Option<ByteRange>,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum LoopKind {
        For,
        /// A Liquid [`tablerow`] loop, which renders a table row per item.
        ///
        /// [`tablerow`]: https://shopify.github.io/liquid/tags/iteration/#tablerow
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

        /// The parameters of a Liquid [`for`] or [`tablerow`] loop, e.g.
        /// `limit: 2` or `cols: 3`.
        ///
        /// [`for`]: https://shopify.github.io/liquid/tags/iteration/#for-parameters
        /// [`tablerow`]: https://shopify.github.io/liquid/tags/iteration/#tablerow-parameters
        pub params: Children!(Arg),
        pub loop_body: Child!(Body),

        /// The body after `{% empty %}`, or `{% else %}` outside [Django],
        /// which renders when there is nothing to loop over.
        ///
        /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
        pub loop_empty: OptionalChild!(Body),

        /// The whitespace control of the opening, `{% empty %}` and closing
        /// tags.
        pub trim: TrimTag,
        pub empty_trim: TrimMarker,

        /// The text after the names of the `{% else %}` and closing tags,
        /// which Liquid ignores after an [`else`] and an [end tag], see
        /// [crate::Dialect::else_and_end_tags_ignore_markup].
        ///
        /// [`else`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/for.rb#L56-L59
        /// [end tag]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/block.rb#L84
        pub empty_markup: Option<ByteRange>,
        pub end_markup: Option<ByteRange>,
    }

    /// A Liquid [`case`] block, which renders the first `when` clause with a
    /// value equal to its subject, or else its `else` clause.
    ///
    /// ```liquid
    /// {% case product.type %}
    ///     {% when "shirt", "hat" %} Apparel
    ///     {% else %} Other
    /// {% endcase %}
    /// ```
    ///
    /// [`case`]: https://shopify.github.io/liquid/tags/control-flow/#casewhen
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Case {
        pub subject: Child!(Expr),

        /// Anything between `{% case %}` and the first `{% when %}`, which
        /// [Liquid] doesn't render.
        ///
        /// [Liquid]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/case.rb#L37-L48
        pub leading: Child!(Body),
        pub branches: Children!(When),
        pub otherwise: OptionalChild!(Body),

        /// The whitespace control of the `{% case %}`, `{% else %}` and
        /// `{% endcase %}` tags.
        pub trim: TrimTag,
        pub else_trim: TrimMarker,

        /// The text after the name of the `{% endcase %}` tag, which [Liquid]
        /// ignores, see [crate::Dialect::else_and_end_tags_ignore_markup]. A
        /// `case` takes [none] after its `{% else %}`.
        ///
        /// [Liquid]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/block.rb#L84
        /// [none]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/case.rb#L146-L149
        pub end_markup: Option<ByteRange>,
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

    /// The kind of callable that a [`Guard`] block checks for.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum GuardKind {
        Function,
        Filter,
        Test,
    }

    impl GuardKind {
        /// The word that the kind is written as, e.g. `function`.
        pub fn as_str(self) -> &'static str {
            match self {
                GuardKind::Function => "function",
                GuardKind::Filter => "filter",
                GuardKind::Test => "test",
            }
        }
    }

    /// A Twig [`guard`] block, which renders its body if the function, filter
    /// or test that it names exists, and else the body after its
    /// `{% else %}`, i.e.
    ///
    /// ```twig
    /// {% guard function importmap %}
    ///     {{ importmap('app') }}
    /// {% else %}
    ///     <script src="app.js"></script>
    /// {% endguard %}
    /// ```
    ///
    /// [`guard`]: https://twig.symfony.com/doc/3.x/tags/guard.html
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Guard {
        pub kind: GuardKind,

        /// The name of the callable, which spans both words of a test whose
        /// name is two, e.g. `same as`.
        pub name: Child!(Name),
        pub guard_body: Child!(Body),

        /// The body after `{% else %}`, which renders when the callable
        /// doesn't exist.
        pub otherwise: OptionalChild!(Body),

        /// The whitespace control of the opening, `{% else %}` and closing
        /// tags.
        pub trim: TrimTag,
        pub else_trim: TrimMarker,
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

        /// The [Twig] `{% block name value %}` tag, which has no end
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/tags/extends.html#block-shortcuts
        BlockShortcut(BlockShortcut),

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

        /// The [Liquid] `{% case subject %}` tag
        ///
        /// [Liquid]: https://shopify.github.io/liquid/tags/control-flow/#casewhen
        Case(Child!(Case)),

        /// The [Twig] `{% guard function name %}` tag, ending with
        /// `{% endguard %}`
        ///
        /// [Twig]: https://twig.symfony.com/doc/3.x/tags/guard.html
        Guard(Child!(Guard)),

        /// The [Liquid] `{% render "file" %}` tag
        ///
        /// [Liquid]: https://shopify.github.io/liquid/tags/template/#render
        Render(Child!(Render)),

        /// The [Liquid] `{% capture name %}` tag, ending with
        /// `{% endcapture %}`
        ///
        /// [Liquid]: https://shopify.github.io/liquid/tags/variable/#capture
        Capture(Capture),

        /// The [Liquid] `{% liquid %}` tag, with a tag on each of its lines
        ///
        /// [Liquid]: https://shopify.github.io/liquid/tags/template/#liquid
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
                Tag::Block(_) | Tag::BlockShortcut(_) => "block",
                Tag::With(_) => "with",
                Tag::MacroDef(_) => "macro",
                Tag::Include(_) => "include",
                Tag::Extends(_) => "extends",
                Tag::Import(_) => "import",
                Tag::If(_) => "if",
                Tag::For(_) => "for",
                Tag::Case(_) => "case",
                Tag::Guard(_) => "guard",
                Tag::Render(_) => "render",
                Tag::Capture(_) => "capture",
                Tag::Liquid(_) => "liquid",
                Tag::Continue(_) => "continue",
                Tag::Break(_) => "break",
                Tag::Raw(_) => "raw",
            }
        }

        pub fn is_inline(&self) -> bool {
            matches!(self, Tag::Unprocessable(_) | Tag::Assignment(_) | Tag::Continue(_) | Tag::Break(_) | Tag::Generic(_) | Tag::Render(_) | Tag::Capture(_) | Tag::Liquid(_) | Tag::BlockShortcut(_) )
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
