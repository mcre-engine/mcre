use convert_case::ccase;
use mcre_data::state::BlockState;
use quote::{format_ident, quote};

use crate::{
    analyzer::{Analysis, FieldSchema, PropSchema},
    generators::{
        Scope, ScopeGen, Unit, UnitGen,
        state::{data::StateDataScope, enums::EnumsGenerator},
    },
};

pub mod data;
pub mod enums;

pub struct StateScope<'a> {
    pub states: &'a [BlockState],
}

impl<'a> ScopeGen<'a> for StateScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: "state".to_string(),
            units: Box::new([
                Box::new(StateRootUnit {
                    states: self.states,
                }),
                Box::new(EnumsGenerator),
            ]),
            sub_scopes: Box::new([Box::new(StateDataScope {
                states: self.states,
            })]),
        }
    }
}

pub struct StateRootUnit<'a> {
    states: &'a [BlockState],
}

impl UnitGen for StateRootUnit<'_> {
    fn generate(&self, analysis: &Analysis) -> Unit {
        let fields = analysis.field_schema.iter().map(|(field_name, schema)| {
            let field_name = format_ident!("{}", field_name);
            match schema {
                FieldSchema::Bool => quote! {
                    pub fn #field_name(self) -> bool {
                        data::fields::#field_name::get(self.0)
                    }
                },
                FieldSchema::Int(_, _) => quote! {
                    pub fn #field_name(self) -> u8 {
                        data::fields::#field_name::get(self.0)
                    }
                },
                FieldSchema::Enum(enum_name) => {
                    let enum_name = format_ident!("{}", enum_name);
                    quote! {
                        pub fn #field_name(self) -> #enum_name {
                            unsafe {
                                core::mem::transmute::<u8, #enum_name>(data::fields::#field_name::get(self.0))
                            }
                        }
                    }
                }
            }
        });
        let fields_pascal_idents = analysis
            .field_schema
            .keys()
            .map(|name| format_ident!("{}", ccase!(pascal, name)))
            .collect::<Vec<_>>();
        let fields_snake_idents = analysis
            .field_schema
            .keys()
            .map(|name| format_ident!("{}", name))
            .collect::<Vec<_>>();
        let get_prop_matches =
            analysis
                .prop_schema
                .iter()
                .map(|(prop_name, prop_schema)| match prop_schema {
                    PropSchema::Bool => {
                        let prop_variant = format_ident!("{}", ccase!(pascal, prop_name));
                        let field_variant = format_ident!("Is{}", ccase!(pascal, prop_name));
                        let method = format_ident!("is_{}", prop_name);
                        quote! {
                            PropKey::#prop_variant => self.block().is_field_present(FieldKey::#field_variant).then_some(PropVal::#prop_variant(self.#method()))
                        }
                    }
                    PropSchema::Int(_, _) => {
                        let variant = format_ident!("{}", ccase!(pascal, prop_name));
                        let method = format_ident!("{}", prop_name);
                        quote! {
                            PropKey::#variant => self.block().is_field_present(FieldKey::#variant).then_some(PropVal::#variant(self.#method()))
                        }
                    }
                    PropSchema::Enums { contains_bool, enums } => {
                        if *contains_bool || enums.len() > 1 {
                            let mut fields_variants = Vec::new();
                            let mut fields_methods = Vec::new();

                            if *contains_bool {
                                fields_variants.push(format_ident!("Is{}", ccase!(pascal, prop_name)));
                                fields_methods.push(format_ident!("is_{}", prop_name));
                            }

                            for (field, prop) in &analysis.field_to_prop {
                                if prop == prop_name {
                                    fields_variants.push(format_ident!("{}", ccase!(pascal, field)));
                                    fields_methods.push(format_ident!("{}", field));
                                }
                            }

                            let prop_variant = format_ident!("{}", ccase!(pascal, prop_name));

                            quote! {
                                PropKey::#prop_variant => #(if self.block().is_field_present(FieldKey::#fields_variants) {
                                    Some(PropVal::#prop_variant(self.#fields_methods().into()))
                                } else)* {
                                    None
                                }
                            }
                        } else {
                            let variant = format_ident!("{}", ccase!(pascal, prop_name));
                            let method = format_ident!("{}", prop_name);
                            quote! {
                                PropKey::#variant => self.block().is_field_present(FieldKey::#variant).then_some(PropVal::#variant(self.#method()))
                            }
                        }
                    }
                });
        let max = self.states.last().unwrap().id;
        let code = quote! {
            mod data;
            mod enums;

            use crate::{Block, OffsetType, FieldKey, FieldVal, PropKey, PropVal};
            use serde::{Serialize, Deserialize};
            pub use enums::*;

            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
            pub struct BlockState(u16);

            impl From<u16> for BlockState {
                fn from(id: u16) -> Self {
                    Self(id)
                }
            }

            impl From<BlockState> for u16 {
                fn from(id: BlockState) -> Self {
                    id.0
                }
            }

            impl BlockState {
                pub const MAX: Self = Self(#max);

                pub fn block(self) -> Block {
                    data::block::get(self.0).into()
                }

                pub fn light_emission(self) -> u8 {
                    data::light_emission::get(self.0)
                }

                pub fn use_shape_for_light_occlusion(self) -> bool {
                    data::use_shape_for_light_occlusion::get(self.0)
                }

                pub fn propagates_skylight_down(self) -> bool {
                    data::propagates_skylight_down::get(self.0)
                }

                pub fn light_block(self) -> u8 {
                    data::light_block::get(self.0)
                }

                pub fn solid_render(self) -> bool {
                    data::solid_render::get(self.0)
                }

                pub fn is_air(self) -> bool {
                    data::is_air::get(self.0)
                }

                pub fn ignited_by_lava(self) -> bool {
                    data::ignited_by_lava::get(self.0)
                }

                pub fn can_occlude(self) -> bool {
                    data::can_occlude::get(self.0)
                }

                pub fn is_randomly_ticking(self) -> bool {
                    data::is_randomly_ticking::get(self.0)
                }

                pub fn replaceable(self) -> bool {
                    data::replaceable::get(self.0)
                }

                pub fn spawn_terrain_particles(self) -> bool {
                    data::spawn_terrain_particles::get(self.0)
                }

                pub fn requires_correct_tool_for_drops(self) -> bool {
                    data::requires_correct_tool_for_drops::get(self.0)
                }

                pub fn destroy_speed(self) -> f32 {
                    data::destroy_speed::get(self.0)
                }

                pub fn offset_type(self) -> OffsetType {
                    unsafe { core::mem::transmute::<u8, OffsetType>(data::offset_type::get(self.0)) }
                }

                pub fn max_horizontal_offset(self) -> f32 {
                    data::max_horizontal_offset::get(self.0)
                }

                pub fn max_vertical_offset(self) -> f32 {
                    data::max_vertical_offset::get(self.0)
                }

                pub fn get_field(self, field: FieldKey) -> Option<FieldVal> {
                    if !self.block().is_field_present(field) {
                        return None;
                    }
                    match field {
                        #(FieldKey::#fields_pascal_idents => Some(FieldVal::#fields_pascal_idents(self.#fields_snake_idents())),)*
                    }
                }

                pub fn get_prop(self, prop: PropKey) -> Option<PropVal> {
                    match prop {
                        #( #get_prop_matches, )*
                    }
                }

                pub fn all() -> impl Iterator<Item = Self> {
                    BlockStateIter::new(BlockState(0), Self::MAX)
                }

                #( #fields )*
            }

            pub struct BlockStateIter {
                current: u16,
                end: u16,
            }

            impl BlockStateIter {
                // inclusive range
                pub fn new(start: BlockState, end: BlockState) -> Self {
                    Self {
                        current: start.0,
                        end: end.0,
                    }
                }
            }

            impl Iterator for BlockStateIter {
                type Item = BlockState;

                fn next(&mut self) -> Option<Self::Item> {
                    if self.current > self.end {
                        None
                    } else {
                        let id = self.current;
                        self.current += 1;
                        Some(BlockState(id))
                    }
                }

                fn size_hint(&self) -> (usize, Option<usize>) {
                    let remaining = (self.end - self.current) as usize;
                    (remaining, Some(remaining))
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
