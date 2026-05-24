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

    pub fn insert(&self, key: KeyType, value: ValueType) -> Result<(), BTreeError> {
        let mut root_lock = self.root_page_id.write();
        let root_id = *root_lock;
        if root_id.is_none() {
            // Create root leaf page
            let (frame_id, root_page_id) = self.buffer_pool.new_page(0)?;
            let mut page_data = self.buffer_pool.write_page(frame_id);
            let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut LeafNode) };
            
            leaf.header.node_type = NodeType::Leaf as u8;
            leaf.header.num_keys = 0;
            let max_keys = (PAGE_SIZE - std::mem::size_of::<BTreePageHeader>()) / (std::mem::size_of::<KeyType>() + std::mem::size_of::<ValueType>());
            leaf.header.max_keys = max_keys as u16;
            leaf.header.parent_page_id = INVALID_PAGE_ID;
            leaf.header.next_page_id = INVALID_PAGE_ID;

            leaf.keys[0] = key;
            leaf.values[0] = value;
            leaf.header.num_keys = 1;

            *root_lock = Some(root_page_id);

            drop(page_data);
            self.buffer_pool.unpin_page(root_page_id, true)?;
            return Ok(());
        }
        drop(root_lock);

        // Find leaf
        let (leaf_frame, leaf_page_id) = self.find_leaf_page(key)?.unwrap();
        
        // Pin leaf for writing
        let mut page_data = self.buffer_pool.write_page(leaf_frame);
        let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut LeafNode) };

        let num_keys = leaf.header.num_keys as usize;
        
        // Check duplicate
        if leaf.keys[..num_keys].binary_search(&key).is_ok() {
            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, false)?;
            return Err(BTreeError::DuplicateKey);
        }

        // We can just try to insert
        if num_keys < leaf.header.max_keys as usize {
            // Find pos
            let mut insert_idx = num_keys;
            for i in 0..num_keys {
                if leaf.keys[i] > key {
                    insert_idx = i;
                    break;
                }
            }
            
            // Shift
            for i in (insert_idx..num_keys).rev() {
                leaf.keys[i + 1] = leaf.keys[i];
                leaf.values[i + 1] = leaf.values[i];
            }
            leaf.keys[insert_idx] = key;
            leaf.values[insert_idx] = value;
            leaf.header.num_keys += 1;

            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, true)?;
            return Ok(());
        }

        // Split required!
        // (Implementation omitted for brevity. You can implement full split later)
        // This is a naive implementation that panics on split for now just to prove it compiles and works for small data.
        drop(page_data);
        self.buffer_pool.unpin_page(leaf_page_id, false)?;
        unimplemented!("B+ Tree page splitting is not yet implemented.");
    }

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

            let internal_node = unsafe { &*(page_data.data.as_ptr() as *const InternalNode) };
            let num_keys = header.num_keys as usize;
            let mut child_idx = num_keys;
            for i in 0..num_keys {
                if key < internal_node.keys[i] {
                    child_idx = i;
                    break;
                }
            }
            let next_page_id = internal_node.children[child_idx];
            
            drop(page_data);
            self.buffer_pool.unpin_page(curr_page_id, false)?;
            curr_page_id = next_page_id;
        }
    }

    pub fn search(&self, key: KeyType) -> Result<ValueType, BTreeError> {
        let (leaf_frame_id, curr_page_id) = self.find_leaf_page(key)?
            .ok_or(BTreeError::KeyNotFound)?;

        let page_data = self.buffer_pool.read_page(leaf_frame_id);
        let leaf_node = unsafe { &*(page_data.data.as_ptr() as *const LeafNode) };
        
        let mut found_val = None;
        let num_keys = leaf_node.header.num_keys as usize;
        
        if let Ok(idx) = leaf_node.keys[..num_keys].binary_search(&key) {
            found_val = Some(leaf_node.values[idx]);
        }

        drop(page_data);
        self.buffer_pool.unpin_page(curr_page_id, false)?;
        found_val.ok_or(BTreeError::KeyNotFound)
    }
}
