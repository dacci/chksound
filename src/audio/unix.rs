use anyhow::Result;
use once_cell::sync::OnceCell as SyncOnceCell;
use std::path::Path;

static MPG123: SyncOnceCell<()> = SyncOnceCell::new();

pub struct AudioReader {
    handle: mpg123::Handle,
    sampling_rate: u32,
    channels: usize,
    buffer: Vec<f32>,
    position: usize,
}

impl AudioReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        MPG123.get_or_try_init(mpg123::init)?;

        let handle = mpg123::Handle::new()?;
        handle.open(path)?;
        let (sampling_rate, channels, _) = handle.format()?;

        Ok(Self {
            handle,
            sampling_rate: sampling_rate as _,
            channels: channels as _,
            buffer: Vec::new(),
            position: 0,
        })
    }

    pub fn read(&mut self) -> Result<Option<Vec<f64>>> {
        if self.buffer.len() == self.position {
            self.buffer = match self.handle.decode_frame() {
                Ok(Some(buffer)) => buffer.to_vec(),
                Ok(None) => return Ok(None),
                Err(e) => anyhow::bail!(e),
            };
            self.position = 0;
        }

        if self.position < self.buffer.len() {
            let sample = self.buffer[self.position..self.position + self.channels]
                .iter()
                .map(|s| *s as f64)
                .collect();
            self.position += self.channels;
            Ok(Some(sample))
        } else {
            Ok(None)
        }
    }

    pub fn sampling_rate(&self) -> u32 {
        self.sampling_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mp3() {
        let mut r = AudioReader::open("test_data/sample.mp3").unwrap();
        assert_eq!(r.sampling_rate(), 48000);
        assert_eq!(r.channels(), 2);
    }
}

mod mpg123 {
    #![allow(non_camel_case_types)]

    use libc::{c_char, c_int, c_long, c_uchar, c_void, off_t};
    use std::ffi::{CStr, CString};
    use std::fmt;
    use std::path::Path;
    use std::ptr::{null, null_mut};
    use std::slice::from_raw_parts;

    #[repr(C)]
    struct mpg123_handle_struct(c_void);
    type mpg123_handle = *mut mpg123_handle_struct;

    const MPG123_DONE: i32 = -12;
    const MPG123_OK: i32 = 0;

    #[link(name = "mpg123")]
    extern "C" {
        fn mpg123_init() -> c_int;

        fn mpg123_new(decoder: *const c_char, error: *mut c_int) -> mpg123_handle;
        fn mpg123_delete(mh: mpg123_handle);

        fn mpg123_plain_strerror(errcode: c_int) -> *const c_char;

        fn mpg123_getformat(
            mh: mpg123_handle,
            rate: *mut c_long,
            channels: *mut c_int,
            encoding: *mut c_int,
        ) -> c_int;

        fn mpg123_open(mh: mpg123_handle, path: *const c_char) -> c_int;

        fn mpg123_decode_frame(
            mh: mpg123_handle,
            num: *mut off_t,
            audio: *mut *mut c_uchar,
            bytes: *mut usize,
        ) -> c_int;
    }

    #[derive(Debug)]
    pub struct Error(c_int);

    impl From<c_int> for Error {
        fn from(error: c_int) -> Self {
            Self(error)
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let msg = unsafe { CStr::from_ptr(mpg123_plain_strerror(self.0)) };
            write!(f, "{}", msg.to_string_lossy())
        }
    }

    impl std::error::Error for Error {}

    pub type Result<T, E = Error> = std::result::Result<T, E>;

    pub fn init() -> Result<()> {
        match unsafe { mpg123_init() } {
            MPG123_OK => Ok(()),
            error => Err(error.into()),
        }
    }

    pub struct Handle(mpg123_handle);

    impl Handle {
        const NULL: mpg123_handle = null_mut();

        pub fn new() -> Result<Self> {
            let mut error = MPG123_OK;
            match unsafe { mpg123_new(null(), &mut error) } {
                Self::NULL => Err(error.into()),
                handle => Ok(Self(handle)),
            }
        }

        pub fn open(&self, path: impl AsRef<Path>) -> Result<()> {
            let path = CString::new(path.as_ref().to_str().unwrap()).unwrap();
            match unsafe { mpg123_open(self.0, path.as_ptr()) } {
                MPG123_OK => Ok(()),
                error => Err(error.into()),
            }
        }

        pub fn format(&self) -> Result<(i64, i32, i32)> {
            let mut rate = 0;
            let mut channels = 0;
            let mut encoding = 0;
            match unsafe { mpg123_getformat(self.0, &mut rate, &mut channels, &mut encoding) } {
                MPG123_OK => Ok((rate, channels, encoding)),
                error => Err(error.into()),
            }
        }

        pub fn decode_frame(&self) -> Result<Option<&[f32]>> {
            let mut audio = null_mut();
            let mut bytes = 0;
            match unsafe { mpg123_decode_frame(self.0, null_mut(), &mut audio, &mut bytes) } {
                MPG123_OK => Ok(Some(unsafe {
                    from_raw_parts(audio as *const f32, bytes / 4)
                })),
                MPG123_DONE => Ok(None),
                error => Err(error.into()),
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { mpg123_delete(self.0) }
        }
    }
}
