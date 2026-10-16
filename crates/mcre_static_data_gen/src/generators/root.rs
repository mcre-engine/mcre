use crate::{
    analyzer::Analysis,
    generators::{
        Scope, ScopeGen, Unit, UnitGen, biome::BiomeScope, block::BlockScope, fields::FieldsUnit,
        props::PropsUnit, state::StateScope,
    },
};

use mcre_data::{biome::Biome, block::Block, shape::OcclusionShape, state::BlockState};
use quote::quote;

pub struct RootScope<'a> {
    pub blocks: &'a [Block],
    pub states: &'a [BlockState],
    pub biomes: &'a [Biome],
    pub shapes: &'a [OcclusionShape],
}

impl<'a> ScopeGen<'a> for RootScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: String::new(),
            units: Box::new([
                Box::new(RootUnit),
                Box::new(PropsUnit),
                Box::new(FieldsUnit),
            ]),
            sub_scopes: Box::new([
                Box::new(BlockScope {
                    blocks: self.blocks,
                }),
                Box::new(StateScope {
                    blocks: self.blocks,
                    states: self.states,
                    shapes: self.shapes,
                }),
                Box::new(BiomeScope {
                    biomes: self.biomes,
                }),
            ]),
        }
    }
}

pub struct RootUnit;

impl UnitGen for RootUnit {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let code = quote! {
            mod biome;
            mod block;
            mod state;
            mod props;
            mod fields;

            #[allow(ambiguous_glob_reexports)]
            pub use biome::*;
            #[allow(ambiguous_glob_reexports)]
            pub use block::*;
            #[allow(ambiguous_glob_reexports)]
            pub use state::*;
            pub use props::*;
            pub use fields::*;
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
