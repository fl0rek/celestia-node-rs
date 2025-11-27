//! # Builder Macro
//!
//! A derive macro for creating builder patterns with runtime validation,
//! inspired by the `bon` crate but with runtime checking instead of compile-time type states.
//!
//! ## Features
//!
//! - Automatic setter generation for struct fields
//! - Runtime validation of required fields
//! - Support for hand-written methods alongside generated ones
//! - Custom build function support
//!
//! ## Example
//!
//! ```ignore
//! use builder_macro::Builder;
//!
//! #[derive(Builder)]
//! struct ServerConfig {
//!     host: String,
//!     port: u16,
//!     timeout: Option<Duration>,
//! }
//!
//! impl ServerConfig {
//!     // Hand-written method that sets multiple fields
//!     pub fn localhost(self, port: u16) -> Self {
//!         self.host("127.0.0.1".to_string()).port(port)
//!     }
//!
//!     // Optional: custom build function
//!     // If not provided, the default build() method returns the struct directly
//! }
//!
//! // Usage
//! let config = ServerConfig::builder()
//!     .host("example.com".to_string())
//!     .port(8080)
//!     .timeout(Duration::from_secs(30))
//!     .build()
//!     .unwrap();
//! ```

extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Field, Fields, Ident, Meta, Token, Type, TypeGenerics,
    parse_macro_input,
};

/// Derive macro for creating a builder pattern with runtime validation.
///
/// This macro generates:
/// - A builder struct with `Option`-wrapped fields for runtime validation
/// - Setter methods for each field
/// - A `builder()` constructor method
/// - A `build()` method that validates required fields
///
/// ## Attributes
///
/// - `#[builder(skip)]` on a field: Don't generate a setter for this field
/// - `#[builder(default)]` on a field: Use `Default::default()` if not set (makes field optional)
/// - `#[builder(default = expr)]` on a field: Use `expr` if not set (makes field optional)
/// - `#[builder(into)]` on a field: Accept `impl Into<T>` instead of `T` in the setter
/// - Multiple attributes can be combined: `#[builder(into, default)]`
///
/// ## Field Types
///
/// - Required fields: Must be set before `build()` succeeds
/// - `Option<T>` fields: Optional, defaults to `None`. Generates two setters:
///   - `field(value: T)` - wraps in Some
///   - `maybe_field(value: Option<T>)` - sets directly
/// - Fields with `#[builder(default)]` or `#[builder(default = expr)]`: Optional, uses default if not set
///
/// ## Example
///
/// ```ignore
/// #[derive(Builder)]
/// struct Config {
///     #[builder(into)]
///     url: String,                       // Required, accepts impl Into<String>
///     #[builder(default)]
///     retry_count: u32,                  // Optional, uses Default::default() (0)
///     #[builder(default = 8080)]
///     port: u16,                         // Optional, uses 8080
///     timeout: Option<Duration>,         // Optional (None by default)
///     #[builder(skip)]
///     internal: InternalState,           // Skipped
/// }
/// ```
#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match expand_builder(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_builder(input: DeriveInput) -> syn::Result<TokenStream2> {
    let struct_name = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "Builder macro only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "Builder macro can only be applied to structs",
            ));
        }
    };

    // Parse field information
    let field_info: Vec<FieldInfo> = fields
        .iter()
        .map(FieldInfo::from_field)
        .collect::<syn::Result<_>>()?;

    // Generate builder struct name
    let builder_name = format_ident!("{}Builder", struct_name);

    // Generate builder struct fields (all wrapped in Option for runtime checking)
    let builder_fields = generate_builder_fields(&field_info);

    // Generate setter methods
    let setters = generate_setters(&field_info);

    // Generate builder() constructor on the original struct
    let builder_constructor = quote! {
        pub fn builder() -> #builder_name #ty_generics {
            #builder_name::new()
        }
    };

    // Generate new() method for the builder
    let builder_new_method = generate_builder_new_method(&field_info);

    // Generate build() method that validates and constructs the target struct
    let build_method = generate_build_method(struct_name, ty_generics.clone(), &field_info);

    let expanded = quote! {
        // Builder struct
        #vis struct #builder_name #impl_generics #where_clause {
            #(#builder_fields,)*
        }

        impl #impl_generics #builder_name #ty_generics #where_clause {
            /// Create a new builder instance
            #builder_new_method

            #(#setters)*

            #build_method
        }

        // Add builder() constructor to the original struct
        impl #impl_generics #struct_name #ty_generics #where_clause {
            #builder_constructor
        }
    };

    Ok(expanded)
}

struct FieldInfo {
    name: Ident,
    ty: Type,
    is_optional: bool,
    skip: bool,
    default: Option<DefaultValue>,
    into: bool,
}

impl FieldInfo {
    fn from_field(field: &Field) -> syn::Result<Self> {
        let name = field.ident.clone().unwrap();
        let ty = field.ty.clone();

        // Check if field is Option<T>
        let is_optional = is_option_type(&ty);

        // Parse builder attributes
        let attrs = parse_builder_attributes(&field.attrs)?;

        Ok(FieldInfo {
            name,
            ty,
            is_optional,
            skip: attrs.skip,
            default: attrs.default,
            into: attrs.into,
        })
    }

    /// Returns true if the field is required (not optional and no default)
    fn is_required(&self) -> bool {
        !self.is_optional && self.default.is_none()
    }
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

fn parse_builder_attributes(attrs: &[Attribute]) -> syn::Result<BuilderAttrs> {
    let mut result = BuilderAttrs::default();

    for attr in attrs {
        if attr.path().is_ident("builder") {
            if let Meta::List(list) = &attr.meta {
                // Parse comma-separated list of attributes
                let parser = |input: syn::parse::ParseStream| {
                    let mut attrs = Vec::new();
                    while !input.is_empty() {
                        let ident: Ident = input.parse()?;

                        // Check if this is "default = expr"
                        if ident == "default" && input.peek(Token![=]) {
                            let _: Token![=] = input.parse()?;
                            let expr: syn::Expr = input.parse()?;
                            attrs.push(BuilderAttr::DefaultExpr(expr));
                        } else {
                            attrs.push(BuilderAttr::Ident(ident));
                        }

                        // Check if there's a comma
                        if input.peek(Token![,]) {
                            let _: Token![,] = input.parse()?;
                        } else {
                            break;
                        }
                    }
                    Ok(attrs)
                };

                let parsed_attrs = syn::parse::Parser::parse2(parser, list.tokens.clone())?;

                for attr in parsed_attrs {
                    match attr {
                        BuilderAttr::Ident(ident) => {
                            let ident_str = ident.to_string();
                            match ident_str.as_str() {
                                "skip" => result.skip = true,
                                "default" => result.default = Some(DefaultValue::UseDefault),
                                "into" => result.into = true,
                                _ => return Err(syn::Error::new_spanned(
                                    ident,
                                    format!("Unknown builder attribute: {}", ident_str)
                                )),
                            }
                        }
                        BuilderAttr::DefaultExpr(expr) => {
                            result.default = Some(DefaultValue::Expr(expr));
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

enum BuilderAttr {
    Ident(Ident),
    DefaultExpr(syn::Expr),
}

#[derive(Default)]
struct BuilderAttrs {
    skip: bool,
    default: Option<DefaultValue>,
    into: bool,
}

enum DefaultValue {
    UseDefault,
    Expr(syn::Expr),
}

fn generate_builder_fields(fields: &[FieldInfo]) -> Vec<TokenStream2> {
    fields
        .iter()
        .map(|field| {
            let name = &field.name;
            let ty = &field.ty;

            // For optional fields, we store them directly as Option<T>
            // For required fields, we wrap them in Option<T> for validation
            if field.is_optional {
                // Already Option<T>, store as-is
                quote! { #name: #ty }
            } else {
                // Wrap in Option for validation
                quote! { #name: ::core::option::Option<#ty> }
            }
        })
        .collect()
}

fn generate_builder_new_method(fields: &[FieldInfo]) -> TokenStream2 {
    let field_inits = fields.iter().map(|field| {
        let name = &field.name;
        quote! { #name: ::core::option::Option::None }
    });

    quote! {
        pub fn new() -> Self {
            Self {
                #(#field_inits,)*
            }
        }
    }
}

fn generate_setters(fields: &[FieldInfo]) -> Vec<TokenStream2> {
    fields.iter().filter_map(|field| {
        if field.skip {
            return None;
        }

        let name = &field.name;
        let ty = &field.ty;

        let doc_comment = format!("Set the `{}` field", name);

        let setter = if field.is_optional {
            // For Option<T> fields, generate two methods:
            // 1. field(value: T) - wraps in Some
            // 2. maybe_field(value: Option<T>) - sets directly (including None)
            let inner_ty = extract_option_inner_type(ty);
            let maybe_name = format_ident!("maybe_{}", name);
            let maybe_doc = format!("Set the `{}` field (accepts Option)", name);

            if field.into {
                // For Option<T> with into, accept impl Into<T>
                quote! {
                    #[doc = #doc_comment]
                    pub fn #name(mut self, value: impl ::core::convert::Into<#inner_ty>) -> Self {
                        self.#name = ::core::option::Option::Some(value.into());
                        self
                    }

                    #[doc = #maybe_doc]
                    pub fn #maybe_name(mut self, value: #ty) -> Self {
                        self.#name = value;
                        self
                    }
                }
            } else {
                quote! {
                    #[doc = #doc_comment]
                    pub fn #name(mut self, value: #inner_ty) -> Self {
                        self.#name = ::core::option::Option::Some(value);
                        self
                    }

                    #[doc = #maybe_doc]
                    pub fn #maybe_name(mut self, value: #ty) -> Self {
                        self.#name = value;
                        self
                    }
                }
            }
        } else {
            // For non-optional fields
            if field.into {
                // Accept impl Into<T>
                quote! {
                    #[doc = #doc_comment]
                    pub fn #name(mut self, value: impl ::core::convert::Into<#ty>) -> Self {
                        self.#name = ::core::option::Option::Some(value.into());
                        self
                    }
                }
            } else {
                // Accept the type directly
                quote! {
                    #[doc = #doc_comment]
                    pub fn #name(mut self, value: #ty) -> Self {
                        self.#name = ::core::option::Option::Some(value);
                        self
                    }
                }
            }
        };

        Some(setter)
    }).collect()
}

fn extract_option_inner_type(ty: &Type) -> TokenStream2 {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return quote! { #inner_ty };
                    }
                }
            }
        }
    }
    quote! { #ty }
}

fn generate_build_method(
    struct_name: &Ident,
    struct_generics: TypeGenerics,
    fields: &[FieldInfo],
) -> TokenStream2 {
    // Generate validation checks for required fields (no default and not optional)
    let validations = fields.iter().filter_map(|field| {
        if field.is_required() {
            let name = &field.name;
            let name_str = name.to_string();
            Some(quote! {
                if self.#name.is_none() {
                    return ::core::result::Result::Err(
                        ::std::format!("Required field '{}' not set", #name_str)
                    );
                }
            })
        } else {
            None
        }
    });

    // Generate field constructions for the final struct
    let field_constructions = fields.iter().map(|field| {
        let name = &field.name;
        if field.is_optional {
            // For optional fields, use as-is (already Option<T>)
            quote! { #name: self.#name }
        } else if let Some(default) = &field.default {
            // For fields with default
            match default {
                DefaultValue::UseDefault => {
                    // Use Default::default()
                    quote! {
                        #name: self.#name.unwrap_or_default()
                    }
                }
                DefaultValue::Expr(expr) => {
                    // Use provided expression
                    quote! {
                        #name: self.#name.unwrap_or_else(|| #expr)
                    }
                }
            }
        } else {
            // For required fields without defaults, unwrap the Option
            quote! {
                #name: self.#name.expect("field was validated")
            }
        }
    });

    quote! {
        /// Build the final struct. Returns an error if any required fields are not set.
        pub fn build(self) -> ::core::result::Result<#struct_name #struct_generics, ::std::string::String> {
            #(#validations)*

            ::core::result::Result::Ok(#struct_name {
                #(#field_constructions,)*
            })
        }
    }
}
