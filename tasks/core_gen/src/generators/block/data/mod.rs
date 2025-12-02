use crate::{
    analyzer::{Analysis, FieldSchema},
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
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.default_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "min_state_id".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.min_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "max_state_id".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.max_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "fields_present".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, analysis: &Analysis<'_>| {
                        let mut fields_present = 0;

                        for (i, (field, schema)) in analysis.field_schema.iter().enumerate() {
                            let present = match schema {
                                FieldSchema::Bool => {
                                    let name = field.strip_prefix("is_").unwrap();
                                    block.states.iter().any(|state| state.name == name)
                                }
                                FieldSchema::Int(_, _) => {
                                    block.states.iter().any(|state| &state.name == field)
                                }
                                FieldSchema::Enum(_) => block.states.iter().any(|state| {
                                    analysis
                                        .prop_to_field
                                        .get(&(block.name.as_str(), state.name.as_str()))
                                        .is_some_and(|field1| field1 == field)
                                }),
                            };

                            if present {
                                let flag = 1u128 << i;

                                fields_present |= flag;
                            }
                        }

                        fields_present
                    }),
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
            pub(crate) mod fields_present;
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
