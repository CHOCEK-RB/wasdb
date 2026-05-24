use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use wasdb_storage::PageId;

/// Represents the metadata for a single frame of memory in the buffer pool.
pub struct FrameDescriptor {
    /// The page currently residing in this frame, or None if the frame is empty.
    /// Protected by a mutex for interior mutability during frame replacement.
    pub page_id: Mutex<Option<PageId>>,
    /// Number of active readers or writers preventing eviction.
    pub pin_count: AtomicU32,
    /// Indicates if the frame's contents have been modified and require writing to disk.
    pub is_dirty: AtomicBool,
}

impl FrameDescriptor {
    /// Initializes an empty frame descriptor.
    pub fn new() -> Self {
        Self {
            page_id: Mutex::new(None),
            pin_count: AtomicU32::new(0),
            is_dirty: AtomicBool::new(false),
        }
    }

    /// Safely decrements the pin count.
    /// Panics if the pin count is already zero to catch internal accounting bugs.
    pub fn unpin(&self) {
        let prev = self.pin_count.fetch_sub(1, Ordering::SeqCst);
        assert!(prev > 0, "Attempted to unpin a frame with pin_count 0");
    }

    /// Increments the pin count.
    pub fn pin(&self) {
        self.pin_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl Default for FrameDescriptor {
    fn default() -> Self {
        Self::new()
    }
}
