use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_runner(item, false)
}

#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_runner(item, true)
}

fn generate_runner(item: TokenStream, is_test: bool) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;

    let wrapper_name = if is_test {
        fn_name.clone()
    } else {
        syn::parse_quote!(main)
    };

    let inner_name = syn::Ident::new(&format!("__mcje_inner_{}", fn_name), fn_name.span());

    let mut inner_fn = input_fn.clone();
    inner_fn.sig.ident = inner_name.clone();

    let wrapper_attr = if is_test {
        quote! { #[test] }
    } else {
        quote! {}
    };

    let body = if input_fn.sig.asyncness.is_some() {
        quote! {
            let _ = rt.block_on(#inner_name(&mut env));
        }
    } else {
        quote! {
            #inner_name(&mut env);
        }
    };

    let output = quote! {
        #wrapper_attr
        #fn_vis fn #wrapper_name() {
            #inner_fn

            let rt = tokio::runtime::Runtime::new().unwrap();
            let jvm = rt.block_on(::mcje::init());
            jvm.attach_current_thread(|mut env| {
                ::mcje::bootstrap(&mut env);
                #body
                Ok::<_, jni::errors::Error>(())
            }).unwrap();
        }
    };

    output.into()
}
