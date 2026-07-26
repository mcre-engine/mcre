use mcre_data::state::{BlockState, StateValue};
use quote::{format_ident, quote};

use crate::{
    analyzer::{Analysis, FieldSchema},
    generators::{Scope, ScopeGen, Unit, UnitGen, common::SubByteGen},
};

pub struct StateFieldsDataScope<'a> {
    pub states: &'a [BlockState],
}

impl<'a> ScopeGen<'a> for StateFieldsDataScope<'a> {
    fn generate(&self, analysis: &Analysis) -> Scope<'a> {
        let mut units: Vec<Box<dyn UnitGen>> = analysis
            .field_schema
            .iter()
            .map(|(field_name, field_schema)| {
                let field_name = field_name.clone();
                let unit = match field_schema {
                    FieldSchema::Bool => SubByteGen {
                        name: field_name.clone(),
                        is_bool: true,
                        min: 0,
                        max: 1,
                        list: self.states,
                        mapping_fn: Box::new(move |state, _analysis: &Analysis<'_>| {
                            state
                                .state_values
                                .get(field_name.strip_prefix("is_").unwrap())
                                .map_or(0, |val| matches!(val, StateValue::Bool(true)) as u8)
                        }),
                    },
                    FieldSchema::Int(min, max) => SubByteGen {
                        name: field_name.clone(),
                        is_bool: false,
                        min: *min,
                        max: *max,
                        list: self.states,
                        mapping_fn: {
                            let min = *min;
                            Box::new(move |state, _analysis: &Analysis<'_>| {
                                state
                                    .state_values
                                    .get(&field_name)
                                    .and_then(|val| match val {
                                        StateValue::Int(val) => Some(*val),
                                        _ => None,
                                    })
                                    .unwrap_or(min)
                            })
                        },
                    },
                    FieldSchema::Enum(enum_name) => SubByteGen {
                        name: field_name.clone(),
                        is_bool: false,
                        min: 0,
                        max: (analysis.enums.get(enum_name).unwrap().len() - 1) as u8,
                        list: self.states,
                        mapping_fn: {
                            let enum_name = enum_name.to_string();
                            Box::new(move |state, analysis: &Analysis<'_>| {
                                let prop_name = if let Some(prop_name) =
                                    analysis.field_to_prop.get(&field_name)
                                {
                                    *prop_name
                                } else {
                                    field_name.as_str()
                                };
                                state
                                    .state_values
                                    .get(prop_name)
                                    .and_then(|val| {
                                        if let Some(field_name1) = analysis
                                            .prop_to_field
                                            .get(&(state.block_name.as_str(), prop_name))
                                            && field_name1 != &field_name
                                        {
                                            None
                                        } else if let StateValue::String(val) = val {
                                            let enum_values =
                                                analysis.enums.get(&enum_name.as_str()).unwrap();

                                            Some(
                                                enum_values
                                                    .iter()
                                                    .position(|variant| variant == val)
                                                    .unwrap()
                                                    .try_into()
                                                    .unwrap(),
                                            )
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or_default()
                            })
                        },
                    },
                };

                Box::new(unit) as Box<dyn UnitGen>
            })
            .collect();

        units.push(Box::new(StateFieldsDataRootUnit));

        Scope {
            name: "fields".to_string(),
            sub_scopes: Box::new([]),
            units: units.into_boxed_slice(),
        }
    }
}

pub struct StateFieldsDataRootUnit;

impl UnitGen for StateFieldsDataRootUnit {
    fn generate(&self, analysis: &Analysis) -> Unit {
        let imports = analysis.field_schema.iter().map(|(field_name, _)| {
            let field_name = format_ident!("{}", field_name);
            quote! { pub(crate) mod #field_name; }
        });

        let code = quote! { #( #imports )* };

        Unit {
            name: "mod".to_string(),
            code,
            data: None,
        }
    }
}
