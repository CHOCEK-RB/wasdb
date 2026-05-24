
use wasdb_buffer::buffer_pool::BufferPoolManager;
use wasdb_storage::{DiskManager, PageId};

/// Dummy tree module to satisfy compilation for now.
pub struct BTreeIndex<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    _buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
    _root_page_id: PageId,
}
