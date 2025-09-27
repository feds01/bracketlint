//! Contains a helper macro to get the difference of two identifier lists.

use syn::{Ident, Token, parse::Parse};

/// Represents the difference of two lists of symbols.
pub(crate) struct Difference {
    pub symbols: Vec<Ident>,
    pub symbols_to_remove: Vec<Ident>,
    pub callback_macro: Ident,
    pub callback_macro_flag: Ident,
}

/// Parse a `Difference` from the following format:
///
/// `$($node:ident),*; $($node_to_remove:ident),*; $callback_macro:ident;
/// $callback_macro_flag:ident`
impl Parse for Difference {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Parse first section: comma-separated identifiers until semicolon
        let mut symbols = Vec::new();
        loop {
            symbols.push(input.parse::<Ident>()?);
            if input.peek(Token![;]) {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        // Parse semicolon
        input.parse::<Token![;]>()?;

        // Parse second section: comma-separated identifiers until semicolon
        let mut symbols_to_remove = Vec::new();
        loop {
            if input.peek(Token![;]) {
                break; // Empty list case
            }
            symbols_to_remove.push(input.parse::<Ident>()?);
            if input.peek(Token![;]) {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        // Parse semicolon
        input.parse::<Token![;]>()?;

        // Parse third section: single callback macro identifier
        let callback_macro: Ident = input.parse()?;

        // Parse semicolon
        input.parse::<Token![;]>()?;

        // Parse fourth section: single callback macro flag identifier
        let callback_macro_flag: Ident = input.parse()?;

        Ok(Self { symbols, symbols_to_remove, callback_macro, callback_macro_flag })
    }
}
