use crate::executors::{Executor, Tuple};
use wasdb_catalog::schema::Schema;
use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom};
use tempfile::NamedTempFile;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

// We need a wrapper to hold tuples in the BinaryHeap to implement min-heap by the sort key
struct HeapEntry {
    tuple: Tuple,
    run_idx: usize,
    sort_idx: usize,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.tuple.values[self.sort_idx] == other.tuple.values[self.sort_idx]
    }
}

impl Eq for HeapEntry {}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap
        other.tuple.values[self.sort_idx]
            .partial_cmp(&self.tuple.values[self.sort_idx])
            .unwrap_or(Ordering::Equal)
    }
}

pub struct ExternalMergeSortExecutor {
    child: Box<dyn Executor>,
    sort_idx: usize,
    chunk_size: usize,
    runs: Vec<NamedTempFile>,
    heap: BinaryHeap<HeapEntry>,
}

impl ExternalMergeSortExecutor {
    pub fn new(child: Box<dyn Executor>, sort_idx: usize) -> Self {
        Self {
            child,
            sort_idx,
            chunk_size: 100, // Small chunk size to force external sort behavior in tests
            runs: Vec::new(),
            heap: BinaryHeap::new(),
        }
    }

    fn serialize_tuple(tuple: &Tuple) -> Vec<u8> {
        // Simple serialization using serde json for mock educational DB
        serde_json::to_vec(tuple).expect("Failed to serialize tuple")
    }

    fn deserialize_tuple(buf: &str) -> Option<Tuple> {
        if buf.is_empty() { return None; }
        serde_json::from_str(buf).ok()
    }

    fn perform_external_merge_sort(&mut self) {
        let mut chunk = Vec::new();
        
        // Phase 1: Read, Sort, Write
        while let Some(tuple) = self.child.next() {
            chunk.push(tuple);
            if chunk.len() >= self.chunk_size {
                self.flush_chunk(&mut chunk);
            }
        }
        if !chunk.is_empty() {
            self.flush_chunk(&mut chunk);
        }

        // Phase 2: Setup K-way merge
        for (idx, run) in self.runs.iter_mut().enumerate() {
            run.seek(SeekFrom::Start(0)).unwrap();
            if let Some(tuple) = Self::read_next_tuple(run) {
                self.heap.push(HeapEntry {
                    tuple,
                    run_idx: idx,
                    sort_idx: self.sort_idx,
                });
            }
        }
    }

    fn flush_chunk(&mut self, chunk: &mut Vec<Tuple>) {
        let idx = self.sort_idx;
        chunk.sort_by(|a, b| {
            a.values[idx]
                .partial_cmp(&b.values[idx])
                .unwrap_or(Ordering::Equal)
        });

        let mut file = NamedTempFile::new().expect("Failed to create temp file for sort run");
        for tuple in chunk.iter() {
            let data = Self::serialize_tuple(tuple);
            file.write_all(&data).unwrap();
            file.write_all(b"\n").unwrap(); // newline delimited
        }
        self.runs.push(file);
        chunk.clear();
    }

    fn read_next_tuple(file: &mut impl Read) -> Option<Tuple> {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match file.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    if byte[0] == b'\n' {
                        break;
                    }
                    buf.push(byte[0]);
                }
                Err(_) => return None,
            }
        }
        if buf.is_empty() {
            return None;
        }
        Self::deserialize_tuple(std::str::from_utf8(&buf).unwrap())
    }
}

impl Executor for ExternalMergeSortExecutor {
    fn init(&mut self) {
        self.child.init();
        self.runs.clear();
        self.heap.clear();
        self.perform_external_merge_sort();
    }

    fn next(&mut self) -> Option<Tuple> {
        if let Some(entry) = self.heap.pop() {
            let tuple = entry.tuple;
            
            // Fetch next from the same run
            if let Some(mut run_file) = self.runs.get_mut(entry.run_idx).map(|f| f.as_file_mut()) {
                if let Some(next_tuple) = Self::read_next_tuple(run_file) {
                    self.heap.push(HeapEntry {
                        tuple: next_tuple,
                        run_idx: entry.run_idx,
                        sort_idx: self.sort_idx,
                    });
                }
            }
            
            Some(tuple)
        } else {
            None
        }
    }

    fn get_output_schema(&self) -> &Schema {
        self.child.get_output_schema()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executors::{SeqScanExecutor, Value};
    use wasdb_catalog::schema::{Column, TypeId};

    #[test]
    fn next_should_return_tuples_in_sorted_order() {
        // Arrange
        let schema = Schema::new(vec![Column::new(String::from("id"), TypeId::Integer, 4)]);
        let tuples = vec![
            Tuple::new(1, vec![Value::Integer(10)]),
            Tuple::new(1, vec![Value::Integer(1)]),
            Tuple::new(1, vec![Value::Integer(5)]),
        ];
        let scan = SeqScanExecutor::new(schema, tuples);
        let mut sort = ExternalMergeSortExecutor::new(Box::new(scan), 0);

        // Act
        sort.init();

        // Assert
        assert_eq!(sort.next().unwrap().values[0], Value::Integer(1));
        assert_eq!(sort.next().unwrap().values[0], Value::Integer(5));
        assert_eq!(sort.next().unwrap().values[0], Value::Integer(10));
        assert_eq!(sort.next(), None);
    }
}
