#![deny(rust_2018_compatibility)]
#![deny(rust_2018_idioms)]
#![feature(optin_builtin_traits)]
#![no_std]

pub use microamp_macros::shared;

mod cfail;
#[doc(hidden)]
pub mod export;
