use std::mem::size_of;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PageSlot {
    pub offset: u16,
    pub length: u16,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SlottedPageHeader {
    pub log_sequence_number: u64,
    pub total_slots: u16,
    pub free_space_lower: u16,
    pub free_space_upper: u16,
    pub page_flags: u16,
}

pub struct SlottedPage<const PAGE_SIZE: usize> {
    pub data: Box<[u8; PAGE_SIZE]>,
}

impl<const PAGE_SIZE: usize> Default for SlottedPage<PAGE_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const PAGE_SIZE: usize> SlottedPage<PAGE_SIZE> {
    pub fn new() -> Self {
        let mut page = Self {
            data: Box::new([0; PAGE_SIZE]),
        };
        page.init();
        page
    }

    pub fn init(&mut self) {
        let header = SlottedPageHeader {
            log_sequence_number: 0,
            total_slots: 0,
            free_space_lower: size_of::<SlottedPageHeader>() as u16,
            free_space_upper: PAGE_SIZE as u16,
            page_flags: 0,
        };
        unsafe {
            let src = &header as *const _ as *const u8;
            let dst = self.data.as_mut_ptr();
            std::ptr::copy_nonoverlapping(src, dst, size_of::<SlottedPageHeader>());
        }
    }

    pub fn header(&self) -> &SlottedPageHeader {
        unsafe { &*(self.data.as_ptr() as *const SlottedPageHeader) }
    }

    pub fn header_mut(&mut self) -> &mut SlottedPageHeader {
        unsafe { &mut *(self.data.as_mut_ptr() as *mut SlottedPageHeader) }
    }

    pub fn slots(&self) -> &[PageSlot] {
        let header = self.header();
        let start = size_of::<SlottedPageHeader>();
        unsafe {
            std::slice::from_raw_parts(
                self.data[start..].as_ptr() as *const PageSlot,
                header.total_slots as usize,
            )
        }
    }

    pub fn slots_mut(&mut self) -> &mut [PageSlot] {
        let header = self.header();
        let total_slots = header.total_slots;
        let start = size_of::<SlottedPageHeader>();
        unsafe {
            std::slice::from_raw_parts_mut(
                self.data[start..].as_mut_ptr() as *mut PageSlot,
                total_slots as usize,
            )
        }
    }

    pub fn get_record(&self, slot_idx: usize) -> Option<&[u8]> {
        let slots = self.slots();
        if slot_idx >= slots.len() {
            return None;
        }
        let slot = &slots[slot_idx];
        if slot.length == 0 {
            return None;
        }
        let start = slot.offset as usize;
        let end = start + slot.length as usize;
        Some(&self.data[start..end])
    }

    pub fn insert_record(&mut self, record: &[u8]) -> Option<usize> {
        let record_len = record.len();
        let required_space = record_len + size_of::<PageSlot>();

        let mut header_copy = *self.header();
        let free_space = header_copy.free_space_upper - header_copy.free_space_lower;

        if free_space < required_space as u16 {
            return None;
        }

        let slot_idx = header_copy.total_slots as usize;
        let new_offset = header_copy.free_space_upper - record_len as u16;

        // Copy record data backwards
        self.data[new_offset as usize..(new_offset as usize + record_len)].copy_from_slice(record);

        // Update header
        header_copy.total_slots += 1;
        header_copy.free_space_upper = new_offset;
        header_copy.free_space_lower += size_of::<PageSlot>() as u16;

        unsafe {
            let src = &header_copy as *const _ as *const u8;
            let dst = self.data.as_mut_ptr();
            std::ptr::copy_nonoverlapping(src, dst, size_of::<SlottedPageHeader>());
        }

        // Update slots array
        let slots = self.slots_mut();
        slots[slot_idx].offset = new_offset;
        slots[slot_idx].length = record_len as u16;

        Some(slot_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn test_insert_and_get() {
        let mut page = SlottedPage::<TEST_PAGE_SIZE>::new();

        let record1 = b"Hello, World!";
        let slot1 = page.insert_record(record1).unwrap();
        assert_eq!(slot1, 0);

        let record2 = b"Another record";
        let slot2 = page.insert_record(record2).unwrap();
        assert_eq!(slot2, 1);

        assert_eq!(page.get_record(slot1).unwrap(), record1);
        assert_eq!(page.get_record(slot2).unwrap(), record2);

        let header = page.header();
        assert_eq!(header.total_slots, 2);
    }
}
