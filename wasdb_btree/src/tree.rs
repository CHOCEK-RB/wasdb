use crate::node::{BTreePageHeader, InternalNode, KeyType, LeafNode, NodeType, ValueType, INVALID_PAGE_ID};
use wasdb_buffer::buffer_pool::BufferPoolManager;
use wasdb_storage::{DiskManager, PageId};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("Buffer pool error: {0}")]
    BufferError(#[from] wasdb_buffer::BufferError),
    #[error("Key not found")]
    KeyNotFound,
    #[error("Duplicate key")]
    DuplicateKey,
    #[error("Root page not initialized")]
    Uninitialized,
}

pub struct BTreeIndex<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
    root_page_id: parking_lot::RwLock<Option<PageId>>,
}

impl<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> BTreeIndex<'a, PAGE_SIZE, D> {
    pub fn new(buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>, root_page_id: Option<PageId>) -> Self {
        Self {
            buffer_pool,
            root_page_id: parking_lot::RwLock::new(root_page_id),
        }
    }

    /// Finds the leaf page that should contain the given key.
    /// Returns the frame_id of the leaf page pinned in the buffer pool.
    fn find_leaf_page(&self, key: KeyType) -> Result<Option<(usize, PageId)>, BTreeError> {
        let root_id = *self.root_page_id.read();
        let mut curr_page_id = match root_id {
            Some(id) => id,
            None => return Ok(None),
        };

        loop {
            let frame_id = self.buffer_pool.fetch_page(curr_page_id)?;
            let page_data = self.buffer_pool.read_page(frame_id);
            let header = unsafe { &*(page_data.data.as_ptr() as *const BTreePageHeader) };

            if header.node_type == NodeType::Leaf as u8 {
                return Ok(Some((frame_id, curr_page_id)));
            }

            // It's an internal node
            let internal_node = unsafe { &*(page_data.data.as_ptr() as *const InternalNode) };
            
            // Binary search or linear search for the correct child pointer
            let num_keys = header.num_keys as usize;
            let mut child_idx = num_keys;
            for i in 0..num_keys {
                if key < internal_node.keys[i] {
                    child_idx = i;
                    break;
                }
            }
            
            let next_page_id = internal_node.children[child_idx];
            
            // Drop read lock and unpin current page before fetching next to avoid deadlocks
            drop(page_data);
            self.buffer_pool.unpin_page(curr_page_id, false)?;
            
            curr_page_id = next_page_id;
        }
    }

    /// Searches for a value by key.
    pub fn search(&self, key: KeyType) -> Result<ValueType, BTreeError> {
        let (leaf_frame_id, curr_page_id) = self.find_leaf_page(key)?
            .ok_or(BTreeError::KeyNotFound)?;

        let page_data = self.buffer_pool.read_page(leaf_frame_id);
        let leaf_node = unsafe { &*(page_data.data.as_ptr() as *const LeafNode) };
        
        let mut found_val = None;
        let num_keys = leaf_node.header.num_keys as usize;
        
        // Binary search for exact key
        if let Ok(idx) = leaf_node.keys[..num_keys].binary_search(&key) {
            found_val = Some(leaf_node.values[idx]);
        }

        drop(page_data);
        self.buffer_pool.unpin_page(curr_page_id, false)?;
        
        found_val.ok_or(BTreeError::KeyNotFound)
    }
}
