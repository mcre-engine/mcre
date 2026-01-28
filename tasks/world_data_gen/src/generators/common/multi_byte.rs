use std::{mem, slice};

use quote::{format_ident, quote};

use crate::{
    analyzer::Analysis,
    generators::{Unit, UnitGen},
};

type MappingFn<'a, T, U> = Box<dyn Fn(&'a T, &Analysis) -> U>;

pub struct MultiByteGen<'a, T, U> {
    pub name: String,
    pub list: &'a [T],
    pub mapping_fn: MappingFn<'a, T, U>,
}

impl<'a, T, U> UnitGen for MultiByteGen<'a, T, U> {
    fn generate(&self, analysis: &Analysis) -> Unit {
        let data: Box<[U]> = self
            .list
            .iter()
            .map(|el| (self.mapping_fn)(el, analysis))
            .collect();
        let data = box_t_to_box_u8(data);

        let len = self.list.len();

        let data_path = format!("./{}.bin", self.name);
        let type_name = format_ident!("{}", std::any::type_name::<U>());

        let code = quote! {
            static VALUES: [#type_name; #len] =
                unsafe { core::mem::transmute(*include_bytes!(#data_path)) };

            pub(crate) fn get(idx: u16) -> #type_name {
                VALUES[idx as usize]
            }
        };

        Unit {
            name: self.name.clone(),
            code,
            data: Some(data),
        }
    }
}

/// Convert `Box<[T]>` into `Box<[u8]>` without copying.
///
/// # Safety
/// `T` must be POD and have no padding you care about.
#[allow(clippy::cast_slice_from_raw_parts)]
pub fn box_t_to_box_u8<T>(b: Box<[T]>) -> Box<[u8]> {
    let len_t = b.len();
    let ptr = Box::into_raw(b) as *mut T as *mut u8;

    let byte_len = len_t * mem::size_of::<T>();
    unsafe { Box::from_raw(slice::from_raw_parts_mut(ptr, byte_len)) }
}
