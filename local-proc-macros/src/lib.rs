use proc_macro::TokenStream;
use quote::quote;
use syn::{Item, parse_macro_input};

#[proc_macro_attribute]
/// Automatically derives common traits for a "Value" type.
///
/// This procedural macro attribute applies `Debug`, `Clone`, `Copy`,
/// `PartialEq`, and `Eq` traits to the annotated item (struct or enum).
///
/// # Arguments
///
/// * `_attr` - The attributes provided to the macro (currently ignored).
/// * `item` - The Rust item (struct or enum) to which the macro is applied.
///
/// # Returns
///
/// A new `TokenStream` containing the original item with the appended `#[derive(...)]` attribute.
pub fn derive_value(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input token stream into a syntax tree node (Item)
    let item = parse_macro_input!(item as Item);

    // Append the standard "Value" trait derives to the item
    let expanded = quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #item
    };

    // Convert the expanded code back into a token stream for the compiler
    TokenStream::from(expanded)
}
