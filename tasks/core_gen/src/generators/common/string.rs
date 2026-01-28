use quote::quote;

use crate::{
    analyzer::Analysis,
    generators::{Unit, UnitGen},
};

pub struct StringGen<'a, T> {
    pub name: String,
    pub list: &'a [T],
    pub mapping_fn: Box<dyn Fn(&'a T) -> &'a str>,
}

impl<'a, T> UnitGen for StringGen<'a, T> {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let strings: Vec<&str> = self.list.iter().map(&self.mapping_fn).collect();
        let len = strings.len();

        let mut offsets = Vec::with_capacity(len + 1);
        let mut string_data = Vec::new();
        let mut current_offset = 0u32;

        offsets.push(current_offset);

        for s in &strings {
            let bytes = s.as_bytes();
            string_data.extend_from_slice(bytes);
            current_offset += bytes.len() as u32;
            offsets.push(current_offset);
        }

        let mut data_blob =
            Vec::with_capacity((len + 1) * std::mem::size_of::<u32>() + string_data.len());
        for offset in offsets {
            data_blob.extend_from_slice(&offset.to_ne_bytes());
        }
        data_blob.extend_from_slice(&string_data);

        let data = data_blob.into_boxed_slice();

        let data_path = format!("./{}.bin", self.name);

        let code = quote! {
            const COUNT: usize = #len;
            static DATA: &[u8] = include_bytes!(#data_path);

            pub(crate) fn get(idx: u16) -> &'static str {
                let idx = idx as usize;

                let get_offset = |i: usize| -> u32 {
                    let start = i * core::mem::size_of::<u32>();
                    let end = start + core::mem::size_of::<u32>();
                    let bytes: [u8; core::mem::size_of::<u32>()] = DATA[start..end].try_into().unwrap();
                    u32::from_ne_bytes(bytes)
                };

                let start = get_offset(idx) as usize;
                let end = get_offset(idx + 1) as usize;

                let strings_data_offset = (COUNT + 1) * core::mem::size_of::<u32>();
                let strings_data = &DATA[strings_data_offset..];

                let slice = &strings_data[start..end];

                // This is safe because we know that we have stored valid UTF-8 strings.
                unsafe { core::str::from_utf8_unchecked(slice) }
            }
        };

        Unit {
            name: self.name.clone(),
            code,
            data: Some(data),
        }
    }
}
