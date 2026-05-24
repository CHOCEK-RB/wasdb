use crate::replacer::ReplacementPolicy;
use parking_lot::Mutex;

/// ClockReplacer implements the Clock page replacement algorithm.
/// It visualizes frames as a circular buffer (a clock).
/// Each frame has a reference bit. When checking for eviction, it sweeps the clock hand:
/// If the reference bit is 1, it resets it to 0. If 0 and the frame is unpinned, it evicts it.
pub struct ClockReplacer {
    capacity: usize,
    inner: Mutex<ClockState>,
}

struct ClockState {
    /// Tracks if the frame was recently accessed.
    ref_bits: Vec<bool>,
    /// Flags indicating if a frame is pinned.
    is_pinned: Vec<bool>,
    /// Tracks if a frame is actively managed by the clock.
    is_active: Vec<bool>,
    /// The current position of the clock hand.
    clock_hand: usize,
    /// Number of active frames currently loaded.
    size: usize,
}

impl ClockReplacer {
    /// Creates a new Clock replacer.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(ClockState {
                ref_bits: vec![false; capacity],
                is_pinned: vec![false; capacity],
                is_active: vec![false; capacity],
                clock_hand: 0,
                size: 0,
            }),
        }
    }
}

impl ReplacementPolicy for ClockReplacer {
    fn record_access(&self, frame_id: usize) {
        let mut state = self.inner.lock();
        if frame_id < self.capacity {
            state.ref_bits[frame_id] = true;
            if !state.is_active[frame_id] {
                state.is_active[frame_id] = true;
                state.size += 1;
            }
        }
    }

    fn set_pin(&self, frame_id: usize, pinned: bool) {
        let mut state = self.inner.lock();
        if frame_id < self.capacity {
            state.is_pinned[frame_id] = pinned;
            if !state.is_active[frame_id] {
                state.is_active[frame_id] = true;
                state.size += 1;
            }
        }
    }

    fn evict(&self) -> Option<usize> {
        let mut state = self.inner.lock();
        if state.size == 0 {
            return None;
        }

        let start_hand = state.clock_hand;

        loop {
            let current = state.clock_hand;

            if state.is_active[current] && !state.is_pinned[current] {
                if state.ref_bits[current] {
                    // Reset reference bit and move on
                    state.ref_bits[current] = false;
                } else {
                    // Found an unpinned active frame with ref_bit 0. Evict it.
                    state.is_active[current] = false;
                    state.size -= 1;
                    state.clock_hand = (current + 1) % self.capacity;
                    return Some(current);
                }
            }

            state.clock_hand = (current + 1) % self.capacity;

            // If we've done a full circle and found nothing eligible, stop to avoid infinite loop.
            if state.clock_hand == start_hand {
                // It means all active frames are pinned or we just flipped all ref bits to 0.
                // We should loop one more time if we just flipped bits, but to be safe and avoid
                // infinite loops if all are pinned, we count pinned frames.
                let unpinned_count = state
                    .is_active
                    .iter()
                    .zip(state.is_pinned.iter())
                    .filter(|(&active, &pinned)| active && !pinned)
                    .count();
                if unpinned_count == 0 {
                    return None;
                }
            }
        }
    }

    fn remove(&self, frame_id: usize) {
        let mut state = self.inner.lock();
        if frame_id < self.capacity && state.is_active[frame_id] {
            state.ref_bits[frame_id] = false;
            state.is_pinned[frame_id] = false;
            state.is_active[frame_id] = false;
            state.size -= 1;
        }
    }

    fn size(&self) -> usize {
        let state = self.inner.lock();
        state.size
    }
}
