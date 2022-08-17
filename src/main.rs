mod audio;

use audio::{bs1770::Stats, Aggregator, Analyzer, AudioFile, AudioReader, M4aFile, Mp3File};
use clap::Parser;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

#[derive(Parser)]
struct Args {
    /// Files or directories to analyze.
    paths: Vec<PathBuf>,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    match run(Args::parse()) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

struct Entry {
    file: Box<dyn AudioFile + Send>,
    aggregator: Option<Arc<Mutex<Aggregator>>>,
    stats: Option<Stats>,
    peak: Option<f64>,
}

impl Entry {
    fn new(file: Box<dyn AudioFile + Send>, aggregator: Option<Arc<Mutex<Aggregator>>>) -> Self {
        Self {
            file,
            aggregator,
            stats: None,
            peak: None,
        }
    }
}

fn adjust_gain(gain: f64, base: f64) -> i32 {
    (10.0_f64.powf(-gain / 10.0) * base).round().min(65534.0) as i32
}

fn run(args: Args) -> Result<()> {
    let para = thread::available_parallelism()?.get();
    let (tx1, rx1) = bounded(para);
    let (tx2, rx2) = unbounded();
    let mut threads = Vec::with_capacity(para);
    for _ in 0..para {
        let rx = rx1.clone();
        let tx = tx2.clone();
        threads.push(thread::spawn(|| analyzer(rx, tx)));
    }
    drop(rx1);
    drop(tx2);

    let mut map = HashMap::new();
    for path in &args.paths {
        process(path, &mut map, &tx1);
    }
    drop(tx1);

    for thread in threads {
        thread.join().unwrap();
    }

    for mut entry in rx2.iter() {
        let track_gain = entry.stats.unwrap().get_mean(-10.0).to_gain();
        let track_peak = (entry.peak.unwrap() * 32768.0) as i32;

        let (album_gain, album_peak) = if let Some(ref aggregator) = entry.aggregator {
            let guard = aggregator.lock().unwrap();
            let gain = guard.stats.get_mean(-10.0).to_gain();
            let peak = (guard.peak * 32768.0) as i32;
            (gain, peak)
        } else {
            (track_gain, track_peak)
        };

        let normalization = format!(
            " {:08X} {:08X} {:08X} {:08X} 00000000 00000000 {:08X} {:08X} 00000000 00000000",
            adjust_gain(track_gain, 1000.0),
            adjust_gain(album_gain, 1000.0),
            adjust_gain(track_gain, 2500.0),
            adjust_gain(album_gain, 2500.0),
            track_peak,
            album_peak
        );
        entry.file.set_normalization(&normalization);

        if let Err(e) = entry.file.save() {
            log::error!("{}: {e}", entry.file.path().display());
        }
    }

    Ok(())
}

fn process(path: &Path, map: &mut HashMap<String, Arc<Mutex<Aggregator>>>, tx: &Sender<Entry>) {
    let res = if path.is_dir() {
        process_dir(path, map, tx)
    } else {
        process_file(path, map, tx)
    };

    if let Err(e) = res {
        log::error!("{}: {e}", path.display())
    }
}

fn process_dir(
    path: &Path,
    map: &mut HashMap<String, Arc<Mutex<Aggregator>>>,
    tx: &Sender<Entry>,
) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        process(&path, map, tx);
    }

    Ok(())
}

fn process_file(
    path: &Path,
    map: &mut HashMap<String, Arc<Mutex<Aggregator>>>,
    tx: &Sender<Entry>,
) -> Result<()> {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => ext.to_lowercase(),
        _ => return Ok(()),
    };

    let file: Box<dyn AudioFile + Send> = match ext.as_str() {
        "mp3" => Box::new(Mp3File::open(path)?),
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        "m4a" => Box::new(M4aFile::open(path)?),
        _ => return Ok(()),
    };

    let aggregator = if !file.compilation() {
        if let Some(artist) = file.artist() {
            if let Some(album) = file.album() {
                let group = format!("{}\0{}", artist, album);
                let aggregator = map.entry(group).or_insert_with(Default::default);
                Some(Arc::clone(aggregator))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    tx.send(Entry::new(file, aggregator))?;

    Ok(())
}

fn analyzer(rx: Receiver<Entry>, tx: Sender<Entry>) {
    'recv: for mut entry in rx.iter() {
        let mut reader = match AudioReader::open(entry.file.path()) {
            Ok(reader) => reader,
            Err(e) => {
                log::error!("{}: {e}", entry.file.path().display());
                continue;
            }
        };

        let mut analyzer = Analyzer::new(reader.sampling_rate(), reader.channels());
        let (stats, peak) = loop {
            match reader.read() {
                Ok(sample) => match sample {
                    Some(sample) => analyzer.add_sample(&sample),
                    None => break analyzer.flush(),
                },
                Err(e) => {
                    log::error!("{}: {e}", entry.file.path().display());
                    continue 'recv;
                }
            }
        };

        if let Some(ref aggregator) = entry.aggregator {
            aggregator.lock().unwrap().aggregate(&stats, peak);
        }

        let loudness = stats.get_mean(-10.0);

        entry.stats = Some(stats);
        entry.peak = Some(peak);
        log::info!("{}: {}", entry.file.path().display(), loudness);

        let _ = tx.send(entry);
    }
}
