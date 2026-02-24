use super::sys;
use core_foundation::{result, CFType, Dictionary, OSStatus};
use core_media::{BlockBuffer, SampleBuffer, VideoFormatDescription};
use core_video::ImageBuffer;
use std::{marker::PhantomPinned, pin::Pin, sync::mpsc};

struct CallbackContext {
    frames: Option<mpsc::SyncSender<Result<ImageBuffer, OSStatus>>>,
    _pinned: PhantomPinned,
}

pub struct DecompressionSession {
    inner: sys::VTDecompressionSessionRef,
    frames: mpsc::Receiver<Result<ImageBuffer, OSStatus>>,
    _callback_context: Pin<Box<CallbackContext>>,
}

impl DecompressionSession {
    unsafe fn create_session(
        format_desc: &VideoFormatDescription,
        destination_image_buffer_attributes: Option<Dictionary>,
        callback_context: *mut CallbackContext,
    ) -> Result<sys::VTDecompressionSessionRef, OSStatus> {
        unsafe extern "C" fn callback(
            output_callback_ref_con: *mut std::os::raw::c_void,
            _source_frame_ref_con: *mut std::os::raw::c_void,
            status: sys::OSStatus,
            _info_flags: sys::VTDecodeInfoFlags,
            image_buffer: sys::CVImageBufferRef,
            _presentation_time_stamp: sys::CMTime,
            _presentation_duration: sys::CMTime,
        ) {
            // SAFETY: Panicking is not allowed across an FFI boundary. If you add code that may panic here
            // then you must wrap it in `std::panic::catch_unwind`.
            let ctx = &mut *(output_callback_ref_con as *mut CallbackContext);
            if let Some(frames) = ctx.frames.as_ref() {
                let decoded = if image_buffer.is_null() {
                    Err(status.into())
                } else {
                    result(status.into()).map(|_| ImageBuffer::from_get_rule(image_buffer as _))
                };
                if frames.try_send(decoded).is_err() {
                    ctx.frames = None;
                }
            }
        }
        let callback_record = sys::VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(callback),
            decompressionOutputRefCon: callback_context as *mut _,
        };
        let mut ret = std::ptr::null_mut();
        result(
            sys::VTDecompressionSessionCreate(
                std::ptr::null_mut(),
                format_desc.cf_type_ref() as _,
                std::ptr::null(),
                destination_image_buffer_attributes
                    .as_ref()
                    .map(|d| d.cf_type_ref() as _)
                    .unwrap_or(std::ptr::null()),
                &callback_record as _,
                &mut ret as _,
            )
            .into(),
        )?;
        Ok(ret)
    }

    pub fn new(format_desc: &VideoFormatDescription) -> Result<Self, OSStatus> {
        let (tx, rx) = mpsc::sync_channel(120);
        let callback_context = Box::pin(CallbackContext {
            frames: Some(tx),
            _pinned: PhantomPinned,
        });
        let inner = unsafe { Self::create_session(format_desc, None, &*callback_context as *const _ as *mut _)? };
        Ok(Self {
            inner,
            frames: rx,
            _callback_context: callback_context,
        })
    }

    pub fn new_with_destination_image_buffer_attributes(
        format_desc: &VideoFormatDescription,
        destination_image_buffer_attributes: Dictionary,
    ) -> Result<Self, OSStatus> {
        let (tx, rx) = mpsc::sync_channel(120);
        let callback_context = Box::pin(CallbackContext {
            frames: Some(tx),
            _pinned: PhantomPinned,
        });
        let inner = unsafe { Self::create_session(format_desc, Some(destination_image_buffer_attributes), &*callback_context as *const _ as *mut _)? };
        Ok(Self {
            inner,
            frames: rx,
            _callback_context: callback_context,
        })
    }

    /// Submits a frame for decoding. Decoded frames are delivered asynchronously via [`frames()`].
    /// Does not block.
    pub fn decode_frame(&mut self, frame_data: &[u8], format_desc: &VideoFormatDescription) -> Result<(), OSStatus> {
        result(
            unsafe {
                let block_buffer = BlockBuffer::with_memory_block(frame_data)?;
                let sample_buffer = SampleBuffer::new(&block_buffer, Some(format_desc.into()), 1, Some(&[frame_data.len()]))?;
                sys::VTDecompressionSessionDecodeFrame(
                    self.inner,
                    sample_buffer.cf_type_ref() as _,
                    sys::kVTDecodeFrame_EnableAsynchronousDecompression,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            }
            .into(),
        )
    }

    /// Receiver for decoded frames. Poll this after calling [`decode_frame()`].
    /// If the channel buffer is exceeded, the sender is dropped and no further frames will be delivered.
    pub fn frames(&self) -> &mpsc::Receiver<Result<ImageBuffer, OSStatus>> {
        &self.frames
    }
}

impl Drop for DecompressionSession {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe {
                sys::VTDecompressionSessionWaitForAsynchronousFrames(self.inner);
                sys::VTDecompressionSessionInvalidate(self.inner);
            }
            self.inner = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use core_foundation::{MutableDictionary, Number};
    use core_media::VideoFormatDescription;
    use std::{fs::File, io::Read};

    #[test]
    fn test_decompression_session() {
        let mut f = File::open("src/testdata/smptebars.h264").unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();

        let nalus: Vec<_> = h264::iterate_annex_b(&buf).collect();
        let format_desc = VideoFormatDescription::with_h264_parameter_sets(&[nalus[0], nalus[1]], 4).unwrap();
        let mut sess = DecompressionSession::new(&format_desc).unwrap();

        // This file is encoded as exactly one NALU per frame.
        for nalu in &nalus[3..10] {
            let mut frame_data = vec![0, 0, (nalu.len() / 256) as u8, nalu.len() as u8];
            frame_data.extend_from_slice(nalu);
            sess.decode_frame(&frame_data, &format_desc).unwrap();
        }
        for _ in &nalus[3..10] {
            sess.frames().recv().unwrap().unwrap();
        }
    }

    #[test]
    fn test_decompression_session_with_destination_image_buffer_attributes() {
        let mut f = File::open("src/testdata/smptebars.h264").unwrap();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();

        let nalus: Vec<_> = h264::iterate_annex_b(&buf).collect();
        let format_desc = VideoFormatDescription::with_h264_parameter_sets(&[nalus[0], nalus[1]], 4).unwrap();

        let mut destination_image_buffer_attributes = MutableDictionary::new_cf_type();
        // Set the pixel format to BGRA
        unsafe {
            let key = sys::kCVPixelBufferPixelFormatTypeKey as _;
            let value = Number::from(sys::kCVPixelFormatType_32BGRA).cf_type_ref();
            destination_image_buffer_attributes.set_value(key, value);
        }
        let mut sess = DecompressionSession::new_with_destination_image_buffer_attributes(&format_desc, destination_image_buffer_attributes.into()).unwrap();

        // This file is encoded as exactly one NALU per frame.
        for nalu in &nalus[3..10] {
            let mut frame_data = vec![0, 0, (nalu.len() / 256) as u8, nalu.len() as u8];
            frame_data.extend_from_slice(nalu);
            sess.decode_frame(&frame_data, &format_desc).unwrap();
        }
        for _ in &nalus[3..10] {
            let decoded_frame = sess.frames().recv().unwrap().unwrap();
            let decoded_pixel_format = decoded_frame.pixel_buffer().pixel_format_type();
            let decoded_width = decoded_frame.pixel_buffer().width();
            let decoded_height = decoded_frame.pixel_buffer().height();

            assert_eq!(decoded_width, 1280);
            assert_eq!(decoded_height, 720);
            assert_eq!(decoded_pixel_format, sys::kCVPixelFormatType_32BGRA);
        }
    }
}
