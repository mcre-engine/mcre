mod data;

use crate::{
    analyzer::Analysis,
    generators::{Scope, ScopeGen, Unit, UnitGen, block::data::BlockDataScope},
};

use mcre_data::block::Block;
use quote::{format_ident, quote};

pub struct BlockScope<'a> {
    pub blocks: &'a [Block],
}

impl<'a> ScopeGen<'a> for BlockScope<'a> {
    fn generate(&self, _analysis: &Analysis) -> Scope<'a> {
        Scope {
            name: "block".to_string(),
            units: Box::new([
                Box::new(BlockRootUnit {
                    blocks: self.blocks,
                }),
                Box::new(BlockConstsUnit {
                    blocks: self.blocks,
                }),
            ]),
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
            mod consts;

            use crate::{StateId, FieldKey};

            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

                pub fn all() -> impl Iterator<Item = Self> {
                    BlockIdIter::new(BlockId(0), Self::MAX)
                }
            }

            pub struct BlockIdIter {
                current: u16,
                end: u16,
            }

            impl BlockIdIter {
                // inclusive range
                pub fn new(start: BlockId, end: BlockId) -> Self {
                    Self {
                        current: start.0,
                        end: end.0,
                    }
                }
            }

            impl Iterator for BlockIdIter {
                type Item = BlockId;

                fn next(&mut self) -> Option<Self::Item> {
                    if self.current > self.end {
                        None
                    } else {
                        let id = self.current;
                        self.current += 1;
                        Some(BlockId(id))
                    }
                }

                fn size_hint(&self) -> (usize, Option<usize>) {
                    let remaining = (self.end - self.current) as usize;
                    (remaining, Some(remaining))
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

pub struct BlockConstsUnit<'a> {
    blocks: &'a [Block],
}

impl UnitGen for BlockConstsUnit<'_> {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let consts = self.blocks.iter().map(|block| {
            let name = format_ident!("{}", block.name.to_uppercase());
            let id = block.id;
            quote! {
                pub const #name: Self = Self(#id);
            }
        });
        let code = quote! {
            use super::BlockId;

            impl BlockId {
                #( #consts )*
            }
        };

        Unit {
            name: "consts".to_string(),
            code,
            data: None,
        }
    }
}
