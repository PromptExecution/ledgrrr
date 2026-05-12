use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Item, LitStr};

/// Emit a compile-time trait-bound assertion that the annotated type implements
/// `crate::attest::Attested`.
///
/// Usage (within `ledger-core`):
/// ```ignore
/// #[attested("my_invariant")]
/// pub struct MyType { ... }
/// ```
///
/// If `MyType` does not implement `Attested`, the compiler emits a
/// trait-not-satisfied error at the call site.
///
/// # Note on crate paths
/// The generated assertion references `crate::attest::Attested`. This macro is
/// intended for use within `ledger-core`. External crates using this attribute
/// must have `pub mod attest` with the `Attested` trait re-exported as
/// `crate::attest::Attested` at their crate root.
#[proc_macro_attribute]
pub fn attested(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _invariant_lit = parse_macro_input!(attr as LitStr);
    let item = parse_macro_input!(item as Item);

    let type_name = match &item {
        Item::Struct(s) => s.ident.clone(),
        Item::Enum(e) => e.ident.clone(),
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[attested] can only be applied to structs and enums",
            )
            .to_compile_error()
            .into();
        }
    };

    let expanded = quote! {
        #item

        const _: fn() = || {
            fn _assert_attested<T: crate::attest::Attested>() {}
            _assert_attested::<#type_name>();
        };
    };

    expanded.into()
}
