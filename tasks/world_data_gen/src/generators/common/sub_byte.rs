use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    analyzer::Analysis,
    generators::{Unit, UnitGen},
};

type MappingFn<'a, T> = Box<dyn Fn(&'a T, &Analysis) -> u8>;

// u1 (bool), u2, u4, u8
pub struct SubByteGen<'a, T> {
    // inclusive range
    pub name: String,
    pub is_bool: bool,
    pub min: u8,
    pub max: u8,
    pub list: &'a [T],
    pub mapping_fn: MappingFn<'a, T>,
}

impl<'a, T> UnitGen for SubByteGen<'a, T> {
    fn generate(&self, analysis: &Analysis) -> Unit {
        let mut array = SubByteArray::new(self.min, self.max, self.list.len());
        for (i, el) in self.list.iter().enumerate() {
            let val = (self.mapping_fn)(el, analysis);
            array.set(i, val);
        }

        let (code, data) = array.compile(&self.name, self.is_bool);

        Unit {
            name: self.name.clone(),
            code,
            data: Some(data),
        }
    }
}

pub struct SubByteArray {
    min: u8,
    max: u8,
    pow: u8,
    #[allow(unused)]
    len: usize,
    values: Box<[u8]>,
}

impl SubByteArray {
    // inclusive range
    fn new(min: u8, max: u8, len: usize) -> Self {
        let (min, pow) = Self::compute_min_and_pow(min, max);

        let bits_per_value = 1 << pow;
        let total_bits = bits_per_value * len;
        let total_bytes = total_bits.div_ceil(8);

        Self {
            min,
            max,
            pow,
            len,
            values: vec![0; total_bytes].into_boxed_slice(),
        }
    }

    fn compute_min_and_pow(min: u8, max: u8) -> (u8, u8) {
        let pow_with_min = Self::compute_pow(max as u16 - min as u16 + 1);
        let pow_without_min = Self::compute_pow(max as u16 + 1);

        if pow_without_min == pow_with_min {
            (0, pow_without_min)
        } else {
            (min, pow_with_min)
        }
    }

    fn compute_pow(num_values: u16) -> u8 {
        let num_bits = (num_values as f32).log2().ceil() as u8;
        (num_bits as f32).log2().ceil() as u8
    }

    fn set(&mut self, index: usize, val: u8) {
        assert!(val >= self.min && val <= self.max, "Value: {val}");
        let bits_per_value = 1 << self.pow;

        let values_per_byte = 8 / bits_per_value;

        let value_index = index / values_per_byte;
        let shift = (index % values_per_byte) * bits_per_value;

        let mask = ((1u16 << bits_per_value) - 1) as u8;

        self.values[value_index] |= ((val - self.min) & mask) << shift;
    }

    #[allow(unused)]
    fn get(&self, index: usize) -> Option<u8> {
        let bits_per_value = 1 << self.pow;

        let values_per_byte = 8 / bits_per_value;

        let value_index = index / values_per_byte;
        let shift = (index % values_per_byte) * bits_per_value;

        let mask = ((1u16 << bits_per_value) - 1) as u8;

        self.values
            .get(value_index)
            .map(|&value| ((value >> shift) & mask) + self.min)
    }

    pub fn compile(self, name: &str, is_bool: bool) -> (TokenStream, Box<[u8]>) {
        let len = self.values.len();
        let path = format!("./{}.bin", name);

        let code = if is_bool {
            quote! {
                static VALUES: [u8; #len] = *include_bytes!(#path);

                pub(crate) fn get(idx: u16) -> bool {
                    let byte_pos = idx / 8;
                    let bit_pos = idx % 8;

                    let byte = VALUES[byte_pos as usize];

                    ((byte >> bit_pos) & 1) == 1
                }
            }
        } else if self.pow == 3 {
            quote! {
                static VALUES: [u8; #len] = *include_bytes!(#path);

                pub(crate) fn get(idx: u16) -> u8 {
                    VALUES[idx as usize]
                }
            }
        } else {
            let bits_per_value = 1u16 << self.pow; // e.g., 2
            let values_per_byte = 8 / bits_per_value; // e.g., 4
            let mask = ((1u16 << bits_per_value) - 1) as u8;

            let bit_pos_calculation = if bits_per_value == 1 {
                quote! {
                    let bit_pos = idx % #values_per_byte;
                }
            } else {
                quote! {
                    let bit_pos = (idx % #values_per_byte) * #bits_per_value;
                }
            };

            if self.min == 0 {
                quote! {
                    static VALUES: [u8; #len] = *include_bytes!(#path);

                    pub(crate) fn get(idx: u16) -> u8 {
                        let byte_pos = idx / #values_per_byte;
                        #bit_pos_calculation // Insert fixed calculation

                        let byte = VALUES[byte_pos as usize];
                        (byte >> bit_pos) & #mask
                    }
                }
            } else {
                let min = self.min;
                quote! {
                    static VALUES: [u8; #len] = *include_bytes!(#path);

                    pub(crate) fn get(idx: u16) -> u8 {
                        let byte_pos = idx / #values_per_byte;
                        #bit_pos_calculation // Insert fixed calculation

                        let byte = VALUES[byte_pos as usize];
                        ((byte >> bit_pos) & #mask) + #min
                    }
                }
            }
        };

        (code, self.values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_byte_array() {
        let mut arr = SubByteArray::new(1, 16, 100);

        for i in 0..16 {
            arr.set(i, i as u8 + 1);
        }

        for i in 0..16 {
            assert_eq!(arr.get(i).unwrap(), i as u8 + 1);
        }
    }
}
