use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Lit};

enum Traits {
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
}

#[proc_macro_derive(DeriveByKey, attributes(derive_by_key))]
pub fn derive_by_key(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    // Parse the `derive_by_key` attribute
    let attrs = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("derive_by_key"))
        .expect("Expected `#[derive_by_key]` attribute");

    let mut key_fn = None;
    let mut traits = vec![];

    attrs
        // .parse_args_with(AttributeArgs::parse)
        .parse_nested_meta(|meta| {
            if meta.path.is_ident("key") {
                let x: Lit = meta.value()?.parse()?;
                match x {
                    Lit::Str(s) => key_fn = Some(format_ident!("{}", s.value())),
                    _ => return Err(meta.error("Invalid argument; not astring literal")),
                }
            } else if meta.path.is_ident("PartialEq") {
                traits.push(Traits::PartialEq);
            } else if meta.path.is_ident("Eq") {
                traits.push(Traits::Eq);
            } else if meta.path.is_ident("PartialOrd") {
                traits.push(Traits::PartialOrd);
            } else if meta.path.is_ident("Ord") {
                traits.push(Traits::Ord);
            } else if meta.path.is_ident("Hash") {
                traits.push(Traits::Hash);
            } else {
                return Err(meta.error("unsupported property"));
            }
            Ok(())
        })
        .expect("Invalid arguments for `#[derive_by_key]`");

    let key_fn = key_fn.expect("Expected `key` argument");

    let ord_implemented = traits.iter().any(|x| matches!(x, Traits::Ord));

    let impls = traits.into_iter().map(|trt| {
        let partial_something = |trait_name, method, return_type| {
            quote! {
                impl #impl_generics ::std::cmp::#trait_name
                for #struct_name #type_generics #where_clause {
                    fn #method(&self, other: &Self) -> #return_type {
                        ::std::cmp::#trait_name::#method(&self.#key_fn(), &other.#key_fn())
                    }
                }
            }
        };

        match trt {
            Traits::PartialEq => {
                partial_something(quote! { PartialEq }, quote! { eq }, quote! { bool })
            }
            Traits::Eq => {
                quote! {
                    impl #impl_generics ::std::cmp::Eq
                    for #struct_name #type_generics #where_clause {
                    }
                }
            }
            Traits::PartialOrd => {
                if ord_implemented {
                    quote! {
                        impl #impl_generics ::std::cmp::PartialOrd
                        for #struct_name #type_generics #where_clause {
                            fn partial_cmp(&self, other: &Self)
                            -> ::std::option::Option<::std::cmp::Ordering> {
                                Some(self.cmp(other))
                            }
                        }
                    }
                } else {
                    partial_something(
                        quote! { PartialOrd },
                        quote! { partial_cmp },
                        quote! { ::std::option::Option<::std::cmp::Ordering> },
                    )
                }
            }
            Traits::Ord => {
                quote! {
                    impl #impl_generics ::std::cmp::Ord
                    for #struct_name #type_generics #where_clause {
                        fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                            self.#key_fn().cmp(&other.#key_fn())
                        }
                    }
                }
            }
            Traits::Hash => {
                quote! {
                    impl #impl_generics ::std::hash::Hash
                    for #struct_name #type_generics #where_clause {
                        fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
                            self.#key_fn().hash(state)
                        }
                    }
                }
            }
        }
    });

    let expanded = quote! {
        #(#impls)*
    };

    TokenStream::from(expanded)
}
