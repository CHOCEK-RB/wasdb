use wasdb_storage::{PageId, DiskManager};
use std::collections::HashMap;
use std::sync::Arc;
use wasdb_buffer::buffer_pool::BufferPoolManager;
use wasdb_buffer::BufferError;
use wasdb_page::{SlottedPage, TupleHeader};
use parking_lot::RwLock;

/// Represents a tuple/record in the table heap.
pub struct Tuple {
    pub header: TupleHeader,
    pub data: Vec<u8>,
}

/// A lightweight Free Space Map to keep track of how much free space each page has.
pub struct FreeSpaceMap {
    /// Maps page_num -> free_space in bytes.
    /// This is an in-memory approximation.
    pub space_map: HashMap<u32, u16>,
}

impl FreeSpaceMap {
    pub fn new() -> Self {
        Self {
            space_map: HashMap::new(),
        }
    }

    pub fn update(&mut self, page_num: u32, free_space: u16) {
        self.space_map.insert(page_num, free_space);
    }

    pub fn find_page_with_space(&self, required_space: u16) -> Option<u32> {
        for (&page_num, &free_space) in &self.space_map {
            if free_space >= required_space {
                return Some(page_num);
            }
        }
        None
    }
}

/// TableHeap abstracts over a collection of SlottedPages, providing sequential access
/// and insertion capabilities with an FSM for optimization.
pub struct TableHeap<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    buffer_pool: Arc<BufferPoolManager<PAGE_SIZE, D>>,
    file_id: u32,
    first_page_num: u32,
    fsm: RwLock<FreeSpaceMap>,
}

impl<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> TableHeap<PAGE_SIZE, D> {
    pub fn new(
        buffer_pool: Arc<BufferPoolManager<PAGE_SIZE, D>>,
        file_id: u32,
        first_page_num: Option<u32>,
    ) -> Result<Self, BufferError> {
        let first_page = match first_page_num {
            Some(pn) => pn,
            None => {
                // Allocate a new first page for this table heap
                let (frame_id, page_id) = buffer_pool.new_page(file_id)?;
                buffer_pool.unpin_page(page_id, true)?;
                page_id.page_num
            }
        };

        Ok(Self {
            buffer_pool,
            file_id,
            first_page_num: first_page,
            fsm: RwLock::new(FreeSpaceMap::new()),
        })
    }

    pub fn get_first_page_num(&self) -> u32 {
        self.first_page_num
    }

    pub fn get_file_id(&self) -> u32 {
        self.file_id
    }

    /// Insert a tuple into the table heap, returning its CTID: (page_num, slot_index).
    pub fn insert_tuple(&self, tuple: &Tuple) -> Result<(u32, u16), BufferError> {
        // Record size: tuple data + 4 byte slot (in page) + TupleHeader (in tuple data)
        let required_space = (tuple.data.len() + 4) as u16;

        let mut target_page_num = {
            let fsm = self.fsm.read();
            fsm.find_page_with_space(required_space)
        };

        let page_id = if let Some(pn) = target_page_num {
            PageId { file_id: self.file_id, page_num: pn }
        } else {
            // Traverse the linked list or allocate a new page.
            let mut curr_page_num = self.first_page_num;
            let mut last_page_num = curr_page_num;
            let mut found_page = false;

            loop {
                let pid = PageId { file_id: self.file_id, page_num: curr_page_num };
                let frame_id = self.buffer_pool.fetch_page(pid)?;
                let next_page_num = {
                    let page = self.buffer_pool.read_page(frame_id);
                    if page.header().free_space_upper - page.header().free_space_lower >= required_space {
                        found_page = true;
                        page.header().next_page_num
                    } else {
                        page.header().next_page_num
                    }
                };

                if found_page {
                    self.buffer_pool.unpin_page(pid, false)?;
                    target_page_num = Some(curr_page_num);
                    break;
                }

                self.buffer_pool.unpin_page(pid, false)?;

                if next_page_num == u32::MAX {
                    break; // Reached end of list
                }
                last_page_num = curr_page_num;
                curr_page_num = next_page_num;
            }

            if let Some(pn) = target_page_num {
                PageId { file_id: self.file_id, page_num: pn }
            } else {
                // Need to allocate a new page and append it
                let (new_frame_id, new_page_id) = self.buffer_pool.new_page(self.file_id)?;
                
                // Link last page to new page
                let last_pid = PageId { file_id: self.file_id, page_num: last_page_num };
                let last_frame = self.buffer_pool.fetch_page(last_pid)?;
                {
                    let mut last_page = self.buffer_pool.write_page(last_frame);
                    last_page.header_mut().next_page_num = new_page_id.page_num;
                }
                self.buffer_pool.unpin_page(last_pid, true)?;

                self.buffer_pool.unpin_page(new_page_id, false)?; // Just allocated
                new_page_id
            }
        };

        // Insert into the page
        let frame_id = self.buffer_pool.fetch_page(page_id)?;
        let slot_index = {
            let mut page = self.buffer_pool.write_page(frame_id);
            let slot_idx = page.insert_record(&tuple.data, tuple.header.xmin)
                .ok_or(BufferError::NoFreeFrames)?;
            
            // Update FSM
            let free_space = page.header().free_space_upper - page.header().free_space_lower;
            let mut fsm = self.fsm.write();
            fsm.update(page_id.page_num, free_space);

            slot_idx as u16
        };

        self.buffer_pool.unpin_page(page_id, true)?;
        Ok((page_id.page_num, slot_index))
    }

    /// Read a tuple via CTID
    pub fn get_tuple(&self, ctid: (u32, u16)) -> Result<Tuple, BufferError> {
        let (page_num, slot_index) = ctid;
        let page_id = PageId { file_id: self.file_id, page_num };
        
        let frame_id = self.buffer_pool.fetch_page(page_id)?;
        let tuple_res = {
            let page = self.buffer_pool.read_page(frame_id);
            page.get_record(slot_index as usize)
                .map(|(header, data)| Tuple {
                    header,
                    data: data.to_vec(),
                })
        };
        self.buffer_pool.unpin_page(page_id, false)?;

        tuple_res.ok_or(BufferError::PageNotFound)
    }
}
