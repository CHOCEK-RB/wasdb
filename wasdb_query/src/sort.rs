use crate::executors::{Executor, Tuple};
use wasdb_catalog::schema::Schema;

pub struct ExternalMergeSortExecutor<E: Executor> {
    child: Box<E>,
    sorted_tuples: Vec<Tuple>,
    cursor: usize,
    sort_idx: usize,
}

impl<E: Executor> ExternalMergeSortExecutor<E> {
    pub fn new(child: Box<E>, sort_idx: usize) -> Self {
        Self {
            child,
            sorted_tuples: Vec::new(),
            cursor: 0,
            sort_idx,
        }
    }

    /// Mock implementation of External Merge Sort.
    /// In a fully compliant system, this would:
    /// 1. Read chunks of pages into the Buffer Pool.
    /// 2. Sort the chunks in-memory and write them out as "runs" to disk.
    /// 3. K-way merge the sorted runs from disk.
    fn perform_external_merge_sort(&mut self) {
        // Collect all for simplicity in this educational implementation
        while let Some(tuple) = self.child.next() {
            self.sorted_tuples.push(tuple);
        }

        let idx = self.sort_idx;
        self.sorted_tuples.sort_by(|a, b| {
            a[idx]
                .partial_cmp(&b[idx])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

impl<E: Executor> Executor for ExternalMergeSortExecutor<E> {
    fn init(&mut self) {
        self.child.init();
        self.sorted_tuples.clear();
        self.cursor = 0;
        self.perform_external_merge_sort();
    }

    fn next(&mut self) -> Option<Tuple> {
        if self.cursor < self.sorted_tuples.len() {
            let tuple = self.sorted_tuples[self.cursor].clone();
            self.cursor += 1;
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
            vec![Value::Integer(10)],
            vec![Value::Integer(1)],
            vec![Value::Integer(5)],
        ];
        let scan = SeqScanExecutor::new(schema, tuples);
        let mut sort = ExternalMergeSortExecutor::new(Box::new(scan), 0);

        // Act
        sort.init();

        // Assert
        assert_eq!(sort.next(), Some(vec![Value::Integer(1)]));
        assert_eq!(sort.next(), Some(vec![Value::Integer(5)]));
        assert_eq!(sort.next(), Some(vec![Value::Integer(10)]));
        assert_eq!(sort.next(), None);
    }
}
