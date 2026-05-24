use thiserror::Error;

pub mod buffer_pool;
pub mod clock;
pub mod frame;
pub mod lru;
pub mod replacer;

#[derive(Error, Debug)]
pub enum BufferError {
    #[error("All frames are currently pinned")]
    NoFreeFrames,
    #[error("Page not found in buffer pool")]
    PageNotFound,
    #[error("Storage error: {0}")]
    StorageError(#[from] wasdb_storage::StorageError),
}
