use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DataStruct, DeriveInput, Fields, Type, parse_macro_input};

#[proc_macro_derive(TupleSerialize)]
pub fn tuple_serialize_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(named_fields),
            ..
        }) => &named_fields.named,
        _ => {
            return syn::Error::new_spanned(
                &input,
                "TupleSerialize can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let mut field_serializations = Vec::new();

    for field in fields {
        let field_name = match &field.ident {
            Some(ident) => ident,
            None => {
                return syn::Error::new_spanned(field, "TupleSerialize only supports named fields")
                    .to_compile_error()
                    .into();
            }
        };

        let tokens = match &field.ty {
            Type::Path(type_path) => {
                let path = &type_path.path;

                if path.is_ident("ArcCell") {
                    quote! {
                        tvmffi::stack::TupleItem::Cell(self.#field_name.clone())
                    }
                } else if path.is_ident("BigInt") {
                    quote! {
                        tvmffi::stack::TupleItem::Int(self.#field_name.clone())
                    }
                } else if path.is_ident("bool") {
                    quote! {
                        if self.#field_name {
                            tvmffi::stack::TupleItem::Int(
                                num_bigint::BigInt::from(u64::MAX)
                            )
                        } else {
                            tvmffi::stack::TupleItem::Int(
                                num_bigint::BigInt::from(0u64)
                            )
                        }
                    }
                } else if path.is_ident("i32") {
                    quote! {
                        tvmffi::stack::TupleItem::Int(
                            num_bigint::BigInt::from(self.#field_name as i64)
                        )
                    }
                } else if path.is_ident("u32") {
                    quote! {
                        tvmffi::stack::TupleItem::Int(
                            num_bigint::BigInt::from(self.#field_name as u64)
                        )
                    }
                } else if path.is_ident("IntAddr") {
                    quote! {
                        tvmffi::stack::TupleItem::Cell({
                            let mut builder = tycho_types::cell::CellBuilder::new();
                            self.#field_name
                                .store_into(
                                    &mut builder,
                                    tycho_types::cell::Cell::empty_context()
                                )
                                .expect("IntAddr::store_into failed in TupleSerialize::to_tuple");
                            let cell = builder
                                .build()
                                .expect("CellBuilder::build failed in TupleSerialize::to_tuple");
                            let boc = tycho_types::boc::Boc::encode_base64(&cell);
                            tonlib_core::cell::ArcCell::from_boc_b64(&boc)
                                .expect("ArcCell::from_boc_b64 failed in TupleSerialize::to_tuple")
                        })
                    }
                } else {
                    return syn::Error::new_spanned(
                        &field.ty,
                        "Unsupported field type for TupleSerialize",
                    )
                    .to_compile_error()
                    .into();
                }
            }
            _ => {
                return syn::Error::new_spanned(
                    &field.ty,
                    "Unsupported field type structure for TupleSerialize",
                )
                .to_compile_error()
                .into();
            }
        };

        field_serializations.push(tokens);
    }

    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn to_tuple(&self) -> tvmffi::stack::Tuple {
                use tonlib_core::tlb_types::tlb::TLB;
                use tycho_types::cell::Store;
                use tycho_types::cell::CellFamily;

                tvmffi::stack::Tuple(vec![
                    #(#field_serializations),*
                ])
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(TupleDeserialize)]
pub fn tuple_deserialize_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("TupleDeserialize only supports structs with named fields"),
        },
        _ => panic!("TupleDeserialize only supports structs"),
    };

    let field_count = fields.len();

    let field_deserializations = fields.iter().enumerate().map(|(index, field)| {
        let field_name = &field.ident;
        let field_type = &field.ty;

        match field_type {
            Type::Path(type_path) => {
                let type_ident = &type_path.path.segments.last().unwrap().ident;
                match type_ident.to_string().as_str() {
                    "ArcCell" => quote! {
                        #field_name: match tuple.0.get(#index) {
                            Some(tvmffi::stack::TupleItem::Cell(cell)) => cell.clone(),
                            _ => panic!("Expected Cell at index {}", #index),
                        }
                    },
                    "BigInt" => quote! {
                        #field_name: match tuple.0.get(#index) {
                            Some(tvmffi::stack::TupleItem::Int(val)) => val.clone(),
                            _ => panic!("Expected Int at index {}", #index),
                        }
                    },
                    "bool" => quote! {
                        #field_name: match tuple.0.get(#index) {
                            Some(tvmffi::stack::TupleItem::Int(val)) => *val == num_bigint::BigInt::from(18446744073709551615u64),
                            Some(tvmffi::stack::TupleItem::Null) => false,
                            _ => panic!("Expected Int or Null at index {}", #index),
                        }
                    },
                    "i32" => quote! {
                        #field_name: match tuple.0.get(#index) {
                            Some(tvmffi::stack::TupleItem::Int(val)) => val.to_i32().unwrap_or(0),
                            _ => panic!("Expected Int at index {}", #index),
                        }
                    },
                    "u32" => quote! {
                        #field_name: match tuple.0.get(#index) {
                            Some(tvmffi::stack::TupleItem::Int(val)) => val.to_u32().unwrap_or(0),
                            _ => panic!("Expected Int at index {}", #index),
                        }
                    },
                    "IntAddr" => quote! {
                        #field_name: match tuple.0.get(#index) {
                            Some(tvmffi::stack::TupleItem::Cell(cell)) => {
                                let boc = cell.to_boc_b64(false).unwrap();
                                let cell_parsed = tycho_types::boc::Boc::decode_base64(&boc).unwrap();
                                let mut slice = cell_parsed.as_slice().unwrap();
                                tycho_types::models::IntAddr::load_from(&mut slice).unwrap()
                            },
                            _ => panic!("Expected Cell at index {}", #index),
                        }
                    },
                    _ => panic!("Unsupported field type: {}", type_ident),
                }
            }
            _ => panic!("Unsupported field type structure"),
        }
    });

    let expanded = quote! {
        impl #name {
            pub fn from_tuple(tuple: &tvmffi::stack::Tuple) -> Self {
                if tuple.0.len() != #field_count {
                    panic!("Tuple has {} elements, expected {}", tuple.0.len(), #field_count);
                }

                Self {
                    #(#field_deserializations),*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
