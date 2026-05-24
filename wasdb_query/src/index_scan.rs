use crate::executors::{Executor, Tuple, Value};
use wasdb_catalog::schema::Schema;

pub struct IndexScanExecutor {
    schema: Schema,
    tuples: Vec<Tuple>, // In a real DB, this would read from the BTree via BufferPool
    key: i32,
    cursor: usize,
    index_col_idx: usize,
}

impl IndexScanExecutor {
    pub fn new(schema: Schema, tuples: Vec<Tuple>, key: i32, index_col_idx: usize) -> Self {
        Self {
            schema,
            tuples,
            key,
            cursor: 0,
            index_col_idx,
        }
    }
}

impl Executor for IndexScanExecutor {
    fn init(&mut self) {
        self.cursor = 0;
        // Mocking an index scan by pre-filtering the tuples
        self.tuples.retain(|t| {
            if let Some(Value::Integer(val)) = t.values.get(self.index_col_idx) {
                *val == self.key
            } else {
                false
            }
        });
    }

    fn next(&mut self) -> Option<Tuple> {
        if self.cursor < self.tuples.len() {
            let tuple = self.tuples[self.cursor].clone();
            self.cursor += 1;
            Some(tuple)
        } else {
            None
        }
    }

    fn get_output_schema(&self) -> &Schema {
        &self.schema
    }
}
