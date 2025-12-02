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
        let data = self.list.iter().map(&self.mapping_fn);
        let len = self.list.len();

        let code = quote! {
            static VALUES: [&str; #len] = [#( #data, )*];

            pub(crate) fn get(idx: u16) -> &'static str {
                VALUES[idx as usize]
            }
        };

        Unit {
            name: self.name.clone(),
            code,
            data: None,
        }
    }
}
