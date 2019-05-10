#![deny(rust_2018_compatibility)]
#![deny(rust_2018_idioms)]
#![doc(include = "../README.md")]
#![feature(external_doc)]
#![deny(missing_docs)]
#![feature(optin_builtin_traits)]
#![no_std]

pub use microamp_macros::shared;

mod cfail;
#[doc(hidden)]
pub mod export;
