use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// Generates the setup code for the main entry point.
#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_runner(item, false)
}

/// Generates the setup code for test functions.
#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    generate_runner(item, true)
}

fn generate_runner(item: TokenStream, is_test: bool) -> TokenStream {
    // 1. Parse the input function (the user's code)
    let input_fn = parse_macro_input!(item as ItemFn);

    // 2. Extract necessary parts
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;

    // 3. Determine the name of the outer wrapper function and the inner logic function
    // If it's main, we enforce the name "main". If it's a test, we keep the user's name.
    let wrapper_name = if is_test {
        fn_name.clone()
    } else {
        syn::parse_quote!(main)
    };

    // We rename the user's actual logic to avoid naming conflicts with the wrapper
    let inner_name = syn::Ident::new(&format!("__mcje_inner_{}", fn_name), fn_name.span());

    // 4. Update the inner function signature to use the new name
    let mut inner_fn = input_fn.clone();
    inner_fn.sig.ident = inner_name.clone();

    // 5. Determine the appropriate attribute (tokio::main or tokio::test)
    let macro_attr = if is_test {
        quote! { #[tokio::test] }
    } else {
        quote! { #[tokio::main] }
    };

    // 6. Check if the user function is async to decide if we need to .await it
    let await_call = if input_fn.sig.asyncness.is_some() {
        quote! { .await }
    } else {
        quote! {}
    };

    // 7. Generate the final code
    let output = quote! {
        #macro_attr
        #fn_vis async fn #wrapper_name() {
            // Define the user's function inside the scope (renamed)
            #inner_fn

            let jvm = ::mcje::init().await;
            let mut env = jvm.attach_current_thread().unwrap();
            ::mcje::bootstrap(&mut env);
            // Call the user's function with the prepared environment
            #inner_name(&mut env)#await_call;
        }
    };

    output.into()
}
