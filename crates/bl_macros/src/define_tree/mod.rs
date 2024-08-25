//! This crate defines a macro [`define_tree!`] that can be used to define a
//! tree structure comprised of nodes. The macro will generate visitor and
//! walker implementations for the given tree.

pub(crate) mod definitions;
pub(crate) mod difference;
pub(crate) mod emit;
pub(crate) mod parse;
pub(crate) mod validate;
