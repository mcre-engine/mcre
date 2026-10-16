mod fields;
mod shapes;

use mcre_data::{block::Block, shape::OcclusionShape, state::BlockState};
use quote::quote;

use crate::{
    analyzer::Analysis,
    generators::{
        Scope, ScopeGen, Unit, UnitGen,
        common::{MultiByteGen, StringGen, SubByteGen},
        state::data::{fields::StateFieldsDataScope, shapes::ShapeTableUnit},
    },
};

pub struct StateDataScope<'a> {
    pub states: &'a [BlockState],
    pub blocks: &'a [Block],
    pub shapes: &'a [OcclusionShape],
}

impl<'a> ScopeGen<'a> for StateDataScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: "data".to_string(),
            units: Box::new([
                Box::new(StateDataRootUnit),
                Box::new(MultiByteGen {
                    name: "block".to_string(),
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
                    name: "light_dampening".to_string(),
                    is_bool: false,
                    min: 0,
                    max: 15,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.light_dampening),
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
                    name: "self_skip".to_string(),
                    is_bool: false,
                    min: 0,
                    max: 63,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.self_skip),
                }),
                Box::new(SubByteGen {
                    name: "collision_full".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.collision_full as u8
                    }),
                }),
                Box::new(StringGen {
                    name: "fluid".to_string(),
                    list: self.states,
                    mapping_fn: Box::new(|state| state.fluid.as_deref().unwrap_or("")),
                }),
                Box::new(MultiByteGen {
                    name: "occlusion_shape".to_string(),
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| state.occlusion_shape),
                }),
                Box::new(SubByteGen {
                    name: "emissive_rendering".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.states,
                    mapping_fn: Box::new(|state, _analysis: &Analysis<'_>| {
                        state.emissive_rendering as u8
                    }),
                }),
                Box::new(ShapeTableUnit {
                    shapes: self.shapes,
                }),
            ]),
            sub_scopes: Box::new([Box::new(StateFieldsDataScope {
                states: self.states,
                blocks: self.blocks,
            })]),
        }
    }
}

pub struct StateDataRootUnit;

impl UnitGen for StateDataRootUnit {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let code = quote! {
            pub(crate) mod block;
            pub(crate) mod collision_full;
            pub(crate) mod emissive_rendering;
            pub(crate) mod fluid;
            pub(crate) mod is_randomly_ticking;
            pub(crate) mod light_dampening;
            pub(crate) mod light_emission;
            pub(crate) mod occlusion_shape;
            pub(crate) mod propagates_skylight_down;
            pub(crate) mod self_skip;
            pub(crate) mod shapes;
            pub(crate) mod solid_render;
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
