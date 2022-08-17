pub mod bs1770;

use anyhow::Result;
use bs1770::{PreFilter, Stats};
use std::path::{Path, PathBuf};

pub trait AudioFile {
    fn path(&self) -> &Path;
    fn save(&self) -> Result<()>;

    fn artist(&self) -> Option<&str>;
    fn album(&self) -> Option<&str>;
    fn compilation(&self) -> bool;
    fn set_normalization(&mut self, val: &str);
}

pub struct Mp3File {
    path: PathBuf,
    tag: id3::Tag,
}

impl Mp3File {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let tag = id3::Tag::read_from_path(&path)?;
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            tag,
        })
    }
}

impl AudioFile for Mp3File {
    fn path(&self) -> &Path {
        &self.path
    }

    fn save(&self) -> Result<()> {
        self.tag.write_to_path(&self.path, id3::Version::Id3v24)?;
        Ok(())
    }

    fn artist(&self) -> Option<&str> {
        use id3::TagLike;
        self.tag.artist()
    }

    fn album(&self) -> Option<&str> {
        use id3::TagLike;
        self.tag.album()
    }

    fn compilation(&self) -> bool {
        use id3::TagLike;
        match self.tag.get("TCMP") {
            Some(f) => match f.content().to_unknown() {
                Ok(u) => match u.data.first() {
                    Some(b) => *b != 0,
                    None => false,
                },
                Err(_) => false,
            },
            None => false,
        }
    }

    fn set_normalization(&mut self, val: &str) {
        use id3::TagLike;
        self.tag.remove_comment(Some("iTunNORM"), None);
        self.tag.add_frame(id3::frame::Comment {
            lang: "eng".to_string(),
            description: "iTunNORM".to_string(),
            text: val.to_string(),
        });
    }
}

pub struct M4aFile {
    path: PathBuf,
    tag: mp4ameta::Tag,
}

impl M4aFile {
    const COMPILATION: mp4ameta::Fourcc = mp4ameta::Fourcc(*b"cpil");

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let tag = mp4ameta::Tag::read_from_path(&path)?;
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            tag,
        })
    }
}

impl AudioFile for M4aFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn save(&self) -> Result<()> {
        self.tag.write_to_path(&self.path)?;
        Ok(())
    }

    fn artist(&self) -> Option<&str> {
        self.tag.artist()
    }

    fn album(&self) -> Option<&str> {
        self.tag.album()
    }

    fn compilation(&self) -> bool {
        match self.tag.data_of(&Self::COMPILATION).next() {
            Some(d) => match d.bytes() {
                Some(b) => b.len() == 1 && b[0] != 0,
                None => false,
            },
            None => false,
        }
    }

    fn set_normalization(&mut self, val: &str) {
        self.tag.add_data(
            mp4ameta::FreeformIdent::new("com.apple.iTunes", "iTunNORM"),
            mp4ameta::Data::Utf8(val.to_string()),
        );
    }
}

pub struct Analyzer {
    filter: PreFilter,
    peak: f64,
}

impl Analyzer {
    pub fn new(sampling_rate: u32, channels: usize) -> Self {
        let mut filter = PreFilter::new(sampling_rate, channels);
        filter.add_block(0.4, 4);

        Self { filter, peak: 0.0 }
    }

    pub fn add_sample(&mut self, sample: &[f64]) {
        self.filter.add_sample(sample);
        self.peak = sample
            .iter()
            .map(|f| f.abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .map(|f| f.max(self.peak))
            .unwrap();
    }

    pub fn flush(self) -> (Stats, f64) {
        (self.filter.flush().pop().unwrap(), self.peak)
    }
}

pub struct Aggregator {
    pub stats: Stats,
    pub peak: f64,
}

impl Aggregator {
    pub fn aggregate(&mut self, stats: &Stats, peak: f64) {
        self.stats.merge(stats);
        self.peak = self.peak.max(peak);
    }
}

impl Default for Aggregator {
    fn default() -> Self {
        Self {
            stats: Stats::new(),
            peak: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_file() {
        let file = Mp3File::open("test_data/sample.mp3").unwrap();
        assert_eq!(file.artist(), Some("Artist"));
        assert_eq!(file.album(), Some("Album"));
        assert_eq!(file.compilation(), true);
    }

    #[test]
    fn m4a_file() {
        let file = M4aFile::open("test_data/sample.m4a").unwrap();
        assert_eq!(file.artist(), Some("Artist"));
        assert_eq!(file.album(), Some("Album"));
        assert_eq!(file.compilation(), true);
    }
}

cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        mod macos;
        pub use self::macos::*;
    } else if #[cfg(target_os = "windows")] {
        mod windows;
        pub use self::windows::*;
    } else if #[cfg(unix)] {
        mod unix;
        pub use self::unix::*;
    } else {
        compile_error!("Unsupported target OS");
    }
}
