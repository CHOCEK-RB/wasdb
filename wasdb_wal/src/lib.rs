use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use wasdb_storage::PageId;
use wasdb_tx::TransactionId;

pub type LogSequenceNumber = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogRecord {
    Insert {
        txn_id: TransactionId,
        page_id: PageId,
        offset: u16,
        data: Vec<u8>,
    },
    Commit {
        txn_id: TransactionId,
    },
    Abort {
        txn_id: TransactionId,
    },
}

impl LogRecord {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            LogRecord::Insert {
                txn_id,
                page_id,
                offset,
                data,
            } => {
                buf.push(1); // Type 1 = Insert
                buf.extend_from_slice(&txn_id.to_le_bytes());
                buf.extend_from_slice(&page_id.file_id.to_le_bytes());
                buf.extend_from_slice(&page_id.page_num.to_le_bytes());
                buf.extend_from_slice(&offset.to_le_bytes());
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(data);
            }
            LogRecord::Commit { txn_id } => {
                buf.push(2); // Type 2 = Commit
                buf.extend_from_slice(&txn_id.to_le_bytes());
            }
            LogRecord::Abort { txn_id } => {
                buf.push(3); // Type 3 = Abort
                buf.extend_from_slice(&txn_id.to_le_bytes());
            }
        }
        buf
    }

    pub fn deserialize(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.is_empty() {
            return None;
        }
        match buf[0] {
            1 => {
                if buf.len() < 25 {
                    return None;
                }
                let txn_id = u64::from_le_bytes(buf[1..9].try_into().ok()?);
                let file_id = u32::from_le_bytes(buf[9..13].try_into().ok()?);
                let page_num = u32::from_le_bytes(buf[13..17].try_into().ok()?);
                let offset = u16::from_le_bytes(buf[17..19].try_into().ok()?);
                let len = u32::from_le_bytes(buf[19..23].try_into().ok()?) as usize;
                if buf.len() < 23 + len {
                    return None;
                }
                let data = buf[23..23 + len].to_vec();
                Some((
                    LogRecord::Insert {
                        txn_id,
                        page_id: PageId { file_id, page_num },
                        offset,
                        data,
                    },
                    23 + len,
                ))
            }
            2 => {
                if buf.len() < 9 {
                    return None;
                }
                let txn_id = u64::from_le_bytes(buf[1..9].try_into().ok()?);
                Some((LogRecord::Commit { txn_id }, 9))
            }
            3 => {
                if buf.len() < 9 {
                    return None;
                }
                let txn_id = u64::from_le_bytes(buf[1..9].try_into().ok()?);
                Some((LogRecord::Abort { txn_id }, 9))
            }
            _ => None,
        }
    }
}

pub struct LogManager {
    file: Mutex<File>,
    next_lsn: AtomicU64,
    flushed_lsn: AtomicU64,
}

impl LogManager {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;

        let metadata = file.metadata()?;
        // A real system would scan the WAL to find the last LSN. We simulate by file length.
        let file_len = metadata.len();

        Ok(Self {
            file: Mutex::new(file),
            next_lsn: AtomicU64::new(file_len + 1),
            flushed_lsn: AtomicU64::new(file_len),
        })
    }

    pub fn append_record(&self, record: &LogRecord) -> io::Result<LogSequenceNumber> {
        let serialized = record.serialize();
        // A record format in file: [Size: u32][LSN: u64][Data...]
        let size = serialized.len() as u32;

        let mut file = self.file.lock();
        let lsn = self.next_lsn.fetch_add(1, Ordering::SeqCst);

        file.write_all(&size.to_le_bytes())?;
        file.write_all(&lsn.to_le_bytes())?;
        file.write_all(&serialized)?;

        Ok(lsn)
    }

    pub fn flush(&self) -> io::Result<()> {
        let file = self.file.lock();
        file.sync_data()?;
        let current_next = self.next_lsn.load(Ordering::SeqCst);
        self.flushed_lsn.store(current_next - 1, Ordering::SeqCst);
        Ok(())
    }

    pub fn get_flushed_lsn(&self) -> LogSequenceNumber {
        self.flushed_lsn.load(Ordering::SeqCst)
    }
}

pub struct RecoveryManager;

impl RecoveryManager {
    pub fn recover<P: AsRef<Path>>(path: P) -> io::Result<Vec<LogRecord>> {
        let mut file = OpenOptions::new().read(true).open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut records = Vec::new();
        let mut cursor = 0;

        while cursor + 12 <= buf.len() {
            let size_bytes: [u8; 4] = buf[cursor..cursor + 4]
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Corrupted WAL size"))?;
            let size = u32::from_le_bytes(size_bytes) as usize;

            let lsn_bytes: [u8; 8] = buf[cursor + 4..cursor + 12]
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Corrupted WAL LSN"))?;
            let _lsn = u64::from_le_bytes(lsn_bytes);
            cursor += 12;

            if cursor + size > buf.len() {
                break; // Incomplete record
            }

            if let Some((record, _)) = LogRecord::deserialize(&buf[cursor..cursor + size]) {
                records.push(record);
            }
            cursor += size;
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn serialize_deserialize_insert_should_match() {
        let record = LogRecord::Insert {
            txn_id: 123,
            page_id: PageId {
                file_id: 1,
                page_num: 2,
            },
            offset: 50,
            data: vec![0, 1, 2, 3],
        };
        let serialized = record.serialize();
        let (deserialized, size) = LogRecord::deserialize(&serialized).unwrap();

        assert_eq!(record, deserialized);
        assert_eq!(size, serialized.len());
    }

    #[test]
    fn log_manager_should_append_and_flush_records() {
        let temp = NamedTempFile::new().unwrap();
        let lm = LogManager::new(temp.path()).unwrap();

        let record = LogRecord::Commit { txn_id: 42 };
        let lsn = lm.append_record(&record).unwrap();

        assert!(lsn > 0);
        lm.flush().unwrap();
        assert_eq!(lm.get_flushed_lsn(), lsn);
    }

    #[test]
    fn recovery_manager_should_read_appended_records() {
        let temp = NamedTempFile::new().unwrap();
        let lm = LogManager::new(temp.path()).unwrap();

        lm.append_record(&LogRecord::Commit { txn_id: 1 }).unwrap();
        lm.append_record(&LogRecord::Abort { txn_id: 2 }).unwrap();
        lm.flush().unwrap();

        let records = RecoveryManager::recover(temp.path()).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], LogRecord::Commit { txn_id: 1 });
        assert_eq!(records[1], LogRecord::Abort { txn_id: 2 });
    }
}
