pub mod high_level;
mod low_level;

#[derive(thiserror::Error, Debug)]
pub enum MapError {
    #[error("IO Error {0:?}")]
    IOError(std::io::Error),

    #[error("Map cannot be found")]
    NotFound,
    #[error("MAP write error, verification failed")]
    WriteError,
    #[error("MAP read error, first byte of response is 0x65")]
    FirstByteis65DontKnowWhatItMeans,
    #[error("MAP read error, first byte of response is not 0x6f, but {0}")]
    UnknownError(u8),
    #[error("MAP read error, checksum failed {0}")]
    ChecksumFailed(u8),
}
