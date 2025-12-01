use crate::{
    analyzer::Analysis,
    generators::{
        Scope, ScopeGen, Unit, UnitGen,
        common::{MultiByteGen, StringGen},
    },
};

use mcre_data::block::Block;
use quote::quote;

pub struct BlockDataScope<'a> {
    pub blocks: &'a [Block],
}

impl<'a> ScopeGen<'a> for BlockDataScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: "data".to_string(),
            units: Box::new([
                Box::new(BlockDataRootUnit),
                Box::new(StringGen {
                    name: "name".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block| &block.name),
                }),
                Box::new(StringGen {
                    name: "display_name".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block| &block.display_name),
                }),
                Box::new(MultiByteGen {
                    name: "default_state_id".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block| block.default_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "min_state_id".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block| block.min_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "max_state_id".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block| block.max_state_id),
                }),
            ]),
            sub_scopes: Box::new([]),
        }
    }
}

pub struct BlockDataRootUnit;

impl UnitGen for BlockDataRootUnit {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let code = quote! {
            pub(crate) mod default_state_id;
            pub(crate) mod display_name;
            pub(crate) mod max_state_id;
            pub(crate) mod min_state_id;
            pub(crate) mod name;
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
