//! The module contains all of the AST definitions for the templates that
//! `bracketlint` supports parsing.

use std::{
    iter::repeat,
    ops::{Deref, DerefMut},
};

use bl_macros::define_tree;
use bl_utils::counter;
use once_cell::sync::Lazy;
use parking_lot::{RwLock, RwLockWriteGuard};
use replace_with::replace_with_or_abort;
use thin_vec::{thin_vec, ThinVec};

use crate::{
    location::{SourceId, Span},
    ByteRange,
};

counter! {
    /// This is the unique identifier for an AST node. This is used to
    /// map spans to nodes, and vice versa. [AstNodeId]s are unique and
    /// they are always increasing as a new nodes are created.
    name: AstNodeId,
    counter_name: AST_COUNTER,
    visibility: pub,
    method_visibility:,
    derives: (Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug),
}

impl AstNodeId {
    /// Create a null node id.
    pub fn null() -> Self {
        AstNodeId::from(0)
    }

    /// Get the [Span] of this [AstNodeId].
    pub fn span(&self) -> Span {
        SpanMap::span_of(*self)
    }

    /// Get the [SourceId] of this [AstNodeId].
    pub fn source(&self) -> SourceId {
        SpanMap::source_of(*self)
    }
}

/// Name for some reference within the AST to a source
/// hunk. This is essentially an interned [Span] that
/// can be used to reference a particular part of the
/// source.
pub type Hunk = AstNodeId;

impl Hunk {
    /// Create a new [Hunk] from a [Span].
    pub fn create(span: Span) -> Self {
        SpanMap::add_span(span)
    }
}

/// The [`SPAN_MAP`] is a global static that is used to store the span
/// of each AST node. This is used to avoid storing the [Span] on the
/// [`AstNode<T>`] itself in order for other data structures to be able
/// to query the [Span] of a node simply by using the [AstNodeId] of the
/// node.

static SPAN_MAP: Lazy<RwLock<Vec<Span>>> = Lazy::new(|| {
    // We initialise the map with a NULL node-id so we can use it as the default
    // for items that need a node, but don't have one.
    RwLock::new(vec![Span::new(ByteRange::new(0, 0), SourceId::default())])
});

/// A thread/job local map of [AstNodeId]s to [ByteRange]s. The [LocalSpanMap]
/// can be used by a thread to "reserve" [AstNodeId]s for nodes that will be
/// added to the global [`SPAN_MAP`] later.
///
/// ##Note: This is only used by the parser in order to reduce contention for [`SPAN_MAP`].
pub struct LocalSpanMap {
    map: Vec<(AstNodeId, ByteRange)>,
    source: SourceId,
}

impl LocalSpanMap {
    /// Create a new [LocalSpanMap].
    pub fn new(source: SourceId) -> Self {
        Self { map: vec![], source }
    }

    /// Create a new [LocalSpanMap] with a given capacity.
    pub fn with_capacity(source: SourceId, capacity: usize) -> Self {
        Self { map: Vec::with_capacity(capacity), source }
    }

    /// Add a new node to the map.
    pub fn add(&mut self, range: ByteRange) -> AstNodeId {
        let id = AstNodeId::new();
        self.map.push((id, range));
        id
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Utilities for working with the [`SPAN_MAP`].
pub struct SpanMap;

impl SpanMap {
    /// Get the span of a node by [AstNodeId].
    pub fn span_of(id: AstNodeId) -> Span {
        let span = SPAN_MAP.read()[id.to_usize()];
        debug_assert_ne!(span, Span::null(), "span of node {id:?} is null");
        span
    }

    /// Get the [SourceId] of a node by [AstNodeId].
    pub fn source_of(id: AstNodeId) -> SourceId {
        SpanMap::span_of(id).id
    }

    fn extend_map(writer: &mut RwLockWriteGuard<Vec<Span>>, id: AstNodeId) {
        let len = (id.to_usize() + 1).saturating_sub(writer.len());
        if len > 0 {
            writer.extend(repeat(Span::null()).take(len));
        }
    }

    /// Get a mutable reference to the [`SPAN_MAP`]. This is only
    /// internal to the `hash-ast` crate since it creates entries
    /// in the span map when creating new AST nodes.
    fn add_span(span: Span) -> AstNodeId {
        let mut writer = SPAN_MAP.write();

        // Create the new id, expand the map for capacity and
        // then write the span into the map.
        let id = AstNodeId::new();
        Self::extend_map(&mut writer, id);
        writer[id.to_usize()] = span;

        id
    }

    /// Update the span of a node by [AstNodeId].
    fn update_span(id: AstNodeId, span: Span) {
        SPAN_MAP.write()[id.to_usize()] = span;
    }

    /// Merge a [LocalSpanMap] into the [`SPAN_MAP`].
    pub fn add_local_map(local: LocalSpanMap) {
        // If no nodes were added, don't do anything!
        if local.map.is_empty() {
            return;
        }

        let mut writer = SPAN_MAP.write();
        let (key, _) = local.map.last().unwrap();

        // Reserve enough space in the global map to fit the local map.
        //
        // ##Note: During high loads, we're likely reserving space for all of the
        // other nodes that are to be added.
        Self::extend_map(&mut writer, *key);

        // Now we write all of the items into the map.
        for (id, range) in local.map {
            writer[id.to_usize()] = Span::new(range, local.source);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode<T> {
    pub body: Box<T>,

    /// The location of the node in the source file.
    pub id: AstNodeId,
}

impl<T> AstNode<T> {
    /// Create an [AstNodeRef] from this [AstNode].
    pub fn ast_ref(&self) -> AstNodeRef<T> {
        AstNodeRef { body: self.body.as_ref(), id: self.id }
    }

    /// Create an [AstNodeRefMut] from this [AstNode].
    pub fn ast_ref_mut(&mut self) -> AstNodeRefMut<T> {
        AstNodeRefMut { body: self.body.as_mut(), id: self.id }
    }

    /// Create an [AstNodeRef] by providing a body and copying over the
    /// [AstNodeId] that belong to this [AstNode].
    pub fn with_body<'u, U>(&self, body: &'u U) -> AstNodeRef<'u, U> {
        AstNodeRef { body, id: self.id }
    }
}

#[derive(Debug)]
pub struct AstNodeRef<'t, T> {
    /// A reference to the body of the [AstNode].
    pub body: &'t T,

    /// The [AstNodeId] of the node, representing a unique identifier within
    /// the AST, useful for performing fast comparisons of trees.
    pub id: AstNodeId,
}

impl<T> Clone for AstNodeRef<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for AstNodeRef<'_, T> {}

impl<'t, T> AstNodeRef<'t, T> {
    /// Create a new [AstNodeRef<T>].
    pub fn new(body: &'t T, id: AstNodeId) -> Self {
        AstNodeRef { body, id }
    }

    /// Get a reference to body of the [AstNodeRef].
    pub fn body(&self) -> &'t T {
        self.body
    }

    /// Utility function to copy over the [AstNodeId] from
    /// another [AstNodeRef] with a provided body.
    pub fn with_body<'u, U>(&self, body: &'u U) -> AstNodeRef<'u, U> {
        AstNodeRef { body, id: self.id }
    }

    /// Get the [Span] of this [AstNodeRef].
    pub fn span(&self) -> Span {
        SpanMap::span_of(self.id)
    }

    /// Get the [AstNodeId] of this [AstNodeRef].
    pub fn id(&self) -> AstNodeId {
        self.id
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

    /// The [AstNodeId] of the [AstNode], representing a unique identifier
    /// within the AST, useful for performing fast comparisons of trees.
    pub id: AstNodeId,
}

impl<'t, T> AstNodeRefMut<'t, T> {
    /// Create a new [AstNodeRefMut<T>].
    pub fn new(body: &'t mut T, id: AstNodeId) -> Self {
        AstNodeRefMut { body, id }
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

    /// Get the [Span] of this [AstNodeRefMut].
    pub fn span(&self) -> Span {
        SpanMap::span_of(self.id)
    }

    /// Get the [AstNodeId] of this [AstNodeRefMut].
    pub fn id(&self) -> AstNodeId {
        self.id
    }

    /// Get this node as an immutable reference
    pub fn immutable(&self) -> AstNodeRef<T> {
        AstNodeRef::new(self.body, self.id)
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
    fn node_ref(&self) -> AstNodeRef<T> {
        self.node().ast_ref()
    }

    /// Get a [AstNodeRefMut<T>].
    fn node_ref_mut(&mut self) -> AstNodeRefMut<T> {
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

    /// The id that is used to refer to the span of the [AstNodes].
    id: AstNodeId,
}

impl<T> AstNodes<T> {
    /// Create a new [AstNodes].
    pub fn empty(span: Span) -> Self {
        Self::new(thin_vec![], span)
    }

    /// Create an [AstNodes] with items and a [Span].
    pub fn new(nodes: ThinVec<AstNode<T>>, span: Span) -> Self {
        let id = SpanMap::add_span(span);
        Self { nodes, id }
    }

    /// Create a new [AstNodes] with an existing [AstNodeId].
    pub fn with_id(nodes: ThinVec<AstNode<T>>, id: AstNodeId) -> Self {
        Self { nodes, id }
    }

    /// Function to adjust the span location of [AstNodes] if it is initially
    /// incorrectly offset because there is a 'pre-conditional' token that must
    /// be parsed before parsing the nodes. This token could be something like a
    /// '<' or '(' which starts a tuple, or type bound
    pub fn set_span(&mut self, span: Span) {
        SpanMap::update_span(self.id, span);
    }

    /// Get the [AstNodeId] of this [AstNodes].
    pub fn id(&self) -> AstNodeId {
        self.id
    }

    /// Get the [Span] of this [AstNodes].
    pub fn span(&self) -> Span {
        SpanMap::span_of(self.id)
    }

    /// Insert an item into the [AstNodes] at a particular index.
    pub fn insert(&mut self, item: AstNode<T>, index: usize) {
        self.nodes.insert(index, item);
    }

    /// Merge two [AstNodes] together, this will append the nodes of the
    /// other [AstNodes] to this one, and then return the new [AstNodes].
    ///
    /// **Note** this will automatically update the [Span] of this node
    /// by extending it with the span of the other node.
    pub fn merge(&mut self, other: Self) {
        self.set_span(self.span().join(other.span()));
        self.nodes.extend(other.nodes);
    }

    /// Iterate over each child whilst wrapping it in a [AstNodeRef].
    pub fn ast_ref_iter(&self) -> impl Iterator<Item = AstNodeRef<T>> {
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

    #[derive(Hash, Debug, Clone, Copy, PartialEq, Eq)]
    #[node]
    pub enum Op {
        /// +
        Add,
        /// -
        Sub,
        /// *
        Mul,
        /// /
        Div,
        /// %
        Modulo,
    }

    /// All logic operators
    #[derive(Copy, Clone, Debug, PartialEq)]
    #[node]
    pub enum BinOp {
        /// >
        Gt,
        /// >=
        Gte,
        /// <
        Lt,
        /// <=
        Lte,
        /// ==
        Eq,
        /// !=
        NotEq,
        /// `and``
        And,
        /// `or`
        Or,
        /// `in`
        In
    }

    /// Unary operators
    #[derive(Copy, Clone, Debug, PartialEq)]
    #[node]
    pub enum UnaryOp {
        /// `not`
        Not,
        /// -
        Neg,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Lit {
        Bool(bool),
        Float(f64),
        Int(i64)
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct LitExpr {
        lit: Lit
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct ArrayExpr {
        children: Children!(Expr),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct BinExpr {
        lhs: Child!(Expr),
        rhs: Child!(Expr),
        op: Child!(BinOp),
    }

    pub type Identifier = u32;

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Name {
        data: Identifier
    }

    /// A path reference, possibly to another file, or a module, i.e. in an `include`
    /// tag:
    ///
    /// ```html
    /// {% include "file" %}
    ///            ^^^^^^
    /// ```
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Path {
        data: String
    }


    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct VarExpr {
        var: Name,
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct UnaryExpr {
        op: Child!(UnaryOp),
        expr: Child!(Expr),
    }

    /// An argument to a function, filter or a custom taf call.
    ///
    /// ##Note: It is an invariant for both `name` and `value` to
    /// be [`None`] at the same time.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Arg {
        name: OptionalChild!(Name),
        value: OptionalChild!(Expr),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct CallExpr {
        subject: Child!(Expr),
        args: Children!(Arg),
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
        subject: Child!(Expr),
        filters: Children!(CallExpr),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Expr {
        Unary(UnaryExpr),
        Lit(LitExpr),
        Array(ArrayExpr),
        Bin(BinExpr),
        Var(VarExpr),
        Call(CallExpr),
        MacroCall(MacroCallExpr),
        FilteredExpr(FilteredExpr)
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
        label: OptionalChild!(Name),
        children: Children!(Statement),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct MacroDef {
        name: Child!(Name),
        args: Children!(Name),
        inner: Children!(Statement),
    }

    /// Directly insert the contents of another file into the current template.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Include {
        path: Child!(Path),
    }

    /// Extend the current template with the contents of another file.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Extends {
        path: Child!(Path),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Import {
        path: Child!(Path),
        name: OptionalChild!(Name),
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
    pub struct Set {
        name: Child!(Name),
        value: Child!(Expr),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Super {
    }

    /// A generic tag, which can be used to represent user specific tags, and built-in tags, like
    /// `block` or `extends`.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Tag {
        name: Child!(Name),
        args: Children!(Arg),
    }

    /// A hunk of text, the [Span] of this node exactly represents the range
    /// of text that this node represents.
    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Text {
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Var {
        identifier: Child!(Name),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Body {
        contents: Children!(Statement),
    }

    #[derive(Debug, PartialEq, Clone)]
    #[node]
    pub struct IfClause {
        /// The condition of the `if` block.
        pub condition: Child!(Expr),
        /// The body of the `if-statement`
        pub if_body: Child!(Body),
    }

    /// An `if` block consisting of the condition, block and an optional else clause
    /// e.g. `{% if x %} ...  {% else %}  y {% endif %}`
    #[derive(Debug, PartialEq, Clone)]
    #[node]
    pub struct If {
        pub clauses: Children!(IfClause),
        /// The else clause.
        pub otherwise: OptionalChild!(Body),
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct For {
        key: OptionalChild!(Name),
        value: Child!(Name),
        loop_body: Child!(Body),
        loop_empty: OptionalChild!(Body),
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
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub struct Comment {
    }

    #[derive(Clone, Debug, PartialEq)]
    #[node]
    pub enum Statement {
        /// The `{{ super() }}` call in a block tag.
        Super(Super),
        /// A hunk of text.
        Text(Text),
        /// A interpolated variable `{{ var }}`
        Var(Var),
        /// A generic
        Tag(Tag),
        /// The `{% block name %}` tag, ending with `{% endblock %}`
        Block(Block),
        /// The `{% macro name() %}` tag, ending with `{% endmacro %}`
        MacroDef(MacroDef),
        /// The `{% set val = something %}` tag
        Set(Set),
        /// The `{% include "file" %}` tag
        Include(Include),
        /// The `{% extends "file" %}` tag
        Extends(Extends),
        /// The `{% import %}` tag
        Import(Import),
        /// The `{% if condition %}` tag
        If(If),
        /// The `{% for item in items %}` tag
        For(For),
        /// The `{% continue %}` tag
        Continue(Continue),
        /// The `{% break %}` tag
        Break(Break),
        /// The `{% raw %}` tag, ending with `{% endraw %}`
        Raw(Raw),
        /// Comment `{# comment #}` tag.
        Comment(Comment),
    }

    #[derive(Debug, Clone, PartialEq)]
    #[node]
    pub struct Document {
        pub children: Children!(Statement),
    }
}
