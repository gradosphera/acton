use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DataStruct, DeriveInput, Fields, parse_macro_input};

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

    let field_serializations = fields.iter().map(|field| {
        let field_name = &field.ident;
        quote! {
            tvmffi::to_stack::ToStack::to_item(&self.#field_name)?
        }
    });

    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn to_tuple(&self, _options: tvmffi::to_stack::SerializationOptions) -> Result<tvmffi::stack::Tuple, tvmffi::to_stack::SerializationError> {
                Ok(tvmffi::stack::Tuple(vec![
                    #(#field_serializations),*
                ]))
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

        quote! {
            #field_name: {
                let item = tuple.0.get(#index).cloned().unwrap_or(tvmffi::stack::TupleItem::Null);
                <#field_type as tvmffi::from_stack::FromStack>::from_item(item)?
            }
        }
    });

    let expanded = quote! {
        impl #name {
            pub fn from_tuple(tuple: &tvmffi::stack::Tuple, options: tvmffi::from_stack::DeserializationOptions) -> Result<Self, tvmffi::from_stack::ArgError> {
                let len = tuple.0.len();
                if options.allow_extra && options.allow_missing {
                    // no check
                } else if options.allow_extra {
                    if len < #field_count {
                        return Err(tvmffi::from_stack::ArgError::MissingElements { expected: #field_count, actual: len });
                    }
                } else if options.allow_missing {
                    if len > #field_count {
                        return Err(tvmffi::from_stack::ArgError::ExtraElements { expected: #field_count, actual: len });
                    }
                } else {
                    if len != #field_count {
                        if len < #field_count {
                            return Err(tvmffi::from_stack::ArgError::MissingElements { expected: #field_count, actual: len });
                        } else {
                            return Err(tvmffi::from_stack::ArgError::ExtraElements { expected: #field_count, actual: len });
                        }
                    }
                }

                Ok(Self {
                    #(#field_deserializations),*
                })
            }
        }
    };

    TokenStream::from(expanded)
}
