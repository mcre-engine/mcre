use convert_case::ccase;
use quote::{format_ident, quote};

use crate::{
    analyzer::{Analysis, FieldSchema},
    generators::{Unit, UnitGen},
};

pub struct FieldsUnit;

impl UnitGen for FieldsUnit {
    fn generate(&self, analysis: &Analysis) -> Unit {
        let fields_idents = analysis
            .field_schema
            .keys()
            .map(|field_name| format_ident!("{}", ccase!(pascal, field_name)))
            .collect::<Vec<_>>();
        let fields_value_types = analysis
            .field_schema
            .values()
            .map(|schema| match schema {
                FieldSchema::Bool => format_ident!("bool"),
                FieldSchema::Int(_, _) => format_ident!("u8"),
                FieldSchema::Enum(enum_name) => format_ident!("{}", enum_name),
            })
            .collect::<Vec<_>>();
        let field_to_prop_idents = analysis
            .field_schema
            .iter()
            .map(|(name, schema)| match schema {
                FieldSchema::Bool => {
                    format_ident!("{}", ccase!(pascal, name.strip_prefix("is_").unwrap()))
                }
                FieldSchema::Int(_, _) => {
                    format_ident!("{}", ccase!(pascal, name))
                }
                FieldSchema::Enum(_) => {
                    let name = if let Some(prop_name) = analysis.field_to_prop.get(name) {
                        *prop_name
                    } else {
                        name
                    };
                    format_ident!("{}", ccase!(pascal, name))
                }
            })
            .collect::<Vec<_>>();
        let fields_indices = (0..(fields_idents.len() as u8)).collect::<Vec<_>>();

        let fields_def = quote! {
            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
            #[repr(u8)]
            pub enum FieldKey {
                #( #fields_idents = #fields_indices, )*
            }

            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
            #[repr(u8)]
            pub enum FieldVal {
                #( #fields_idents(#fields_value_types) = #fields_indices, )*
            }

            impl From<FieldKey> for PropKey {
                fn from(f: FieldKey) -> Self {
                    match f {
                        #( FieldKey::#fields_idents => Self::#field_to_prop_idents, )*
                    }
                }
            }

            #[allow(clippy::useless_conversion)]
            impl From<FieldVal> for PropVal {
                fn from(f: FieldVal) -> Self {
                    match f {
                        #( FieldVal::#fields_idents(val) => Self::#field_to_prop_idents(val.into()), )*
                    }
                }
            }
        };

        let code = quote! {
            use super::{PropKey, PropVal, state::*};

            #fields_def
        };

        Unit {
            name: "fields".to_string(),
            code,
            data: None,
        }
    }
}
