#![cfg(target_os = "macos")]

pub mod sys {
    #![allow(
        deref_nullptr,
        non_snake_case,
        non_upper_case_globals,
        non_camel_case_types,
        clippy::unreadable_literal,
        clippy::cognitive_complexity
    )]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub mod image_buffer;
pub use image_buffer::*;

pub mod pixel_buffer;
pub use pixel_buffer::*;

#[cfg(feature = "metal")]
pub mod metal_texture_cache;
#[cfg(feature = "metal")]
pub use metal_texture_cache::*;
