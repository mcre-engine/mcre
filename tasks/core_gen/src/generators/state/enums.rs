use crate::{
    analyzer::Analysis,
    generators::{Unit, UnitGen},
};

use convert_case::ccase;
use quote::{format_ident, quote};

pub struct EnumsGenerator;

impl UnitGen for EnumsGenerator {
    fn generate(&self, analysis: &Analysis) -> Unit {
        let enums = analysis
            .enums
            .iter()
            .filter(|(enum_name, _)| !analysis.foreign_enums.contains_key(**enum_name))
            .map(|(enum_name, variants)| {
                let enum_name = format_ident!("{}", enum_name);

                let variants_idents = variants
                    .iter()
                    .map(|variant| format_ident!("{}", ccase!(pascal, variant)))
                    .collect::<Vec<_>>();
                let variants_indices = 0..(variants.len() as u8);

                quote! {
                    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
                    #[repr(u8)]
                    pub enum #enum_name {
                        #( #variants_idents = #variants_indices, )*
                    }

                    impl #enum_name {
                        pub fn as_str(self) -> &'static str {
                            match self {
                                #( Self::#variants_idents => #variants, )*
                            }
                        }
                    }

                    impl FromStr for #enum_name {
                        type Err = ();

                        fn from_str(s: &str) -> Result<Self, ()> {
                            match s {
                                #( #variants => Ok(Self::#variants_idents), )*
                                _ => Err(()),
                            }
                        }
                    }
                }
            });

        let imports = analysis
            .foreign_enums
            .keys()
            .map(|enum_name| format_ident!("{}", enum_name));

        let code = quote! {
            pub use crate::{#( #imports ),*};
            use core::str::FromStr;

            #( #enums )*
        };

        Unit {
            name: "enums".to_string(),
            code,
            data: None,
        }
    }
}
