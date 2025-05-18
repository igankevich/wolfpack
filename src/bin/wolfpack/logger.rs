use std::io::stderr;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::OnceLock;

use indicatif::MultiProgress;
use log::set_logger;
use log::set_max_level;
use log::Level;
use log::LevelFilter;
use log::Log;
use log::Metadata;
use log::Record;
use log::SetLoggerError;
use parking_lot::Mutex;

pub struct Logger;

impl Logger {
    pub fn init(max_level: LevelFilter) -> Result<(), SetLoggerError> {
        set_logger(LOGGER.get_or_init(move || Logger)).map(|()| set_max_level(max_level))
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if metadata.level() <= Level::Info && metadata.target().contains("tantivy") {
            // Silence tantivy.
            return false;
        }
        true
    }

    fn log(&self, record: &Record) {
        use std::fmt::Write;
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut buffer = String::with_capacity(4096);
        let prefix = match record.level() {
            Level::Warn => "WARNING: ",
            _ => "",
        };
        let _ = writeln!(&mut buffer, "{prefix}{}", record.args());
        let logger = LOGGER_MUT.lock();
        if logger.active {
            let _ = logger.progress.println(buffer);
        } else {
            use std::io::Write;
            let _ = stderr().write_all(buffer.as_bytes());
        }
    }

    fn flush(&self) {
        use std::io::Write;
        let _ = stderr().flush();
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn get_logger() -> Arc<Mutex<LoggerMut>> {
    LOGGER_MUT.clone()
}

pub struct LoggerMut {
    progress: MultiProgress,
    active: bool,
}

impl LoggerMut {
    pub fn finish(&mut self) {
        let _ = self.progress.clear();
        self.active = false;
    }

    pub fn start(&mut self) -> ProgressGuard {
        let _ = self.progress.clear();
        self.active = true;
        ProgressGuard
    }

    pub fn progress_mut(&mut self) -> &mut MultiProgress {
        &mut self.progress
    }
}

static LOGGER_MUT: LazyLock<Arc<Mutex<LoggerMut>>> = LazyLock::new(|| {
    Arc::new(Mutex::new(LoggerMut {
        progress: MultiProgress::new(),
        active: false,
    }))
});

pub struct ProgressGuard;

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        LOGGER_MUT.lock().finish();
    }
}
