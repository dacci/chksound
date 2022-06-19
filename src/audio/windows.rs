use anyhow::Result;
use once_cell::sync::OnceCell as SyncOnceCell;
use std::path::Path;
use std::ptr::null_mut;
use std::slice::from_raw_parts;
use windows::Win32::Media::MediaFoundation::*;

const MF_VERSION: u32 = MF_SDK_VERSION << 16 | MF_API_VERSION;

static MF: SyncOnceCell<()> = SyncOnceCell::new();

pub struct AudioReader {
    reader: IMFSourceReader,
    sampling_rate: u32,
    channels: usize,
    buffer: Vec<f32>,
    position: usize,
}

impl AudioReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        unsafe {
            MF.get_or_try_init(|| MFStartup(MF_VERSION, MFSTARTUP_LITE))?;

            let reader = MFCreateSourceReaderFromURL(path.as_ref().as_os_str(), None)?;

            let media_type = MFCreateMediaType()?;
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            media_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)?;
            media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 32)?;
            reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as _,
                null_mut(),
                &media_type,
            )?;

            let media_type =
                reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as _)?;
            let sampling_rate = media_type.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
            let channels = media_type.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?;

            Ok(Self {
                reader,
                sampling_rate,
                channels: channels as usize,
                buffer: Vec::new(),
                position: 0,
            })
        }
    }

    pub fn read(&mut self) -> Result<Option<Vec<f64>>> {
        if self.position == self.buffer.len() {
            let mut flags = 0;
            let mut sample = None;
            unsafe {
                self.reader.ReadSample(
                    MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as _,
                    0,
                    null_mut(),
                    &mut flags,
                    null_mut(),
                    &mut sample,
                )?
            };
            if flags as i32 & MF_SOURCE_READERF_ENDOFSTREAM.0 == MF_SOURCE_READERF_ENDOFSTREAM.0 {
                return Ok(None);
            }
            if flags != 0 {
                anyhow::bail!("{flags}");
            }

            let buffer = unsafe { sample.unwrap().ConvertToContiguousBuffer()? };
            let mut pointer: *mut u8 = null_mut();
            let mut length = 0;
            unsafe { buffer.Lock(&mut pointer, null_mut(), &mut length)? };

            self.buffer =
                unsafe { from_raw_parts(pointer as *const f32, (length / 4) as usize).to_vec() };
            self.position = 0;

            unsafe { buffer.Unlock()? };
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
