#![deny(rust_2018_compatibility)]
#![deny(rust_2018_idioms)]
#![deny(warnings)]
#![recursion_limit = "128"]

extern crate proc_macro;

use core::sync::atomic::{AtomicUsize, Ordering};
use proc_macro::TokenStream;

use proc_macro2::Span;
use quote::quote;
use syn::{parse, parse_macro_input, ItemStatic};

/// An attribute to place a static variable in shared memory
///
/// This static variable will refer to the same memory location on all cores
#[proc_macro_attribute]
pub fn shared(args: TokenStream, input: TokenStream) -> TokenStream {
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    if !args.is_empty() {
        return parse::Error::new(Span::call_site(), "`#[shared]` takes no arguments")
            .to_compile_error()
            .into();
    }

    let item = parse_macro_input!(input as ItemStatic);

    let attrs = &item.attrs;
    let expr = &item.expr;
    let ident = &item.ident;
    let symbol = format!("{}.{}", ident, COUNT.fetch_add(1, Ordering::AcqRel));
    let ty = &item.ty;
    let vis = &item.vis;
    if item.mutability.is_some() {
        quote!(
            #[cfg(not(target_arch = "arm"))]
            compile_error!("Only the ARM architecture is supported at the moment");

            #(#attrs)*
            #[cfg(microamp)]
            #[link_section = ".shared"]
            #[export_name = #symbol]
            static mut #ident: #ty = {
                fn assert() {
                    microamp::export::is_data::<#ty>();
                }

                #expr
            };

            #[cfg(not(microamp))]
            extern "C" {
                #[link_name = #symbol]
                #vis static mut #ident: #ty;
            }
        )
        .into()
    } else {
        quote!(
            #[cfg(not(target_arch = "arm"))]
            compile_error!("Only the ARM architecture is supported at the moment");

            #(#attrs)*
            #[cfg(microamp)]
            #[link_section = ".shared"]
            #[no_mangle]
            static #ident: #ty = {
                fn assert() {
                    microamp::export::is_data::<#ty>();
                }

                #expr
            };

            #[cfg(not(microamp))]
            #vis struct #ident;

            #[cfg(not(microamp))]
            impl core::ops::Deref for #ident {
                type Target = #ty;

                fn deref(&self) -> &#ty {
                    #[inline(always)]
                    fn assert<T>() where T: Sync {}
                    assert::<#ty>();

                    extern "C" {
                        static #ident: #ty;
                    }

                    unsafe { &#ident }
                }
            }
        )
        .into()
    }
}
