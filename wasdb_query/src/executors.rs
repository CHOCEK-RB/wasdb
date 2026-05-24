use wasdb_catalog::schema::Schema;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    Integer(i32),
    Varchar(String),
    Boolean(bool),
}

pub type Tuple = Vec<Value>;

/// Volcano-style execution iterator
pub trait Executor {
    /// Initialize or reset the executor.
    fn init(&mut self);
    /// Fetch the next tuple.
    fn next(&mut self) -> Option<Tuple>;
    /// Get the schema of the tuples produced by this executor.
    fn get_output_schema(&self) -> &Schema;
}

/// A dummy SeqScan executor that returns a hardcoded set of tuples
/// for demonstration/testing purposes.
pub struct SeqScanExecutor {
    schema: Schema,
    tuples: Vec<Tuple>,
    cursor: usize,
}

impl SeqScanExecutor {
    pub fn new(schema: Schema, tuples: Vec<Tuple>) -> Self {
        Self {
            schema,
            tuples,
            cursor: 0,
        }
    }
}

impl Executor for SeqScanExecutor {
    fn init(&mut self) {
        self.cursor = 0;
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

pub struct FilterExecutor<Child: Executor> {
    child: Box<Child>,
    predicate: fn(&Tuple) -> bool,
}

impl<E: Executor> FilterExecutor<E> {
    pub fn new(child: Box<E>, predicate: fn(&Tuple) -> bool) -> Self {
        Self { child, predicate }
    }
}

impl<E: Executor> Executor for FilterExecutor<E> {
    fn init(&mut self) {
        self.child.init();
    }

    fn next(&mut self) -> Option<Tuple> {
        while let Some(tuple) = self.child.next() {
            if (self.predicate)(&tuple) {
                return Some(tuple);
            }
        }
        None
    }

    fn get_output_schema(&self) -> &Schema {
        self.child.get_output_schema()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasdb_catalog::schema::{Column, TypeId};

    #[test]
    fn test_seq_scan_and_filter() {
        let schema = Schema::new(vec![
            Column::new(String::from("id"), TypeId::Integer, 4),
        ]);

        let tuples = vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(5)],
            vec![Value::Integer(10)],
        ];

        let scan = SeqScanExecutor::new(schema, tuples);
        
        let mut filter = FilterExecutor::new(Box::new(scan), |t| {
            if let Value::Integer(id) = t[0] {
                id > 1
            } else {
                false
            }
        });

        filter.init();
        assert_eq!(filter.next(), Some(vec![Value::Integer(5)]));
        assert_eq!(filter.next(), Some(vec![Value::Integer(10)]));
        assert_eq!(filter.next(), None);
    }
}
