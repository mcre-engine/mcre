use mcre_data::state::BlockState;
use quote::{format_ident, quote};

use crate::{
    analyzer::{Analysis, FieldSchema},
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
            units: Box::new([Box::new(StateRootUnit), Box::new(EnumsGenerator)]),
            sub_scopes: Box::new([Box::new(StateDataScope {
                states: self.states,
            })]),
        }
    }
}

pub struct StateRootUnit;

impl UnitGen for StateRootUnit {
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
        let code = quote! {
            mod data;
            mod enums;

            use crate::{BlockId, OffsetType};
            use enums::*;

            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
            pub struct StateId(u16);

            impl From<u16> for StateId {
                fn from(id: u16) -> Self {
                    Self(id)
                }
            }

            impl From<StateId> for u16 {
                fn from(id: StateId) -> Self {
                    id.0
                }
            }

            impl StateId {
                pub fn block_id(self) -> BlockId {
                    data::block_id::get(self.0).into()
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

                #( #fields )*
            }
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
