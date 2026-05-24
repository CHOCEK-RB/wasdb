/// Defines the policies for page eviction in the buffer pool.
pub trait ReplacementPolicy: Send + Sync {
    /// Tracks a frame access, updating its position or state in the eviction candidate list.
    fn record_access(&self, frame_id: usize);

    /// Sets the pin status of a frame.
    /// Pinned frames (pinned = true) cannot be evicted.
    fn set_pin(&self, frame_id: usize, pinned: bool);

    /// Finds a victim frame to evict based on the policy algorithm.
    /// Returns `None` if all tracked frames are pinned and no victim is available.
    fn evict(&self) -> Option<usize>;

    /// Called when a frame is explicitly freed (e.g., page deallocation).
    /// Removes the frame from tracking, allowing it to be reused immediately.
    fn remove(&self, frame_id: usize);

    /// Returns the number of frames currently tracked by the replacer.
    fn size(&self) -> usize;
}
