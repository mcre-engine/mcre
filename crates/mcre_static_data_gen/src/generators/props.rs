use convert_case::ccase;
use quote::{format_ident, quote};

use crate::{
    analyzer::{Analysis, PropSchema},
    generators::{Unit, UnitGen},
};

pub struct PropsUnit;

impl UnitGen for PropsUnit {
    fn generate(&self, analysis: &Analysis) -> Unit {
        let props_idents = analysis
            .prop_schema
            .keys()
            .map(|prop_name| format_ident!("{}", ccase!(pascal, prop_name)))
            .collect::<Vec<_>>();
        let props_value_types = analysis
            .prop_schema
            .iter()
            .map(|(name, schema)| match schema {
                PropSchema::Bool => quote! { bool },
                PropSchema::Int(_, _) => quote! { u8 },
                PropSchema::Enums {
                    contains_bool,
                    enums,
                } => {
                    let ident = if *contains_bool || enums.len() > 1 {
                        format_ident!("{}PropVal", ccase!(pascal, name))
                    } else {
                        format_ident!("{}", enums[0])
                    };
                    quote! { #ident }
                }
            })
            .collect::<Vec<_>>();
        let extra_enums = analysis.prop_schema.iter().filter_map(|(name, schema)| {
            if let PropSchema::Enums {
                contains_bool,
                enums,
            } = schema
            {
                if *contains_bool || enums.len() > 1 {
                    let superenum_name = format_ident!("{}PropVal", ccase!(pascal, name));
                    let mut variants_idents = Vec::new();
                    let mut variants_literals = Vec::new();
                    let mut impls = Vec::new();
                    if *contains_bool {
                        variants_idents.push(format_ident!("True"));
                        variants_idents.push(format_ident!("False"));
                        variants_literals.push("true");
                        variants_literals.push("false");
                        impls.push(quote! {
                            impl From<bool> for #superenum_name {
                                fn from(b: bool) -> Self {
                                    if b {
                                        Self::True
                                    } else {
                                        Self::False
                                    }
                                }
                            }
                        });
                    }
                    for enum_name in enums {
                        let enum_variants = analysis.enums.get(enum_name).unwrap();
                        for enum_variant in enum_variants {
                            if !variants_literals.contains(enum_variant) {
                                variants_idents
                                    .push(format_ident!("{}", ccase!(pascal, enum_variant)));
                                variants_literals.push(enum_variant);
                            }
                        }

                        let enum_name = format_ident!("{}", enum_name);

                        let enum_variants_idents = enum_variants.iter().map(|variant| format_ident!("{}", ccase!(pascal, variant)));

                        impls.push(quote! {
                            impl From<#enum_name> for #superenum_name {
                                fn from(e: #enum_name) -> Self {
                                    match e {
                                        #( #enum_name::#enum_variants_idents => Self::#enum_variants_idents, )*
                                    }
                                }
                            }
                        });
                    }
                    Some(quote! {
                        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
                        pub enum #superenum_name {
                            #( #variants_idents, )*
                        }

                        impl #superenum_name {
                            pub fn as_str(self) -> &'static str {
                                match self {
                                    #( Self::#variants_idents => #variants_literals, )*
                                }
                            }
                        }

                        impl FromStr for #superenum_name {
                            type Err = ();

                            fn from_str(s: &str) -> Result<Self, ()> {
                                match s {
                                    #( #variants_literals => Ok(Self::#variants_idents), )*
                                    _ => Err(())
                                }
                            }
                        }

                        #( #impls )*
                    })
                } else {
                    None
                }
            } else {
                None
            }
        });
        let props_literals = analysis.prop_schema.keys().collect::<Vec<_>>();
        let props_indices = (0..(props_idents.len() as u8)).collect::<Vec<_>>();

        let prop_def = quote! {
            #( #extra_enums )*

            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
            #[repr(u8)]
            pub enum PropKey {
                #( #props_idents = #props_indices, )*
            }

            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
            #[repr(u8)]
            pub enum PropVal {
                #( #props_idents(#props_value_types) = #props_indices, )*
            }

            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            #[repr(u8)]
            pub enum PropFilter {
                #( #props_idents(Box<[#props_value_types]>) = #props_indices, )*
            }

            impl PropKey {
                pub fn as_str(self) -> &'static str {
                    match self {
                        #( Self::#props_idents => #props_literals, )*
                    }
                }
            }

            impl PropVal {
                pub fn key(self) -> PropKey {
                    match self {
                        #( Self::#props_idents(_) => PropKey::#props_idents, )*
                    }
                }

                pub fn parse_with_key(key: PropKey, s: &str) -> Option<Self> {
                    match key {
                        #( PropKey::#props_idents => Some(Self::#props_idents(#props_value_types::from_str(s).ok()?)), )*
                    }
                }
            }

            impl PropFilter {
                pub fn key(&self) -> PropKey {
                    match self {
                        #( Self::#props_idents(_) => PropKey::#props_idents, )*
                    }
                }

                pub fn test(&self, val: PropVal) -> bool {
                    match (self, val) {
                        #( (Self::#props_idents(values), PropVal::#props_idents(value)) => values.contains(&value), )*
                        _ => false,
                    }
                }

                pub fn parse_with_key(key: PropKey, s: &str) -> Option<Self> {
                    let parts = s.split("|");
                    match key {
                        #( PropKey::#props_idents => Some(Self::#props_idents(parts.map(|part| #props_value_types::from_str(part).ok()).collect::<Option<Box<_>>>()?)), )*
                    }
                }
            }

            impl FromStr for PropKey {
                type Err = ();

                fn from_str(s: &str) -> Result<Self, ()> {
                    match s {
                        #( #props_literals => Ok(Self::#props_idents), )*
                        _ => Err(())
                    }
                }
            }

            impl FromStr for PropVal {
                type Err = ();

                fn from_str(s: &str) -> Result<Self, ()> {
                    let parts: Vec<_> = s.split("=").collect();

                    if parts.len() != 2 {
                        return Err(());
                    }

                    let key = PropKey::from_str(parts[0])?;

                    Self::parse_with_key(key, parts[1]).ok_or(())
                }
            }
        };

        let code = quote! {
            use super::state::*;
            use core::str::FromStr;
            use alloc::{vec::Vec, boxed::Box};

            #prop_def
        };

        Unit {
            name: "props".to_string(),
            code,
            data: None,
        }
    }
}
