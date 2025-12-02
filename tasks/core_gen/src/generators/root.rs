use crate::{
    analyzer::Analysis,
    generators::{
        Scope, ScopeGen, Unit, UnitGen, block::BlockScope, fields::FieldsUnit, props::PropsUnit,
        state::StateScope,
    },
};

use mcre_data::{block::Block, state::BlockState};
use quote::quote;

pub struct RootScope<'a> {
    pub blocks: &'a [Block],
    pub states: &'a [BlockState],
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
                    states: self.states,
                }),
            ]),
        }
    }
}

pub struct RootUnit;

impl UnitGen for RootUnit {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let code = quote! {
            mod block;
            mod state;
            mod props;
            mod fields;

            pub use block::*;
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
