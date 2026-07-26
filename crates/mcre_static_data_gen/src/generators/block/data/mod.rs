use crate::{
    analyzer::{Analysis, FieldSchema},
    generators::{
        Scope, ScopeGen, Unit, UnitGen,
        common::{MultiByteGen, StringGen, SubByteGen},
    },
};

use mcre_data::block::{Block, BlockStateFieldValues};
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
                    name: "default_state".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.default_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "min_state".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.min_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "max_state".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.max_state_id),
                }),
                Box::new(MultiByteGen {
                    name: "fields_present".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, analysis: &Analysis<'_>| {
                        let mut fields_present = 0;

                        for (i, (field_name, schema)) in analysis.field_schema.iter().enumerate() {
                            let present = match schema {
                                FieldSchema::Bool => {
                                    let name = field_name.strip_prefix("is_").unwrap();
                                    block.states.iter().any(|state| {
                                        state.name == name
                                            && matches!(state.values, BlockStateFieldValues::Bool)
                                    })
                                }
                                FieldSchema::Int(_, _) => {
                                    block.states.iter().any(|state| &state.name == field_name)
                                }
                                FieldSchema::Enum(_) => {
                                    let prop_name = if let Some(prop_name) =
                                        analysis.field_to_prop.get(field_name)
                                    {
                                        *prop_name
                                    } else {
                                        field_name.as_str()
                                    };

                                    if let Some(field_name1) = analysis
                                        .prop_to_field
                                        .get(&(block.name.as_str(), prop_name))
                                        && field_name1 != field_name
                                    {
                                        false
                                    } else {
                                        block.states.iter().any(|state| state.name == prop_name)
                                    }
                                }
                            };

                            if present {
                                let flag = 1u128 << i;

                                fields_present |= flag;
                            }
                        }

                        fields_present
                    }),
                }),
                Box::new(SubByteGen {
                    name: "has_collision".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.has_collision as u8
                    }),
                }),
                Box::new(MultiByteGen {
                    name: "explosion_resistance".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.explosion_resistance
                    }),
                }),
                Box::new(MultiByteGen {
                    name: "friction".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.friction),
                }),
                Box::new(MultiByteGen {
                    name: "speed_factor".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.speed_factor),
                }),
                Box::new(MultiByteGen {
                    name: "jump_factor".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.jump_factor),
                }),
                Box::new(MultiByteGen {
                    name: "bounce_restitution".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.bounce_restitution
                    }),
                }),
                Box::new(MultiByteGen {
                    name: "fall_distance_reduction".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.fall_distance_reduction
                    }),
                }),
                Box::new(StringGen {
                    name: "push_reaction".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block| &block.push_reaction),
                }),
                Box::new(StringGen {
                    name: "instrument".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block| &block.instrument),
                }),
                Box::new(MultiByteGen {
                    name: "destroy_speed".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.destroy_speed),
                }),
                Box::new(SubByteGen {
                    name: "requires_correct_tool_for_drops".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.requires_correct_tool_for_drops as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "can_occlude".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.can_occlude as u8),
                }),
                Box::new(SubByteGen {
                    name: "ignited_by_lava".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.ignited_by_lava as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "is_air".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.is_air as u8),
                }),
                Box::new(SubByteGen {
                    name: "spawn_terrain_particles".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.spawn_terrain_particles as u8
                    }),
                }),
                Box::new(SubByteGen {
                    name: "replaceable".to_string(),
                    is_bool: true,
                    min: 0,
                    max: 1,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.replaceable as u8),
                }),
                Box::new(SubByteGen {
                    name: "offset_type".to_string(),
                    is_bool: false,
                    min: 0,
                    max: 2,
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| block.offset_type as u8),
                }),
                Box::new(MultiByteGen {
                    name: "max_horizontal_offset".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.max_horizontal_offset
                    }),
                }),
                Box::new(MultiByteGen {
                    name: "max_vertical_offset".to_string(),
                    list: self.blocks,
                    mapping_fn: Box::new(|block, _analysis: &Analysis<'_>| {
                        block.max_vertical_offset
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
            pub(crate) mod bounce_restitution;
            pub(crate) mod can_occlude;
            pub(crate) mod default_state;
            pub(crate) mod destroy_speed;
            pub(crate) mod display_name;
            pub(crate) mod explosion_resistance;
            pub(crate) mod fall_distance_reduction;
            pub(crate) mod friction;
            pub(crate) mod has_collision;
            pub(crate) mod ignited_by_lava;
            pub(crate) mod instrument;
            pub(crate) mod is_air;
            pub(crate) mod jump_factor;
            pub(crate) mod max_horizontal_offset;
            pub(crate) mod max_state;
            pub(crate) mod max_vertical_offset;
            pub(crate) mod min_state;
            pub(crate) mod name;
            pub(crate) mod offset_type;
            pub(crate) mod push_reaction;
            pub(crate) mod replaceable;
            pub(crate) mod requires_correct_tool_for_drops;
            pub(crate) mod spawn_terrain_particles;
            pub(crate) mod speed_factor;
            pub(crate) mod fields_present;
        };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
