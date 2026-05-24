use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use parking_lot::Mutex;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PageId {
    pub file_id: u32,
    pub page_num: u32,
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Page {page_num} out of bounds in file {file_id}")]
    OutOfBounds { file_id: u32, page_num: u32 },
}

pub trait DiskManager<const PAGE_SIZE: usize>: Send + Sync {
    fn read_page(&self, page_id: PageId, data: &mut [u8; PAGE_SIZE]) -> Result<(), StorageError>;
    fn write_page(&self, page_id: PageId, data: &[u8; PAGE_SIZE]) -> Result<(), StorageError>;
    fn allocate_page(&self, file_id: u32) -> PageId;
    fn deallocate_page(&self, page_id: PageId);
}

pub struct FileHandle {
    pub fd: Mutex<File>,
    pub file_path: PathBuf,
    pub total_pages: AtomicU32,
}

pub struct BasicDiskManager<const PAGE_SIZE: usize> {
    file_handle: FileHandle,
}

impl<const PAGE_SIZE: usize> BasicDiskManager<PAGE_SIZE> {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.as_ref())?;

        let metadata = file.metadata()?;
        let total_pages = (metadata.len() / (PAGE_SIZE as u64)) as u32;

        Ok(Self {
            file_handle: FileHandle {
                fd: Mutex::new(file),
                file_path: path.as_ref().to_path_buf(),
                total_pages: AtomicU32::new(total_pages),
            },
        })
    }
}

impl<const PAGE_SIZE: usize> DiskManager<PAGE_SIZE> for BasicDiskManager<PAGE_SIZE> {
    fn read_page(&self, page_id: PageId, data: &mut [u8; PAGE_SIZE]) -> Result<(), StorageError> {
        let mut file = self.file_handle.fd.lock();
        let offset = (page_id.page_num as u64) * (PAGE_SIZE as u64);
        
        let file_len = file.metadata()?.len();
        if offset >= file_len {
            // When reading a newly allocated page that hasn't been written to, 
            // the file might not be extended yet. We can just zero out the buffer.
            data.fill(0);
            return Ok(());
        }

        file.seek(SeekFrom::Start(offset))?;
        // If the file ends mid-page, read_exact might fail. We read what we can.
        let mut temp_buf = vec![0; PAGE_SIZE];
        let bytes_read = file.read(&mut temp_buf)?;
        data.copy_from_slice(&temp_buf);
        if bytes_read < PAGE_SIZE {
            // zero out the rest
            data[bytes_read..].fill(0);
        }
        
        Ok(())
    }

    fn write_page(&self, page_id: PageId, data: &[u8; PAGE_SIZE]) -> Result<(), StorageError> {
        let mut file = self.file_handle.fd.lock();
        let offset = (page_id.page_num as u64) * (PAGE_SIZE as u64);
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        file.sync_data()?;
        Ok(())
    }

    fn allocate_page(&self, _file_id: u32) -> PageId {
        let page_num = self.file_handle.total_pages.fetch_add(1, Ordering::SeqCst);
        PageId {
            file_id: 0, // Hardcoded for single file usage for now
            page_num,
        }
    }

    fn deallocate_page(&self, _page_id: PageId) {
        // Space reuse is typically handled by a Free Space Map (FSM) in Postgres.
        // For simplicity in this educational SGBD, we leave it as a no-op initially.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn test_read_write_page() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        
        let disk_manager = BasicDiskManager::<TEST_PAGE_SIZE>::new(&path).unwrap();
        
        let page_id = disk_manager.allocate_page(0);
        assert_eq!(page_id.page_num, 0);

        let mut write_data = [0u8; TEST_PAGE_SIZE];
        write_data[0] = 42;
        write_data[TEST_PAGE_SIZE - 1] = 24;

        disk_manager.write_page(page_id, &write_data).unwrap();

        let mut read_data = [0u8; TEST_PAGE_SIZE];
        disk_manager.read_page(page_id, &mut read_data).unwrap();

        assert_eq!(read_data[0], 42);
        assert_eq!(read_data[TEST_PAGE_SIZE - 1], 24);
        assert_eq!(read_data[1..TEST_PAGE_SIZE - 1], [0u8; TEST_PAGE_SIZE - 2]);
    }
}
