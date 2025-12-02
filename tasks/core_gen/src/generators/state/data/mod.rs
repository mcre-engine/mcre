mod fields;

use mcre_data::state::BlockState;
use quote::quote;

use crate::{
    analyzer::Analysis,
    generators::{
        Scope, ScopeGen, Unit, UnitGen,
        common::{MultiByteGen, SubByteGen},
        state::data::fields::StateFieldsDataScope,
    },
};

pub struct StateDataScope<'a> {
    pub states: &'a [BlockState],
}

impl<'a> ScopeGen<'a> for StateDataScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: "data".to_string(),
            units: Box::new([
                Box::new(StateDataRootUnit),
                Box::new(MultiByteGen {
                    name: "block_id".to_string(),
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.block_id),
                }),
                Box::new(SubByteGen {
                    name: "light_emission".to_string(),
                    is_bool: false,
                    min: 0,
                    max: 15,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.light_emission),
                }),
                Box::new(SubByteGen {
                    name: "use_shape_for_light_occlusion".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.use_shape_for_light_occlusion as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "propagates_skylight_down".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.propagates_skylight_down as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "light_block".to_string(),
                    is_bool: false,
                    min: 0,
                    max: 15,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.light_block),
                }),
                Box::new(SubByteGen {
                    name: "solid_render".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.solid_render as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "is_air".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.is_air as u8),
                }),
                Box::new(SubByteGen {
                    name: "ignited_by_lava".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.ignited_by_lava as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "can_occlude".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.can_occlude as u8),
                }),
                Box::new(SubByteGen {
                    name: "is_randomly_ticking".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.is_randomly_ticking as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "replaceable".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.replaceable as u8),
                }),
                Box::new(SubByteGen {
                    name: "spawn_terrain_particles".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.spawn_terrain_particles as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "requires_correct_tool_for_drops".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.requires_correct_tool_for_drops as u8
                    }),
                }),
                Box::new(MultiByteGen {
                    name: "destroy_speed".to_string(),
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.destroy_speed),
                }),
                Box::new(SubByteGen {
                    name: "offset_type".to_string(),
                    is_bool: false,
                    min: 0,
                    max: 2,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.offset_type as u8),
                }),
                Box::new(MultiByteGen {
                    name: "max_horizontal_offset".to_string(),
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.max_horizontal_offset
                    }),
                }),
                Box::new(MultiByteGen {
                    name: "max_vertical_offset".to_string(),
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.max_vertical_offset
                    }),
                }),
            ]),
            sub_scopes: Box::new([Box::new(StateFieldsDataScope {
                states: self.states,
            })]),
        }
    }
}

pub struct StateDataRootUnit;

impl UnitGen for StateDataRootUnit {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let code = quote! {
            pub(crate) mod block_id;
            pub(crate) mod can_occlude;
            pub(crate) mod destroy_speed;
            pub(crate) mod ignited_by_lava;
            pub(crate) mod is_air;
            pub(crate) mod is_randomly_ticking;
            pub(crate) mod light_block;
            pub(crate) mod light_emission;
            pub(crate) mod max_horizontal_offset;
            pub(crate) mod max_vertical_offset;
            pub(crate) mod offset_type;
            pub(crate) mod propagates_skylight_down;
            pub(crate) mod replaceable;
            pub(crate) mod requires_correct_tool_for_drops;
            pub(crate) mod solid_render;
            pub(crate) mod spawn_terrain_particles;
            pub(crate) mod use_shape_for_light_occlusion;

            pub(crate) mod fields;
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
