use self::ffi::*;
use anyhow::{bail, Result};
use core_foundation::base::TCFType;
use core_foundation::url::CFURL;
use core_foundation_sys::base::OSStatus;
use core_foundation_sys::url::CFURLRef;
use std::mem::size_of_val;
use std::path::Path;
use std::ptr::{addr_of, addr_of_mut, null};

pub struct AudioReader {
    file: ExtAudioFile,
    format: AudioStreamBasicDescription,
    frames_per_packet: u32,
    buffer: Vec<f64>,
    pos: usize,
    limit: usize,
}

impl AudioReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let url = CFURL::from_path(path, false).unwrap();
        let mut file = match ExtAudioFile::open_url(&url) {
            Ok(file) => file,
            Err(status) => bail!("failed to open: {}", status),
        };

        let mut format = AudioStreamBasicDescription::default();
        if let Err(status) = file.get_property(kExtAudioFileProperty_FileDataFormat, &mut format) {
            bail!("failed to get property: {}", status)
        };

        let frames_per_packet = format.mFramesPerPacket;
        format.mFormatID = kAudioFormatLinearPCM;
        format.mFormatFlags = kAudioFormatFlagsNativeFloatPacked;
        format.mFramesPerPacket = 1;
        format.mBitsPerChannel = 64;
        format.mBytesPerFrame = format.mBitsPerChannel / 8 * format.mChannelsPerFrame;
        format.mBytesPerPacket = format.mFramesPerPacket * format.mBytesPerFrame;
        if let Err(status) = file.set_property(kExtAudioFileProperty_ClientDataFormat, &format) {
            bail!("failed to set property: {}", status)
        };

        let buffer = vec![0.0; (format.mChannelsPerFrame * frames_per_packet) as _];

        Ok(Self {
            file,
            format,
            frames_per_packet,
            buffer,
            pos: 0,
            limit: 0,
        })
    }

    pub fn read(&mut self) -> Result<Option<Vec<f64>>> {
        if self.pos == self.limit {
            let mut buffers = AudioBufferList {
                mNumberBuffers: 1,
                mBuffers: [AudioBuffer {
                    mNumberChannels: self.format.mChannelsPerFrame,
                    mDataByteSize: self.format.mBytesPerFrame * self.frames_per_packet,
                    mData: self.buffer.as_mut_ptr() as _,
                }],
            };
            let frames = match self.file.read(self.frames_per_packet, &mut buffers) {
                Ok(len) => len,
                Err(status) => bail!("failed to read: {}", status),
            };
            self.pos = 0;
            self.limit = frames as _;
        }

        if self.pos < self.limit {
            let index = self.pos * self.format.mChannelsPerFrame as usize;
            let sample =
                self.buffer[index..index + self.format.mChannelsPerFrame as usize].to_vec();
            self.pos += 1;

            return Ok(Some(sample));
        }

        Ok(None)
    }

    pub fn sampling_rate(&self) -> u32 {
        self.format.mSampleRate as _
    }

    pub fn channels(&self) -> usize {
        self.format.mChannelsPerFrame as _
    }
}

struct ExtAudioFile(ExtAudioFileRef);

impl ExtAudioFile {
    fn open_url(url: &CFURL) -> Result<Self, OSStatus> {
        let mut handle = null();
        match unsafe { ExtAudioFileOpenURL(url.as_concrete_TypeRef(), &mut handle) } {
            0 => Ok(Self(handle)),
            status => Err(status),
        }
    }

    fn read(&mut self, max_frames: u32, data: &mut AudioBufferList) -> Result<u32, OSStatus> {
        let mut frames_read = max_frames;
        match unsafe { ExtAudioFileRead(self.0, addr_of_mut!(frames_read), addr_of_mut!(*data)) } {
            0 => Ok(frames_read),
            status => Err(status),
        }
    }

    fn get_property<T>(&self, id: ExtAudioFilePropertyID, data: &mut T) -> Result<u32, OSStatus> {
        let mut size = size_of_val(data) as _;
        match unsafe { ExtAudioFileGetProperty(self.0, id, &mut size, addr_of_mut!(*data) as _) } {
            0 => Ok(size),
            status => Err(status),
        }
    }

    fn set_property<T>(&mut self, id: ExtAudioFilePropertyID, data: &T) -> Result<(), OSStatus> {
        match unsafe {
            ExtAudioFileSetProperty(self.0, id, size_of_val(data) as _, addr_of!(*data) as _)
        } {
            0 => Ok(()),
            status => Err(status),
        }
    }
}

impl Drop for ExtAudioFile {
    fn drop(&mut self) {
        unsafe { ExtAudioFileDispose(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mp3() {
        let r = AudioReader::open("test_data/sample.mp3").unwrap();
        assert_eq!(r.sampling_rate(), 48000);
        assert_eq!(r.channels(), 2);
    }

    #[test]
    fn test_m4a() {
        let r = AudioReader::open("test_data/sample.m4a").unwrap();
        assert_eq!(r.sampling_rate(), 48000);
        assert_eq!(r.channels(), 2);
    }
}

mod ffi {
    #![allow(non_snake_case, non_upper_case_globals)]

    use super::*;
    use std::ffi::c_void;

    #[repr(C)]
    pub struct AudioBuffer {
        pub mNumberChannels: u32,
        pub mDataByteSize: u32,
        pub mData: *mut c_void,
    }

    #[repr(C)]
    pub struct AudioBufferList {
        pub mNumberBuffers: u32,
        pub mBuffers: [AudioBuffer; 1],
    }

    pub type AudioFormatID = u32;
    pub type AudioFormatFlags = u32;

    #[repr(C)]
    #[derive(Default)]
    pub struct AudioStreamBasicDescription {
        pub mSampleRate: f64,
        pub mFormatID: AudioFormatID,
        pub mFormatFlags: u32,
        pub mBytesPerPacket: u32,
        pub mFramesPerPacket: u32,
        pub mBytesPerFrame: u32,
        pub mChannelsPerFrame: u32,
        pub mBitsPerChannel: u32,
        pub mReserved: u32,
    }

    pub const kAudioFormatLinearPCM: AudioFormatID = 1819304813;
    pub const kAudioFormatFlagIsFloat: AudioFormatFlags = 1;
    pub const kAudioFormatFlagIsPacked: AudioFormatFlags = 8;
    pub const kAudioFormatFlagsNativeEndian: AudioFormatFlags = 0;
    pub const kAudioFormatFlagsNativeFloatPacked: AudioFormatFlags =
        kAudioFormatFlagIsFloat | kAudioFormatFlagsNativeEndian | kAudioFormatFlagIsPacked;

    #[repr(C)]
    pub struct OpaqueExtAudioFile(c_void);
    pub type ExtAudioFileRef = *const OpaqueExtAudioFile;

    pub type ExtAudioFilePropertyID = u32;
    pub const kExtAudioFileProperty_FileDataFormat: ExtAudioFilePropertyID = 1717988724;
    pub const kExtAudioFileProperty_ClientDataFormat: ExtAudioFilePropertyID = 1667657076;

    #[link(name = "AudioToolbox", kind = "framework")]
    extern "C" {
        pub fn ExtAudioFileOpenURL(
            inURL: CFURLRef,
            outExtAudioFile: *mut ExtAudioFileRef,
        ) -> OSStatus;
        pub fn ExtAudioFileDispose(inExtAudioFile: ExtAudioFileRef) -> OSStatus;
        pub fn ExtAudioFileRead(
            inExtAudioFile: ExtAudioFileRef,
            ioNumberFrames: *mut u32,
            ioData: *mut AudioBufferList,
        ) -> OSStatus;
        pub fn ExtAudioFileGetProperty(
            inExtAudioFile: ExtAudioFileRef,
            inPropertyID: ExtAudioFilePropertyID,
            ioPropertyDataSize: *mut u32,
            outPropertyData: *mut c_void,
        ) -> OSStatus;
        pub fn ExtAudioFileSetProperty(
            inExtAudioFile: ExtAudioFileRef,
            inPropertyID: ExtAudioFilePropertyID,
            inPropertyDataSize: u32,
            inPropertyData: *const c_void,
        ) -> OSStatus;
    }
}
