use alloc::{boxed::Box, vec::Vec};
use mcre_core::{Block, BlockPos, BlockState, LegacyRand, PropVal, WeightedList};

use crate::{BlockModelRegistryKey, Condition, Quadrant};

#[derive(Debug, Clone)]
pub enum BakedBlockStateDefinition {
    Variants(Box<[BakedVariantEntry]>),
    Multipart(Box<[BakedMultipartEntry]>),
}

#[derive(Debug, Clone)]
pub struct BakedVariantEntry {
    pub filter: Box<[PropVal]>,
    pub definition: BakedVariantDefinition,
}

#[derive(Debug, Clone)]
pub enum BakedVariantDefinition {
    Single(BakedModelVariant),
    Multiple(WeightedList<BakedModelVariant>),
}

#[derive(Debug, Clone, Copy)]
pub struct BakedModelVariant {
    pub model: BlockModelRegistryKey,
    pub uvlock: bool,
    pub x: Quadrant,
    pub y: Quadrant,
}

#[derive(Debug, Clone)]
pub struct BakedMultipartEntry {
    pub apply: BakedVariantDefinition,
    pub when: Option<Condition>,
}

#[derive(Debug, Clone)]
pub struct BlockModelResolution {
    pub models: Box<[BakedModelVariant]>,
}

impl BakedBlockStateDefinition {
    pub fn resolve(&self, state: BlockState, pos: BlockPos) -> BlockModelResolution {
        match self {
            Self::Variants(variants) => {
                'variant_loop: for variant in variants.iter() {
                    for filter in &variant.filter {
                        let Some(val) = state.get_prop(filter.key()) else {
                            continue 'variant_loop;
                        };

                        if val != *filter {
                            continue 'variant_loop;
                        };
                    }

                    let model = match &variant.definition {
                        BakedVariantDefinition::Single(model) => *model,
                        BakedVariantDefinition::Multiple(models) => *models
                            .get_random(&mut LegacyRand::from_seed(pos.seed() as u64))
                            .unwrap(),
                    };

                    return BlockModelResolution {
                        models: Box::new([model]),
                    };
                }

                BlockModelResolution {
                    models: Box::new([]),
                }
            }
            Self::Multipart(rules) => {
                let mut models = Vec::new();

                let mut source = LegacyRand::from_seed(pos.seed() as u64);

                for rule in rules {
                    let condition_met = if let Some(condition) = &rule.when {
                        condition.test(state)
                    } else {
                        true
                    };

                    if condition_met {
                        let model = match &rule.apply {
                            BakedVariantDefinition::Single(model) => *model,
                            BakedVariantDefinition::Multiple(models) => {
                                *models.get_random(&mut source).unwrap()
                            }
                        };

                        models.push(model);
                    }
                }

                BlockModelResolution {
                    models: models.into_boxed_slice(),
                }
            }
        }
    }
}

pub struct BlockStateDefinitionsRegistry {
    definitions: Box<[BakedBlockStateDefinition]>,
}

impl BlockStateDefinitionsRegistry {
    pub fn get(&self, id: Block) -> &BakedBlockStateDefinition {
        &self.definitions[u16::from(id) as usize]
    }
}
