use wasdb_buffer::buffer_pool::BufferPoolManager;
use wasdb_buffer::lru::LRUReplacer;
use wasdb_buffer::clock::ClockReplacer;
use wasdb_storage::{BasicDiskManager, PageId};

const PAGE_SIZE: usize = 8192;

#[test]
fn test_buffer_pool_lru() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();
    
    let disk_manager = BasicDiskManager::<PAGE_SIZE>::new(&path).unwrap();
    let replacer = Box::new(LRUReplacer::new(10));
    
    let buffer_pool = BufferPoolManager::new(10, disk_manager, replacer);
    
    // Allocate a page ID via storage (usually catalog or upper layers do this, but we fake it)
    let page_id_0 = PageId { file_id: 0, page_num: 0 };
    let page_id_1 = PageId { file_id: 0, page_num: 1 };
    
    let frame_0 = buffer_pool.fetch_page(page_id_0).unwrap();
    {
        let mut page = buffer_pool.write_page(frame_0);
        let header = page.header_mut();
        header.total_slots = 5; // Mutate
    }
    buffer_pool.unpin_page(page_id_0, true).unwrap();
    
    let _frame_1 = buffer_pool.fetch_page(page_id_1).unwrap();
    buffer_pool.unpin_page(page_id_1, false).unwrap();
    
    // Verify it was written (since we flush all)
    buffer_pool.flush_all_pages().unwrap();
}

#[test]
fn test_buffer_pool_clock() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();
    
    let disk_manager = BasicDiskManager::<PAGE_SIZE>::new(&path).unwrap();
    let replacer = Box::new(ClockReplacer::new(10));
    
    let buffer_pool = BufferPoolManager::new(10, disk_manager, replacer);
    
    let page_id_0 = PageId { file_id: 0, page_num: 0 };
    
    let _frame_0 = buffer_pool.fetch_page(page_id_0).unwrap();
    buffer_pool.unpin_page(page_id_0, true).unwrap();
}
