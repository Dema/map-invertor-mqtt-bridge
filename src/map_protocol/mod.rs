use snafu::{Backtrace, Snafu};

pub mod high_level;
mod low_level;

#[derive(Snafu, Debug)]
#[snafu(visibility(pub(crate)))]
pub enum MapError {
    #[snafu(display("IO Error"))]
    IOError {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("Map cannot be found"))]
    NotFound { backtrace: Backtrace },
    #[snafu(display("MAP verify write error: {count} bytes read"))]
    VerifyReadAfterWriteError { backtrace: Backtrace, count: usize },
    #[snafu(display("MAP verify write error: no correct bytes after 20 bytes skipped"))]
    VerifyReadAfterWriteRunawayError { backtrace: Backtrace },
    #[snafu(display("MAP write error, verification failed"))]
    WriteError { backtrace: Backtrace },
    #[snafu(display("MAP read error, first byte of response is 0x65"))]
    FirstByteis65DontKnowWhatItMeans { backtrace: Backtrace },
    #[snafu(display("MAP read error, first byte of response is not 0x6f, but {value}"))]
    UnknownValueError { value: u8, backtrace: Backtrace },
    #[snafu(display("MAP read error, checksum failed {value}"))]
    ChecksumFailed { value: u8, backtrace: Backtrace },
}
