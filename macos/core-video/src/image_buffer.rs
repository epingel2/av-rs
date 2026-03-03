use super::{sys, PixelBuffer};
use core_foundation::CFType;

pub struct ImageBuffer(sys::CVImageBufferRef);
core_foundation::trait_impls!(ImageBuffer);

// CVPixelBuffer is a reference-counted CFType safe to share across threads.
// Send is already provided by trait_impls!; we add Sync so Arc<ImageBuffer> is Send.
unsafe impl Sync for ImageBuffer {}

impl ImageBuffer {
    pub fn pixel_buffer(&self) -> PixelBuffer {
        unsafe { PixelBuffer::from_get_rule(self.0 as _) }
    }
}
