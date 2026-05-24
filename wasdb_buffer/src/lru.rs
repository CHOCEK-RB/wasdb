use crate::replacer::ReplacementPolicy;
use parking_lot::Mutex;

/// LRUReplacer implements the Least Recently Used page replacement policy.
/// It uses a queue to track access order. Unpinned frames are eligible for eviction.
pub struct LRUReplacer {
    capacity: usize,
    /// Protects the internal state of the replacer to ensure thread safety.
    inner: Mutex<LruState>,
}

struct LruState {
    /// Frames ordered by access time (most recent at the back).
    /// O(N) operations are acceptable given typical buffer pool sizes for educational DBS.
    access_queue: Vec<usize>,
    /// Flags indicating if a specific frame is pinned and thus unevictable.
    is_pinned: Vec<bool>,
}

impl LRUReplacer {
    /// Creates a new LRU replacer for a buffer pool of the specified capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(LruState {
                access_queue: Vec::with_capacity(capacity),
                is_pinned: vec![false; capacity],
            }),
        }
    }
}

impl ReplacementPolicy for LRUReplacer {
    fn record_access(&self, frame_id: usize) {
        let mut state = self.inner.lock();

        // Remove existing entry if present to update its position.
        if let Some(pos) = state.access_queue.iter().position(|&id| id == frame_id) {
            state.access_queue.remove(pos);
        }

        // Append to the back (most recently used).
        state.access_queue.push(frame_id);
    }

    fn set_pin(&self, frame_id: usize, pinned: bool) {
        let mut state = self.inner.lock();
        if frame_id < self.capacity {
            state.is_pinned[frame_id] = pinned;
        }
    }

    fn evict(&self) -> Option<usize> {
        let mut state = self.inner.lock();

        // Find the first unpinned frame starting from the front (least recently used).
        for (i, &frame_id) in state.access_queue.iter().enumerate() {
            if !state.is_pinned[frame_id] {
                state.access_queue.remove(i);
                return Some(frame_id);
            }
        }
        None
    }

    fn remove(&self, frame_id: usize) {
        let mut state = self.inner.lock();
        if let Some(pos) = state.access_queue.iter().position(|&id| id == frame_id) {
            state.access_queue.remove(pos);
        }
    }

    fn size(&self) -> usize {
        let state = self.inner.lock();
        state.access_queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evict_should_return_least_recently_used() {
        // Arrange
        let lru = LRUReplacer::new(5);
        lru.record_access(1);
        lru.record_access(2);
        lru.record_access(3);
        lru.record_access(1);

        // Act
        let evicted = lru.evict();

        // Assert
        assert_eq!(evicted, Some(2));
    }

    #[test]
    fn size_should_track_number_of_managed_frames() {
        // Arrange
        let lru = LRUReplacer::new(5);
        
        // Act
        lru.record_access(1);
        lru.record_access(2);
        lru.record_access(3);
        lru.record_access(1);

        // Assert
        assert_eq!(lru.size(), 3);
    }

    #[test]
    fn evict_should_ignore_pinned_frames() {
        // Arrange
        let lru = LRUReplacer::new(5);
        lru.record_access(1);
        lru.record_access(2);
        
        // Act
        lru.set_pin(1, true);
        
        // Assert
        assert_eq!(lru.evict(), Some(2));
        assert_eq!(lru.evict(), None); // 1 is pinned and cannot be evicted
    }
}
