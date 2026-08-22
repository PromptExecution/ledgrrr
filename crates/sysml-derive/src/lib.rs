//! `#[derive(SysmlBlock)]` — walks a struct's fields via its AST (`syn`) and
//! generates a `sysml_block_def()` associated function returning the
//! equivalent SysML-v2 `block def` textual definition, computed at compile
//! time from the field list.
//!
//! Spike for the systems-modeling epic — see
//! `docs/systems-modeling-registry-rescope.md` §2a and §6 task 1. This
//! proves the "walk the Rust AST, generate SysML content via macro"
//! direction the user proposed as an alternative/complement to LinkML,
//! before either is wired into real `ArtifactKind`/`NodeType` node types.
//! Not yet validated against a real SysML-v2 grammar or parser (Part 1's
//! Tier 0 candidates) — the field-type-to-SysML-type mapping below
//! (`Vec<T>` -> `T[*]`, `Option<T>` -> `T[0..1]`) is a reasonable first
//! approximation, not a conformance-checked one.
//!
//! Only supports structs with named fields; anything else is a compile
//! error via `syn::Error::to_compile_error`, not a panic.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, parse_macro_input};

#[proc_macro_derive(SysmlBlock)]
pub fn derive_sysml_block(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let named_fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "SysmlBlock only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "SysmlBlock only supports structs")
                .to_compile_error()
                .into();
        }
    };

    let mut attribute_lines = String::new();
    for field in named_fields {
        // Safe: Fields::Named guarantees every field has an ident.
        let field_name = field.ident.as_ref().unwrap().to_string();
        let (sysml_type, multiplicity) = sysml_type_and_multiplicity(&field.ty);
        attribute_lines.push_str(&format!(
            "    attribute {field_name} : {sysml_type}{multiplicity};\n"
        ));
    }

    let block_def = format!("block def {name} {{\n{attribute_lines}}}\n");

    let expanded = quote! {
        impl #name {
            /// SysML-v2 block definition text for this type, generated at
            /// compile time by `#[derive(SysmlBlock)]` walking its fields.
            pub const fn sysml_block_def() -> &'static str {
                #block_def
            }
        }
    };

    expanded.into()
}

/// Approximate a Rust field type as a SysML-v2 attribute type + multiplicity
/// suffix: `Vec<T>` -> `(T, "[*]")`, `Option<T>` -> `(T, "[0..1]")`,
/// everything else -> `(T, "")`.
fn sysml_type_and_multiplicity(ty: &Type) -> (String, String) {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident = segment.ident.to_string();
            if ident == "Vec" || ident == "Option" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner)) = args.args.first() {
                        let suffix = if ident == "Vec" { "[*]" } else { "[0..1]" };
                        return (type_to_string(inner), suffix.to_string());
                    }
                }
            }
        }
    }
    (type_to_string(ty), String::new())
}

fn type_to_string(ty: &Type) -> String {
    quote!(#ty).to_string().replace(' ', "")
}
