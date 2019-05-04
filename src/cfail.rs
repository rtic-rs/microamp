//! ``` compile_fail
//! microamp::export::is_data::<fn()>();
//! ```
//!
//! ``` compile_fail
//! microamp::export::is_data::<fn() -> !>();
//! ```
//!
//! ``` compile_fail
//! trait Foo {}
//! microamp::export::is_data::<dyn Foo>();
//! ```
