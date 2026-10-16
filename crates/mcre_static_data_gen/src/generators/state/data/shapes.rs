use mcre_data::shape::OcclusionShape;
use quote::quote;

use crate::{analyzer::Analysis, generators::Unit, generators::UnitGen};

pub struct ShapeTableUnit<'a> {
    pub shapes: &'a [OcclusionShape],
}

impl UnitGen for ShapeTableUnit<'_> {
    fn generate(&self, _analysis: &Analysis) -> Unit {
        let arms = self.shapes.iter().enumerate().map(|(id, shape)| {
            let id = id as u16;
            let boxes = shape.boxes.iter().map(|b| {
                let b = b.map(|v| v as f32);
                let [x0, y0, z0, x1, y1, z1] = b;
                quote! { [#x0, #y0, #z0, #x1, #y1, #z1] }
            });
            quote! { #id => &[ #( #boxes ),* ] }
        });

        let code = quote! {
            pub(crate) fn get(id: u16) -> &'static [[f32; 6]] {
                match id {
                    #( #arms, )*
                    _ => &[],
                }
            }
        };

        Unit {
            name: "shapes".to_string(),
            code,
            data: None,
        }
    }
}
