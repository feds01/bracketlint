//! A crate that hosts all of the procedural macros that `bracketlint` requires,
//! i.e. the AstWalker trait family, and some others.

use define_tree::{
    definitions::TreeDef, difference::Difference, emit::emit_tree, validate::validate_tree_def,
};
use quote::quote;
use syn::parse_macro_input;

mod define_tree;

/// Helper to return a syn error as a compiler error if one of the stages fails.
macro_rules! try_syn_err {
    ($x:expr) => {
        match $x {
            Ok(value) => value,
            Err(error) => return error.to_compile_error().into(),
        }
    };
}

/// Define a tree structure.
///
/// This macro will generate a tree structure comprised of nodes.
///
/// # Examples
///
/// The following tree definition:
///
/// ```ignore
/// define_tree! {
///     opts! {{
///         node_type_name: AstNode,
///         nodes_type_name: AstNodes,
///         visitor_trait_base_name: AstVisitor,
///         visitor_node_ref_base_type_name: AstNodeRef,
///         get_ref_from_node_function_base_name: ast_ref,
///         ref_change_body_function_base_name: with_body,
///         root_module: hash_ast::ast,
///     }}
///     #[node]
///     pub struct Foo {
///         bar: OptionalChild!(Bar),
///     }
///     #[node]
///     pub struct Bar {
///         foo: Children!(Foo),
///     }
///     #[node]
///     pub enum Baz {
///         Foo(Bar),
///         Bar(Foo),
///     }
/// }
/// ```
///
/// will generate the nodes `Foo`, `Bar` and `Baz`, with `AstNode` for each
/// child and `AstNodes` for each list of children. A tree lives in an arena, so
/// each node that holds a child, directly or through another node, takes the
/// arena's lifetime, e.g. `Foo<'a>`.
///
/// It also generates:
/// - The visitor traits `AstVisitor` and `AstVisitorMutSelf`, named after
///   `visitor_trait_base_name`, with a `visit_*` method for each node, which
///   takes `&self` or `&mut self`, and has a return type of its own. Nodes are
///   never mutable, since they are shared references into the arena.
/// - The walker modules `walk` and `walk_mut_self`, with a `walk_*` function
///   for each node, which visits its children and returns their results in a
///   structure that mirrors the node. For an enum whose every variant holds one
///   node, `walk_*_same_children` returns that node's result directly.
#[proc_macro]
pub fn define_tree(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let def = parse_macro_input!(input as TreeDef);
    try_syn_err!(validate_tree_def(&def));
    let result = try_syn_err!(emit_tree(&def));
    proc_macro::TokenStream::from(result)
}

/// Macro that takes two lists of identifiers `A = [a,..]` and `B = [b,...]` and
/// returns the set difference `A \ B`. Additionally, it applies a macro to each
/// resultant element.
///
/// Can be used as `difference!(a, b, c, d; a, b; foo; bar)` which will produce
/// `foo!(@bar c); foo!(@bar d)`.
///
/// This is used as a helper to implement all AST visitor methods other than
/// some specified ones. See [`crate::emit`] source for more details.
#[proc_macro]
pub fn difference(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let Difference { symbols, symbols_to_remove, callback_macro, callback_macro_flag } =
        parse_macro_input!(input as Difference);

    let difference_items: Vec<proc_macro2::TokenStream> = symbols
        .into_iter()
        .filter(|symbol| !symbols_to_remove.contains(symbol))
        .map(|symbol| quote! { #callback_macro!(@#callback_macro_flag #symbol); })
        .collect();

    quote! { #(#difference_items)* }.into()
}
