#![no_std]

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::undocumented_unsafe_blocks
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;
