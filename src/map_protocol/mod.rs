use thiserror::Error;

pub mod high_level;
mod low_level;

#[derive(Error, Debug)]
pub enum MapError {
    #[error("IO Error")]
    IOError(#[from] std::io::Error),
    #[error("Map cannot be found")]
    NotFound,
    #[error("MAP verify write error: {count} bytes read")]
    VerifyReadAfterWriteError { count: usize },
    #[error("MAP verify write error: no correct bytes after 20 bytes skipped")]
    VerifyReadAfterWriteRunawayError,
    #[error("MAP write error, verification failed")]
    WriteError,
    #[error("MAP read error, first byte of response is 0x65")]
    FirstByteis65DontKnowWhatItMeans,
    #[error("MAP read error, first byte of response is not 0x6f, but {value}")]
    UnknownValueError { value: u8 },
    #[error("MAP read error, checksum failed {value}")]
    ChecksumFailed { value: u8 },
}
