//! Zero-copy CVPixelBuffer → Metal texture via CVMetalTextureCache.
//! The CVMetal API is not exposed in the C header when not compiling as Objective-C,
//! so we declare it here and link CoreVideo + Metal.

use super::sys;
use crate::ImageBuffer;
use core_foundation::sys::CFRelease;
use std::ffi::c_void;

/// Opaque CVMetalTextureCacheRef (pointer-sized; pass by value in C API).
#[repr(C)]
#[derive(Clone, Copy)]
struct CVMetalTextureCacheRef(*mut c_void);

/// Opaque CVMetalTextureRef (pointer-sized; pass by value in C API).
#[repr(C)]
#[derive(Clone, Copy)]
struct CVMetalTextureRef(*mut c_void);

#[link(name = "CoreVideo", kind = "framework")]
#[link(name = "Metal", kind = "framework")]
extern "C" {
    fn CVMetalTextureCacheCreate(
        allocator: *mut c_void,
        cacheAttributes: *mut c_void,
        device: *mut c_void,
        textureAttributes: *mut c_void,
        cacheOut: *mut CVMetalTextureCacheRef,
    ) -> sys::CVReturn;

    fn CVMetalTextureCacheCreateTextureFromImage(
        allocator: *mut c_void,
        textureCache: CVMetalTextureCacheRef,
        sourceImage: sys::CVImageBufferRef,
        textureAttributes: *mut c_void,
        pixelFormat: u64,
        width: usize,
        height: usize,
        planeIndex: usize,
        textureOut: *mut CVMetalTextureRef,
    ) -> sys::CVReturn;

    fn CVMetalTextureGetTexture(image: CVMetalTextureRef) -> *mut c_void;
}

/// Wrapper around CVMetalTextureCache. Create with [MetalTextureCache::new].
pub struct MetalTextureCache {
    cache: CVMetalTextureCacheRef,
}

impl MetalTextureCache {
    /// `device` is the raw MTLDevice pointer (e.g. `Retained::as_ptr(&device) as *mut _`).
    pub fn new(device: *mut c_void) -> Result<Self, String> {
        let mut cache = CVMetalTextureCacheRef(std::ptr::null_mut());
        let ret = unsafe {
            CVMetalTextureCacheCreate(
                std::ptr::null_mut(), // kCFAllocatorDefault
                std::ptr::null_mut(), // cache attributes
                device,
                std::ptr::null_mut(), // texture attributes
                &mut cache,
            )
        };
        if ret != sys::kCVReturnSuccess {
            return Err(format!("CVMetalTextureCacheCreate failed: {}", ret));
        }
        if cache.0.is_null() {
            return Err("CVMetalTextureCacheCreate returned null".into());
        }
        Ok(Self { cache })
    }

    /// Create a Metal texture from a CVImageBuffer (e.g. CVPixelBuffer from Video Toolbox).
    /// * `image` – source (e.g. from `ImageBuffer::as_sys_ref()`).
    /// * `pixel_format` – Metal pixel format (e.g. `MTLPixelFormat::RGBA8Unorm.0 as u64`).
    /// * `width`, `height` – texture dimensions (for 2vuy, width is typically source width / 2).
    /// * `plane_index` – plane index (0 for non-planar).
    pub fn create_texture_from_image(
        &self,
        image: &ImageBuffer,
        pixel_format: u64,
        width: usize,
        height: usize,
        plane_index: usize,
    ) -> Result<MetalTexture, String> {
        let mut texture_out = CVMetalTextureRef(std::ptr::null_mut());
        let ret = unsafe {
            CVMetalTextureCacheCreateTextureFromImage(
                std::ptr::null_mut(),
                self.cache,
                image.as_sys_ref(),
                std::ptr::null_mut(),
                pixel_format,
                width,
                height,
                plane_index,
                &mut texture_out,
            )
        };
        if ret != sys::kCVReturnSuccess {
            return Err(format!("CVMetalTextureCacheCreateTextureFromImage failed: {}", ret));
        }
        if texture_out.0.is_null() {
            return Err("CVMetalTextureCacheCreateTextureFromImage returned null".into());
        }
        Ok(MetalTexture(texture_out))
    }
}

impl Drop for MetalTextureCache {
    fn drop(&mut self) {
        if !self.cache.0.is_null() {
            unsafe { CFRelease(self.cache.0 as _) };
            self.cache.0 = std::ptr::null_mut();
        }
    }
}

/// Wrapper around CVMetalTextureRef. Release with [Drop].
pub struct MetalTexture(CVMetalTextureRef);

impl MetalTexture {
    /// Raw `id<MTLTexture>` pointer. Caller may wrap with objc2-metal types.
    pub fn mtl_texture_ptr(&self) -> *mut c_void {
        unsafe { CVMetalTextureGetTexture(self.0) }
    }
}

impl Drop for MetalTexture {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe { CFRelease(self.0 .0 as _) };
            self.0 .0 = std::ptr::null_mut();
        }
    }
}
