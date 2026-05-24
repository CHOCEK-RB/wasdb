use wasdb_storage::PageId;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("Node is full")]
    NodeFull,
}
pub const INVALID_PAGE_ID: PageId = PageId { file_id: u32::MAX, page_num: u32::MAX };

pub type KeyType = i32;
pub type ValueType = u64; // Typically a Record ID (PageId + SlotIdx)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeType {
    Internal = 1,
    Leaf = 2,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BTreePageHeader {
    pub node_type: u8, // Casted from NodeType
    pub num_keys: u16,
    pub max_keys: u16,
    pub parent_page_id: PageId,
    pub next_page_id: PageId, // For leaf node linking
}

// 8KB page size
pub const MAX_KEYS: usize = 340; 

#[derive(Clone, Copy)]
#[repr(C)]
pub struct LeafNode {
    pub header: BTreePageHeader,
    pub keys: [KeyType; MAX_KEYS],
    pub values: [ValueType; MAX_KEYS],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct InternalNode {
    pub header: BTreePageHeader,
    pub keys: [KeyType; MAX_KEYS],
    pub children: [PageId; MAX_KEYS + 1],
}

impl LeafNode {
    pub fn new(max_keys: u16) -> Self {
        Self {
            header: BTreePageHeader {
                node_type: NodeType::Leaf as u8,
                num_keys: 0,
                max_keys,
                parent_page_id: INVALID_PAGE_ID,
                next_page_id: INVALID_PAGE_ID,
            },
            keys: [0; MAX_KEYS],
            values: [0; MAX_KEYS],
        }
    }

    pub fn insert(&mut self, key: KeyType, value: ValueType) -> Result<(), BTreeError> {
        if self.header.num_keys >= self.header.max_keys {
            return Err(BTreeError::NodeFull); // Node is full
        }
        
        let num_keys = self.header.num_keys as usize;
        let mut idx = 0;
        while idx < num_keys && self.keys[idx] < key {
            idx += 1;
        }

        // Shift elements to make room
        if idx < num_keys {
            self.keys.copy_within(idx..num_keys, idx + 1);
            self.values.copy_within(idx..num_keys, idx + 1);
        }

        self.keys[idx] = key;
        self.values[idx] = value;
        self.header.num_keys += 1;

        Ok(())
    }
}

impl InternalNode {
    pub fn new(max_keys: u16) -> Self {
        Self {
            header: BTreePageHeader {
                node_type: NodeType::Internal as u8,
                num_keys: 0,
                max_keys,
                parent_page_id: INVALID_PAGE_ID,
                next_page_id: INVALID_PAGE_ID,
            },
            keys: [0; MAX_KEYS],
            children: [INVALID_PAGE_ID; MAX_KEYS + 1],
        }
    }
}
