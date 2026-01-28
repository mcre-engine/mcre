use indexmap::IndexMap;
use mcre_data::block::{Block, BlockStateFieldValues};

pub enum PropSchema<'a> {
    Bool,
    Int(u8, u8),
    Enums {
        contains_bool: bool,
        enums: Vec<&'a str>,
    },
}

pub enum FieldSchema<'a> {
    Bool,
    Int(u8, u8),
    Enum(&'a str),
}

pub struct Analysis<'a> {
    pub enums: IndexMap<&'a str, Vec<&'a str>>,
    // (block_name, prop name) -> field name
    pub prop_to_field: IndexMap<(&'a str, &'a str), String>,
    pub field_to_prop: IndexMap<String, &'a str>,
    pub prop_schema: IndexMap<&'a str, PropSchema<'a>>,
    pub field_schema: IndexMap<String, FieldSchema<'a>>,
    pub foreign_enums: IndexMap<&'a str, Box<[&'a str]>>,
}

pub fn analyze<'a>(
    blocks: &'a [Block],
    foreign_enums: IndexMap<&'a str, Box<[&'a str]>>,
) -> Analysis<'a> {
    let mut enums = IndexMap::<&str, Vec<&str>>::new();
    let mut prop_schema = IndexMap::<&str, PropSchema>::new();
    // prop name -> (enum_name, block_name[])[]
    let mut prop_name_to_enums = IndexMap::<&str, Vec<(&str, Vec<&str>)>>::new();

    for (enum_name, values) in &foreign_enums {
        enums.insert(enum_name, values.to_vec());
    }

    for block in blocks {
        for state in &block.states {
            // enum
            if let BlockStateFieldValues::Enum { enum_name, values } = &state.values {
                if !foreign_enums.contains_key(&enum_name.as_str()) {
                    enums
                        .entry(enum_name)
                        .and_modify(|enum_values| {
                            for value in values {
                                if !enum_values.contains(&value.as_str()) {
                                    enum_values.push(value);
                                }
                            }
                        })
                        .or_insert_with(|| values.iter().map(|value| value.as_str()).collect());
                }

                // prop_name_to_enums
                prop_name_to_enums
                    .entry(&state.name)
                    .and_modify(|enums| {
                        if let Some(i) = enums
                            .iter()
                            .position(|(enum_name1, _)| enum_name1 == enum_name)
                        {
                            let entry = &mut enums[i];
                            if !entry.1.contains(&block.name.as_str()) {
                                entry.1.push(&block.name);
                            }
                        } else {
                            enums.push((enum_name, vec![&block.name]));
                        }
                    })
                    .or_insert_with(|| vec![(enum_name, vec![&block.name])]);
            }

            // prop_schema
            prop_schema
                .entry(&state.name)
                .and_modify(|mut schema| match (&mut schema, &state.values) {
                    (PropSchema::Bool, BlockStateFieldValues::Bool) => {}
                    (PropSchema::Int(min0, max0), BlockStateFieldValues::Int { min, max }) => {
                        *min0 = (*min0).min(*min);
                        *max0 = (*max0).max(*max);
                    }
                    (
                        PropSchema::Enums { enums, .. },
                        BlockStateFieldValues::Enum { enum_name, .. },
                    ) => {
                        if !enums.contains(&enum_name.as_str()) {
                            enums.push(enum_name.as_str());
                        }
                    }
                    (PropSchema::Bool, BlockStateFieldValues::Enum { enum_name, .. }) => {
                        *schema = PropSchema::Enums {
                            contains_bool: true,
                            enums: vec![enum_name],
                        };
                    }
                    (PropSchema::Enums { contains_bool, .. }, BlockStateFieldValues::Bool) => {
                        *contains_bool = true;
                    }
                    _ => {
                        unreachable!();
                    }
                })
                .or_insert_with(|| match &state.values {
                    BlockStateFieldValues::Bool => PropSchema::Bool,
                    BlockStateFieldValues::Int { min, max } => PropSchema::Int(*min, *max),
                    BlockStateFieldValues::Enum { enum_name, .. } => PropSchema::Enums {
                        contains_bool: false,
                        enums: vec![enum_name],
                    },
                });
        }
    }

    let mut field_schema = IndexMap::new();

    for (prop_name, prop_schema) in &prop_schema {
        if matches!(
            prop_schema,
            PropSchema::Bool
                | PropSchema::Enums {
                    contains_bool: true,
                    ..
                }
        ) {
            field_schema.insert(format!("is_{}", prop_name), FieldSchema::Bool);
            continue;
        }

        let schema = match prop_schema {
            PropSchema::Int(min, max) => FieldSchema::Int(*min, *max),
            PropSchema::Enums { enums, .. } => {
                if enums.len() != 1 {
                    continue;
                }
                FieldSchema::Enum(enums[0])
            }
            _ => unreachable!(),
        };
        field_schema.insert(prop_name.to_string(), schema);
    }

    let mut prop_to_field = IndexMap::new();
    let mut field_to_prop = IndexMap::new();

    for (prop_name, enums) in prop_name_to_enums {
        if enums.len() == 1 {
            continue;
        }

        let field_prefixes =
            strip_common_suffix(enums.iter().map(|(enum_name, _)| *enum_name).collect());

        for ((enum_name, blocks), prefix) in enums.into_iter().zip(field_prefixes) {
            let field_name = if prefix.is_empty() {
                prop_name.to_string()
            } else {
                format!("{}_{}", prefix.to_lowercase(), prop_name)
            };

            for block_name in blocks {
                prop_to_field.insert((block_name, prop_name), field_name.clone());
            }

            field_to_prop.insert(field_name.clone(), prop_name);

            field_schema.insert(field_name, FieldSchema::Enum(enum_name));
        }
    }

    Analysis {
        enums,
        prop_to_field,
        field_to_prop,
        prop_schema,
        field_schema,
        foreign_enums,
    }
}

fn strip_common_suffix(strings: Vec<&str>) -> Vec<String> {
    if strings.is_empty() {
        return vec![];
    }

    // Find longest common suffix
    let min_len = strings.iter().map(|s| s.len()).min().unwrap();
    let mut suffix_len = 0;

    for i in 0..min_len {
        let c = strings[0].as_bytes()[strings[0].len() - 1 - i];
        if strings.iter().all(|s| s.as_bytes()[s.len() - 1 - i] == c) {
            suffix_len += 1;
        } else {
            break;
        }
    }

    strings
        .iter()
        .map(|s| s[..s.len() - suffix_len].to_string())
        .collect()
}
