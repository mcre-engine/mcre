mod data;

use crate::{
    analyzer::Analysis,
    generators::{Scope, ScopeGen, Unit, UnitGen, block::data::BlockDataScope},
};

use mcre_data::block::Block;
use quote::quote;

pub struct BlockScope<'a> {
    pub blocks: &'a [Block],
}

impl<'a> ScopeGen<'a> for BlockScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: "block".to_string(),
            units: Box::new([Box::new(BlockRootUnit {
                blocks: self.blocks,
            })]),
            sub_scopes: Box::new([Box::new(BlockDataScope {
                blocks: self.blocks,
            })]),
        }
    }
}

pub struct BlockRootUnit<'a> {
    blocks: &'a [Block],
}

impl UnitGen for BlockRootUnit<'_> {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let max = self.blocks.last().unwrap().id;
        let code = quote! {
            mod data;

            use crate::{StateId, FieldKey};

            #[derive(Debug, Copy, Clone, Hash)]
            pub struct BlockId(u16);

            impl From<u16> for BlockId {
                fn from(id: u16) -> Self {
                    Self(id)
                }
            }

            impl From<BlockId> for u16 {
                fn from(id: BlockId) -> Self {
                    id.0
                }
            }

            impl BlockId {
                pub const MAX: Self = Self(#max);

                pub fn name(self) -> &'static str {
                    data::name::get(self.0)
                }

                pub fn display_name(self) -> &'static str {
                    data::display_name::get(self.0)
                }

                pub fn default_state_id(self) -> StateId {
                    data::default_state_id::get(self.0).into()
                }

                pub fn min_state_id(self) -> StateId {
                    data::min_state_id::get(self.0).into()
                }

                pub fn max_state_id(self) -> StateId {
                    data::max_state_id::get(self.0).into()
                }

                pub fn is_field_present(self, field: FieldKey) -> bool {
                    let fields_present = data::fields_present::get(self.0);
                    ((fields_present >> (field as u8)) & 1) == 1
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
