use crate::{
    analyzer::Analysis,
    generators::{Scope, ScopeGen, Unit, UnitGen, block::BlockScope, state::StateScope},
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
            units: Box::new([Box::new(RootUnit)]),
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

            pub use block::*;
            pub use state::*;
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
