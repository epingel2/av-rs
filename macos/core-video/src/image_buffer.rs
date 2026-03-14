use super::{sys, PixelBuffer};
use core_foundation::CFType;

pub struct ImageBuffer(sys::CVImageBufferRef);
core_foundation::trait_impls!(ImageBuffer);

impl ImageBuffer {
    pub fn pixel_buffer(&self) -> PixelBuffer {
        unsafe { PixelBuffer::from_get_rule(self.0 as _) }
    }

    /// Raw CVImageBufferRef for use with C APIs (e.g. CVMetalTextureCache).
    pub fn as_sys_ref(&self) -> sys::CVImageBufferRef {
        self.0
    }
}
