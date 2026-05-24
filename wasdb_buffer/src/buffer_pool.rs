use crate::frame::FrameDescriptor;
use crate::replacer::ReplacementPolicy;
use crate::BufferError;
use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use wasdb_page::SlottedPage;
use wasdb_storage::{DiskManager, PageId};

/// BufferPoolManager orchestrates the fetching and flushing of pages to disk.
pub struct BufferPoolManager<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    _pool_size: usize,
    /// Physical memory pages.
    frames: Vec<RwLock<SlottedPage<PAGE_SIZE>>>,
    /// Metadata for each frame.
    descriptors: Vec<FrameDescriptor>,
    /// Page mapping table to find which frame holds a page.
    page_table: Mutex<HashMap<PageId, usize>>,
    /// Disk manager for fetching/flushing.
    disk_manager: D,
    /// Policy for evicting pages (e.g. LRU, Clock).
    replacer: Box<dyn ReplacementPolicy>,
}

impl<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> BufferPoolManager<PAGE_SIZE, D> {
    pub fn new(pool_size: usize, disk_manager: D, replacer: Box<dyn ReplacementPolicy>) -> Self {
        let mut frames = Vec::with_capacity(pool_size);
        let mut descriptors = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            frames.push(RwLock::new(SlottedPage::<PAGE_SIZE>::new()));
            descriptors.push(FrameDescriptor::new());
        }

        Self {
            _pool_size: pool_size,
            frames,
            descriptors,
            page_table: Mutex::new(HashMap::with_capacity(pool_size)),
            disk_manager,
            replacer,
        }
    }

    /// Finds a free frame, either by finding an empty one or evicting a victim.
    fn find_victim_frame(&self) -> Result<usize, BufferError> {
        // First, look for an empty frame
        for (i, desc) in self.descriptors.iter().enumerate() {
            if desc.page_id.lock().is_none() {
                return Ok(i);
            }
        }

        // If no empty frames, ask the replacer for a victim
        let victim = self.replacer.evict().ok_or(BufferError::NoFreeFrames)?;
        Ok(victim)
    }

    /// Fetches a page from the buffer pool. If it doesn't exist, reads from disk.
    /// The returned frame is already pinned.
    pub fn fetch_page(&self, page_id: PageId) -> Result<usize, BufferError> {
        let mut page_table = self.page_table.lock();

        if let Some(&frame_id) = page_table.get(&page_id) {
            let desc = &self.descriptors[frame_id];
            desc.pin();
            self.replacer.set_pin(frame_id, true);
            self.replacer.record_access(frame_id);
            return Ok(frame_id);
        }

        // Cache miss. Need to find a free frame.
        let frame_id = self.find_victim_frame()?;
        let desc = &self.descriptors[frame_id];
        
        // If the frame has a dirty page, flush it.
        if desc.is_dirty.load(Ordering::SeqCst) {
            let page_id_guard = desc.page_id.lock();
            if let Some(old_page_id) = *page_id_guard {
                let page_data = self.frames[frame_id].read();
                self.disk_manager.write_page(old_page_id, &page_data.data)?;
                desc.is_dirty.store(false, Ordering::SeqCst);
            }
        }

        // Update page table
        {
            let mut page_id_guard = desc.page_id.lock();
            if let Some(old_page_id) = *page_id_guard {
                page_table.remove(&old_page_id);
            }
            *page_id_guard = Some(page_id);
        }
        page_table.insert(page_id, frame_id);

        // Fetch from disk
        {
            let mut page_data = self.frames[frame_id].write();
            self.disk_manager.read_page(page_id, &mut page_data.data)?;
        }

        // Setup descriptor
        desc.pin();
        self.replacer.set_pin(frame_id, true);
        self.replacer.record_access(frame_id);

        Ok(frame_id)
    }

    /// Unpins a page. Marks it as dirty if `is_dirty` is true.
    pub fn unpin_page(&self, page_id: PageId, is_dirty: bool) -> Result<(), BufferError> {
        let page_table = self.page_table.lock();
        if let Some(&frame_id) = page_table.get(&page_id) {
            let desc = &self.descriptors[frame_id];
            if is_dirty {
                desc.is_dirty.store(true, Ordering::SeqCst);
            }
            desc.unpin();
            if desc.pin_count.load(Ordering::SeqCst) == 0 {
                self.replacer.set_pin(frame_id, false);
            }
            Ok(())
        } else {
            Err(BufferError::PageNotFound)
        }
    }

    /// Flushes a specific page to disk.
    pub fn flush_page(&self, page_id: PageId) -> Result<(), BufferError> {
        let page_table = self.page_table.lock();
        if let Some(&frame_id) = page_table.get(&page_id) {
            let desc = &self.descriptors[frame_id];
            let page_data = self.frames[frame_id].read();
            self.disk_manager.write_page(page_id, &page_data.data)?;
            desc.is_dirty.store(false, Ordering::SeqCst);
            Ok(())
        } else {
            Err(BufferError::PageNotFound)
        }
    }

    /// Flushes all dirty pages to disk.
    pub fn flush_all_pages(&self) -> Result<(), BufferError> {
        let _page_table = self.page_table.lock(); // Keep lock to avoid simultaneous evictions
        for (frame_id, desc) in self.descriptors.iter().enumerate() {
            if desc.is_dirty.load(Ordering::SeqCst) {
                if let Some(page_id) = *desc.page_id.lock() {
                    let page_data = self.frames[frame_id].read();
                    self.disk_manager.write_page(page_id, &page_data.data)?;
                    desc.is_dirty.store(false, Ordering::SeqCst);
                }
            }
        }
        Ok(())
    }

    /// Provides read access to the actual slotted page.
    pub fn read_page(&self, frame_id: usize) -> RwLockReadGuard<'_, SlottedPage<PAGE_SIZE>> {
        self.frames[frame_id].read()
    }

    /// Provides write access to the actual slotted page.
    pub fn write_page(&self, frame_id: usize) -> RwLockWriteGuard<'_, SlottedPage<PAGE_SIZE>> {
        self.frames[frame_id].write()
    }

    /// Allocates a new page on disk and brings it into the buffer pool.
    pub fn new_page(&self, file_id: u32) -> Result<(usize, PageId), BufferError> {
        let page_id = self.disk_manager.allocate_page(file_id);
        let frame_id = self.fetch_page(page_id)?;
        Ok((frame_id, page_id))
    }
}
