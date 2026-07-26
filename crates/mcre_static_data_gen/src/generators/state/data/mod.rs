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
            pub(crate) mod block;
            pub(crate) mod is_randomly_ticking;
            pub(crate) mod light_dampening;
            pub(crate) mod light_emission;
            pub(crate) mod propagates_skylight_down;
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
