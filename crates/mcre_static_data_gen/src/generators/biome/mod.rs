use mcre_data::biome::{Biome, GrassColorModifier as DataModifier};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{
    analyzer::Analysis,
    generators::{Scope, ScopeGen, Unit, UnitGen},
};

pub struct BiomeScope<'a> {
    pub biomes: &'a [Biome],
}

impl<'a> ScopeGen<'a> for BiomeScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: "biome".to_string(),
            units: Box::new([
                Box::new(BiomeRootUnit {
                    biomes: self.biomes,
                }),
                Box::new(BiomeConstsUnit {
                    biomes: self.biomes,
                }),
            ]),
            sub_scopes: Box::new([]),
        }
    }
}

fn arms(biomes: &[Biome], f: impl Fn(&Biome) -> TokenStream) -> Vec<TokenStream> {
    biomes
        .iter()
        .map(|biome| {
            let id = biome.id;
            let value = f(biome);
            quote! { #id => #value }
        })
        .collect()
}

fn color_value(color: [u8; 3]) -> TokenStream {
    let [r, g, b] = color;
    quote! { [#r, #g, #b] }
}

fn opt_color_value(color: Option<[u8; 3]>) -> TokenStream {
    match color {
        Some(color) => {
            let color = color_value(color);
            quote! { Some(#color) }
        }
        None => quote! { None },
    }
}

fn modifier_value(modifier: DataModifier) -> TokenStream {
    let variant = format_ident!("{modifier:?}");
    quote! { GrassColorModifier::#variant }
}

pub struct BiomeRootUnit<'a> {
    biomes: &'a [Biome],
}

impl UnitGen for BiomeRootUnit<'_> {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let max = self.biomes.iter().map(|biome| biome.id).max().unwrap_or(0);
        let plains = self
            .biomes
            .iter()
            .find(|biome| biome.name == "plains")
            .or(self.biomes.first())
            .expect("at least one biome");

        let name_arms = arms(self.biomes, |biome| {
            let name = &biome.name;
            quote! { #name }
        });
        let temperature_arms = arms(self.biomes, |biome| {
            let temperature = biome.temperature;
            quote! { #temperature }
        });
        let downfall_arms = arms(self.biomes, |biome| {
            let downfall = biome.downfall;
            quote! { #downfall }
        });
        let precipitation_arms = arms(self.biomes, |biome| {
            let has_precipitation = biome.has_precipitation;
            quote! { #has_precipitation }
        });
        let water_arms = arms(self.biomes, |biome| color_value(biome.water_color));
        let foliage_arms = arms(self.biomes, |biome| {
            opt_color_value(biome.foliage_color_override)
        });
        let dry_foliage_arms = arms(self.biomes, |biome| {
            opt_color_value(biome.dry_foliage_color_override)
        });
        let grass_arms = arms(self.biomes, |biome| {
            opt_color_value(biome.grass_color_override)
        });
        let modifier_arms = arms(self.biomes, |biome| {
            modifier_value(biome.grass_color_modifier)
        });

        let plains_name = &plains.name;
        let plains_temperature = plains.temperature;
        let plains_downfall = plains.downfall;
        let plains_precipitation = plains.has_precipitation;
        let plains_water = color_value(plains.water_color);
        let plains_foliage = opt_color_value(plains.foliage_color_override);
        let plains_dry_foliage = opt_color_value(plains.dry_foliage_color_override);
        let plains_grass = opt_color_value(plains.grass_color_override);
        let plains_modifier = modifier_value(plains.grass_color_modifier);

        let code = quote! {
            mod consts;

            use mcre_core::GrassColorModifier;
            use serde::{Serialize, Deserialize};

            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
            pub struct Biome(u16);

            impl From<u16> for Biome {
                fn from(id: u16) -> Self {
                    Self(id)
                }
            }

            impl From<Biome> for u16 {
                fn from(id: Biome) -> Self {
                    id.0
                }
            }

            impl Biome {
                pub const MAX: Self = Self(#max);

                pub fn name(self) -> &'static str {
                    match self.0 {
                        #( #name_arms, )*
                        _ => #plains_name,
                    }
                }

                pub fn temperature(self) -> f32 {
                    match self.0 {
                        #( #temperature_arms, )*
                        _ => #plains_temperature,
                    }
                }

                pub fn downfall(self) -> f32 {
                    match self.0 {
                        #( #downfall_arms, )*
                        _ => #plains_downfall,
                    }
                }

                pub fn has_precipitation(self) -> bool {
                    match self.0 {
                        #( #precipitation_arms, )*
                        _ => #plains_precipitation,
                    }
                }

                pub fn water_color(self) -> [u8; 3] {
                    match self.0 {
                        #( #water_arms, )*
                        _ => #plains_water,
                    }
                }

                pub fn foliage_color_override(self) -> Option<[u8; 3]> {
                    match self.0 {
                        #( #foliage_arms, )*
                        _ => #plains_foliage,
                    }
                }

                pub fn dry_foliage_color_override(self) -> Option<[u8; 3]> {
                    match self.0 {
                        #( #dry_foliage_arms, )*
                        _ => #plains_dry_foliage,
                    }
                }

                pub fn grass_color_override(self) -> Option<[u8; 3]> {
                    match self.0 {
                        #( #grass_arms, )*
                        _ => #plains_grass,
                    }
                }

                pub fn grass_color_modifier(self) -> GrassColorModifier {
                    match self.0 {
                        #( #modifier_arms, )*
                        _ => #plains_modifier,
                    }
                }

                pub fn all() -> impl Iterator<Item = Self> {
                    (0..=u16::from(Self::MAX)).map(Self)
                }
            }
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}

pub struct BiomeConstsUnit<'a> {
    biomes: &'a [Biome],
}

impl UnitGen for BiomeConstsUnit<'_> {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let consts = self.biomes.iter().map(|biome| {
            let name = format_ident!("{}", biome.name.to_uppercase());
            let id = biome.id;
            quote! {
                pub const #name: Self = Self(#id);
            }
        });
        let code = quote! {
            use super::Biome;

            impl Biome {
                #( #consts )*
            }
        };

        Unit {
            name: "consts".to_string(),
            code,
            data: None,
        }
    }
}
