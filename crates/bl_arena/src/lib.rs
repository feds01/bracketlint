//! The arena that a parsed tree is allocated in. Each node and list of a tree
//! is allocated in one arena, and the whole tree is freed at once when the
//! arena is reset or dropped, rather than one node at a time.
//!
//! The arena is a [bumpalo] arena, which this crate keeps behind its own API,
//! so that the rest of bracketlint doesn't depend on it.

/// An arena that values are allocated in, which frees them all at once when
/// it is reset or dropped. A value in the arena is never dropped, so it must
/// not own anything that needs dropping, e.g. a `String`.
#[derive(Default)]
pub struct Arena {
    bump: bumpalo::Bump,
}

impl Arena {
    /// Create an empty arena.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate `value` in the arena.
    pub fn alloc<T>(&self, value: T) -> &mut T {
        self.bump.alloc(value)
    }

    /// Create an empty list, which grows in the arena as it is pushed to.
    pub fn vec<T>(&self) -> ArenaVec<'_, T> {
        ArenaVec { vec: bumpalo::collections::Vec::new_in(&self.bump) }
    }

    /// Free everything in the arena at once, and keep its memory to allocate
    /// in again.
    pub fn reset(&mut self) {
        self.bump.reset();
    }
}

/// A list that is built in an [Arena], and then kept in it as a slice.
pub struct ArenaVec<'a, T> {
    vec: bumpalo::collections::Vec<'a, T>,
}

impl<'a, T> ArenaVec<'a, T> {
    /// Add `value` to the end of the list.
    pub fn push(&mut self, value: T) {
        self.vec.push(value);
    }

    /// Whether the list has no items.
    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    /// The items of the list, as a slice in the arena.
    pub fn into_slice(self) -> &'a [T] {
        self.vec.into_bump_slice()
    }
}
