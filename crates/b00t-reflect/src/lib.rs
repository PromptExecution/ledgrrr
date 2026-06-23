//! `#[derive(HolonEmit)]` — auto-derive `emit_nodes()` from an annotated enum.
//!
//! Each variant must carry a `#[holon(...)]` attribute with these key=value fields:
//!   - `id`            — stable type identifier (required)
//!   - `label`         — display label (required)
//!   - `kind`          — node kind string (required)
//!   - `z_layer`       — optional; if present, also requires `semantic_type`
//!   - `semantic_type` — optional; if present, also requires `z_layer`
//!
//! The derived impl adds:
//! ```ignore
//! impl MyEnum {
//!     pub fn emit_nodes() -> Vec<::b00t_reflect_types::HolonNode> { ... }
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, Ident, LitStr, Token,
    parse::Parse, parse::ParseStream,
};

/// A single `key = "value"` pair inside `#[holon(...)]`.
struct HolonKv {
    key: Ident,
    value: LitStr,
}

impl Parse for HolonKv {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let value: LitStr = input.parse()?;
        Ok(HolonKv { key, value })
    }
}

/// All key=value pairs inside one `#[holon(...)]` attribute.
struct HolonAttr {
    id: Option<LitStr>,
    label: Option<LitStr>,
    kind: Option<LitStr>,
    z_layer: Option<LitStr>,
    semantic_type: Option<LitStr>,
}

impl Parse for HolonAttr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut id = None;
        let mut label = None;
        let mut kind = None;
        let mut z_layer = None;
        let mut semantic_type = None;

        while !input.is_empty() {
            let kv: HolonKv = input.parse()?;
            match kv.key.to_string().as_str() {
                "id" => id = Some(kv.value),
                "label" => label = Some(kv.value),
                "kind" => kind = Some(kv.value),
                "z_layer" => z_layer = Some(kv.value),
                "semantic_type" => semantic_type = Some(kv.value),
                other => {
                    return Err(syn::Error::new(
                        kv.key.span(),
                        format!("unknown holon key '{other}'; expected id, label, kind, z_layer, or semantic_type"),
                    ));
                }
            }
            let _ = input.parse::<Token![,]>();
        }

        Ok(HolonAttr { id, label, kind, z_layer, semantic_type })
    }
}

fn parse_holon_attr(attrs: &[syn::Attribute], variant_name: &Ident) -> syn::Result<HolonAttr> {
    for attr in attrs {
        if attr.path().is_ident("holon") {
            let parsed: HolonAttr = attr.parse_args()?;
            return Ok(parsed);
        }
    }
    Err(syn::Error::new(
        variant_name.span(),
        format!(
            "variant `{variant_name}` is missing a `#[holon(id=\"...\", label=\"...\", kind=\"...\")]` attribute"
        ),
    ))
}

/// `#[derive(HolonEmit)]` — generates `emit_nodes() -> Vec<::b00t_reflect_types::HolonNode>`.
///
/// # Example
/// ```ignore
/// #[derive(HolonEmit)]
/// enum VizDomain {
///     #[holon(id = "iso::HasVisualization", label = "HasVisualization", kind = "abstract_trait")]
///     HasVisualization,
///     #[holon(id = "pipeline::PipelineState<Ingested>", label = "PipelineState<Ingested>",
///             kind = "pipeline_state", z_layer = "Pipeline", semantic_type = "Pipeline")]
///     PipelineStateIngested,
/// }
/// ```
#[proc_macro_derive(HolonEmit, attributes(holon))]
pub fn holon_emit_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let variants = match &input.data {
        Data::Enum(e) => &e.variants,
        _ => {
            return syn::Error::new_spanned(name, "HolonEmit only applies to enums")
                .to_compile_error()
                .into();
        }
    };

    for v in variants.iter() {
        if !matches!(v.fields, Fields::Unit) {
            return syn::Error::new_spanned(
                &v.ident,
                "HolonEmit requires all variants to be unit variants (no fields)",
            )
            .to_compile_error()
            .into();
        }
    }

    let mut node_arms = Vec::new();

    for v in variants.iter() {
        let attr = match parse_holon_attr(&v.attrs, &v.ident) {
            Ok(a) => a,
            Err(e) => return e.to_compile_error().into(),
        };

        let id_str = match &attr.id {
            Some(s) => s.clone(),
            None => {
                return syn::Error::new(v.ident.span(), "holon attribute is missing required key `id`")
                    .to_compile_error()
                    .into();
            }
        };
        let label_str = match &attr.label {
            Some(s) => s.clone(),
            None => {
                return syn::Error::new(v.ident.span(), "holon attribute is missing required key `label`")
                    .to_compile_error()
                    .into();
            }
        };
        let kind_str = match &attr.kind {
            Some(s) => s.clone(),
            None => {
                return syn::Error::new(v.ident.span(), "holon attribute is missing required key `kind`")
                    .to_compile_error()
                    .into();
            }
        };

        let node_call = match (&attr.z_layer, &attr.semantic_type) {
            (Some(zl), Some(st)) => {
                quote! {
                    ::b00t_reflect_types::HolonNode {
                        id: #id_str.to_string(),
                        label: #label_str.to_string(),
                        kind: #kind_str.to_string(),
                        z_layer: Some(#zl.to_string()),
                        semantic_type: Some(#st.to_string()),
                    }
                }
            }
            (None, None) => {
                quote! {
                    ::b00t_reflect_types::HolonNode {
                        id: #id_str.to_string(),
                        label: #label_str.to_string(),
                        kind: #kind_str.to_string(),
                        z_layer: None,
                        semantic_type: None,
                    }
                }
            }
            (Some(_), None) => {
                return syn::Error::new(
                    v.ident.span(),
                    "holon attribute has `z_layer` but is missing `semantic_type`",
                )
                .to_compile_error()
                .into();
            }
            (None, Some(_)) => {
                return syn::Error::new(
                    v.ident.span(),
                    "holon attribute has `semantic_type` but is missing `z_layer`",
                )
                .to_compile_error()
                .into();
            }
        };

        node_arms.push(node_call);
    }

    let expanded = quote! {
        impl #name {
            /// Auto-derived by `#[derive(HolonEmit)]`.
            /// Returns one `HolonNode` per enum variant, in declaration order.
            pub fn emit_nodes() -> Vec<::b00t_reflect_types::HolonNode> {
                vec![
                    #(#node_arms),*
                ]
            }
        }
    };

    expanded.into()
}
