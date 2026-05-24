use crate::node::{
    BTreePageHeader, InternalNode, KeyType, LeafNode, NodeType, ValueType, INVALID_PAGE_ID,
};
use thiserror::Error;
use wasdb_buffer::buffer_pool::BufferPoolManager;
use wasdb_storage::{DiskManager, PageId};

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
    pub fn new(
        buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
        root_page_id: Option<PageId>,
    ) -> Self {
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
            let max_keys = (PAGE_SIZE - std::mem::size_of::<BTreePageHeader>())
                / (std::mem::size_of::<KeyType>() + std::mem::size_of::<ValueType>());
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
        let (leaf_frame, leaf_page_id) =
            self.find_leaf_page(key)?.ok_or(BTreeError::KeyNotFound)?;

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
        let (leaf_frame_id, curr_page_id) =
            self.find_leaf_page(key)?.ok_or(BTreeError::KeyNotFound)?;

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

impl<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> crate::traits::Index
    for BTreeIndex<'a, PAGE_SIZE, D>
{
    fn insert(
        &self,
        key: i32,
        ctid: wasdb_storage::CTID,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.insert(key, ctid).map_err(|e| e.into())
    }

    fn search(&self, key: i32) -> Result<wasdb_storage::CTID, Box<dyn std::error::Error>> {
        self.search(key).map_err(|e| e.into())
    }

    fn range_search(
        &self,
        start_key: i32,
        end_key: i32,
    ) -> Result<Vec<wasdb_storage::CTID>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        let mut curr_page_id = match self
            .find_leaf_page(start_key)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
        {
            Some((_frame_id, pid)) => pid,
            None => return Ok(results),
        };

        loop {
            let frame_id = self.buffer_pool.fetch_page(curr_page_id)?;
            let page_data = self.buffer_pool.read_page(frame_id);
            let leaf_node = unsafe { &*(page_data.data.as_ptr() as *const crate::node::LeafNode) };
            let num_keys = leaf_node.header.num_keys as usize;

            let mut stop = false;
            for i in 0..num_keys {
                let k = leaf_node.keys[i];
                if k >= start_key && k <= end_key {
                    results.push(leaf_node.values[i]);
                } else if k > end_key {
                    stop = true;
                    break;
                }
            }

            let next_page_id = leaf_node.header.next_page_id;
            drop(page_data);
            self.buffer_pool.unpin_page(curr_page_id, false)?;

            if stop || next_page_id == crate::node::INVALID_PAGE_ID {
                break;
            }
            curr_page_id = next_page_id;
        }

        Ok(results)
    }

    fn delete(&self, key: i32) -> Result<(), Box<dyn std::error::Error>> {
        let (leaf_frame, leaf_page_id) = self
            .find_leaf_page(key)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
            .ok_or_else(|| Box::new(BTreeError::KeyNotFound) as Box<dyn std::error::Error>)?;

        let mut page_data = self.buffer_pool.write_page(leaf_frame);
        let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut crate::node::LeafNode) };
        let num_keys = leaf.header.num_keys as usize;

        let mut delete_idx = None;
        for i in 0..num_keys {
            if leaf.keys[i] == key {
                delete_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = delete_idx {
            // Shift left
            for i in idx..num_keys - 1 {
                leaf.keys[i] = leaf.keys[i + 1];
                leaf.values[i] = leaf.values[i + 1];
            }
            leaf.header.num_keys -= 1;

            let min_keys = leaf.header.max_keys / 2;
            let parent_id = leaf.header.parent_page_id;

            // Complex Deletion check (Merging/Redistribution)
            if leaf.header.num_keys < min_keys && parent_id != crate::node::INVALID_PAGE_ID {
                // We have an underflow and a parent.
                // Note: full robust implementation would lock parent, find sibling, borrow or merge,
                // and recursively update parent.
                // Since this is Nivel 5 educational, we implement the structure of borrowing from a sibling.

                drop(page_data);

                // Fetch parent to find siblings
                if let Ok(parent_frame) = self.buffer_pool.fetch_page(parent_id) {
                    let parent_data = self.buffer_pool.read_page(parent_frame);
                    let parent_node = unsafe {
                        &*(parent_data.data.as_ptr() as *const crate::node::InternalNode)
                    };

                    let mut child_idx = 0;
                    for i in 0..=parent_node.header.num_keys as usize {
                        if parent_node.children[i] == leaf_page_id {
                            child_idx = i;
                            break;
                        }
                    }

                    let left_sibling_id = if child_idx > 0 {
                        Some(parent_node.children[child_idx - 1])
                    } else {
                        None
                    };
                    let right_sibling_id = if child_idx < parent_node.header.num_keys as usize {
                        Some(parent_node.children[child_idx + 1])
                    } else {
                        None
                    };

                    drop(parent_data);
                    self.buffer_pool.unpin_page(parent_id, false)?;

                    let mut handled = false;

                    if let Some(ls_id) = left_sibling_id {
                        let ls_frame = self
                            .buffer_pool
                            .fetch_page(ls_id)
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                        let mut ls_data = self.buffer_pool.write_page(ls_frame);
                        let ls_node = unsafe {
                            &mut *(ls_data.data.as_mut_ptr() as *mut crate::node::LeafNode)
                        };

                        if ls_node.header.num_keys > min_keys {
                            // Borrow from left sibling
                            let borrow_key = ls_node.keys[ls_node.header.num_keys as usize - 1];
                            let borrow_val = ls_node.values[ls_node.header.num_keys as usize - 1];
                            ls_node.header.num_keys -= 1;

                            // Pin our leaf to modify
                            let mut our_data = self.buffer_pool.write_page(leaf_frame);
                            let our_node = unsafe {
                                &mut *(our_data.data.as_mut_ptr() as *mut crate::node::LeafNode)
                            };

                            // Shift our keys right
                            let n = our_node.header.num_keys as usize;
                            for i in (0..n).rev() {
                                our_node.keys[i + 1] = our_node.keys[i];
                                our_node.values[i + 1] = our_node.values[i];
                            }
                            our_node.keys[0] = borrow_key;
                            our_node.values[0] = borrow_val;
                            our_node.header.num_keys += 1;

                            drop(our_data);
                            handled = true;
                        } else {
                            // Merge into left sibling
                            // Pin our leaf to modify
                            let mut our_data = self.buffer_pool.write_page(leaf_frame);
                            let our_node = unsafe {
                                &mut *(our_data.data.as_mut_ptr() as *mut crate::node::LeafNode)
                            };

                            let mut ls_keys = ls_node.header.num_keys as usize;
                            for i in 0..our_node.header.num_keys as usize {
                                ls_node.keys[ls_keys] = our_node.keys[i];
                                ls_node.values[ls_keys] = our_node.values[i];
                                ls_keys += 1;
                            }
                            ls_node.header.num_keys = ls_keys as u16;
                            ls_node.header.next_page_id = our_node.header.next_page_id;

                            // Delete our node entirely (in a real DB, free page, delete key from parent)
                            our_node.header.num_keys = 0;
                            drop(our_data);
                            handled = true;
                        }
                        drop(ls_data);
                        self.buffer_pool
                            .unpin_page(ls_id, true)
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    }

                    if !handled {
                        if let Some(rs_id) = right_sibling_id {
                            // Similar logic for right sibling (omitted for brevity)
                            let _rs_frame = self
                                .buffer_pool
                                .fetch_page(rs_id)
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                            self.buffer_pool
                                .unpin_page(rs_id, false)
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                        }
                    }
                }
                self.buffer_pool.unpin_page(leaf_page_id, true)?;
                return Ok(());
            }

            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, true)?;
            Ok(())
        } else {
            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, false)?;
            Err(Box::new(BTreeError::KeyNotFound))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use wasdb_buffer::lru::LRUReplacer;
    use wasdb_storage::BasicDiskManager;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn insert_should_succeed_for_new_key() {
        let temp_file = NamedTempFile::new().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(temp_file.path()).unwrap();
        let buffer = BufferPoolManager::new(10, disk, Box::new(LRUReplacer::new(10)));
        let btree = BTreeIndex::new(&buffer, None);

        let ctid = wasdb_storage::CTID::default();
        assert!(btree.insert(1, ctid).is_ok());
    }

    #[test]
    fn search_should_return_value_for_existing_key() {
        let temp_file = NamedTempFile::new().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(temp_file.path()).unwrap();
        let buffer = BufferPoolManager::new(10, disk, Box::new(LRUReplacer::new(10)));
        let btree = BTreeIndex::new(&buffer, None);

        let ctid = wasdb_storage::CTID {
            slot_idx: 100,
            ..Default::default()
        };
        btree.insert(1, ctid).unwrap();
        assert_eq!(btree.search(1).unwrap(), ctid);
    }

    #[test]
    fn insert_should_return_error_for_duplicate_key() {
        let temp_file = NamedTempFile::new().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(temp_file.path()).unwrap();
        let buffer = BufferPoolManager::new(10, disk, Box::new(LRUReplacer::new(10)));
        let btree = BTreeIndex::new(&buffer, None);

        let ctid = wasdb_storage::CTID::default();
        btree.insert(2, ctid).unwrap();
        assert!(matches!(
            btree.insert(2, ctid),
            Err(BTreeError::DuplicateKey)
        ));
    }

    #[test]
    fn search_should_return_error_for_missing_key() {
        let temp_file = NamedTempFile::new().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(temp_file.path()).unwrap();
        let buffer = BufferPoolManager::new(10, disk, Box::new(LRUReplacer::new(10)));
        let btree = BTreeIndex::new(&buffer, None);

        assert!(matches!(btree.search(99), Err(BTreeError::KeyNotFound)));
    }
}
