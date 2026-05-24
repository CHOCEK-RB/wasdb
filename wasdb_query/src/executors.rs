use wasdb_catalog::schema::Schema;
use wasdb_tx::{TransactionId, TransactionManager, INVALID_TXN_ID};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    Integer(i32),
    Varchar(String),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tuple {
    pub xmin: TransactionId,
    pub xmax: TransactionId,
    pub values: Vec<Value>,
}

impl Tuple {
    pub fn new(xmin: TransactionId, values: Vec<Value>) -> Self {
        Self {
            xmin,
            xmax: INVALID_TXN_ID,
            values,
        }
    }
}

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
/// and filters out rows that are invisible to the current transaction based on MVCC rules.
pub struct SeqScanExecutor {
    schema: Schema,
    tuples: Vec<Tuple>,
    cursor: usize,
    txn_manager: Option<Arc<TransactionManager>>,
    current_txn: TransactionId,
}

impl SeqScanExecutor {
    pub fn new(schema: Schema, tuples: Vec<Tuple>) -> Self {
        Self {
            schema,
            tuples,
            cursor: 0,
            txn_manager: None,
            current_txn: INVALID_TXN_ID,
        }
    }

    pub fn with_mvcc(
        schema: Schema,
        tuples: Vec<Tuple>,
        txn_manager: Arc<TransactionManager>,
        current_txn: TransactionId,
    ) -> Self {
        Self {
            schema,
            tuples,
            cursor: 0,
            txn_manager: Some(txn_manager),
            current_txn,
        }
    }
}

impl Executor for SeqScanExecutor {
    fn init(&mut self) {
        self.cursor = 0;
    }

    fn next(&mut self) -> Option<Tuple> {
        while self.cursor < self.tuples.len() {
            let tuple = self.tuples[self.cursor].clone();
            self.cursor += 1;

            if let Some(tm) = &self.txn_manager {
                if !tm.is_visible(tuple.xmin, tuple.xmax, self.current_txn) {
                    continue; // Skip invisible tuples
                }
            }
            return Some(tuple);
        }
        None
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
    fn seq_scan_should_return_all_tuples() {
        let schema = Schema::new(vec![Column::new(String::from("id"), TypeId::Integer, 4)]);
        let tuples = vec![Tuple::new(1, vec![Value::Integer(1)]), Tuple::new(1, vec![Value::Integer(5)])];
        let mut scan = SeqScanExecutor::new(schema, tuples);
        
        scan.init();
        assert_eq!(scan.next().unwrap().values[0], Value::Integer(1));
        assert_eq!(scan.next().unwrap().values[0], Value::Integer(5));
        assert_eq!(scan.next(), None);
    }

    #[test]
    fn filter_should_drop_unmatched_tuples() {
        let schema = Schema::new(vec![Column::new(String::from("id"), TypeId::Integer, 4)]);
        let tuples = vec![Tuple::new(1, vec![Value::Integer(1)]), Tuple::new(1, vec![Value::Integer(10)])];
        let scan = SeqScanExecutor::new(schema, tuples);
        
        let mut filter = FilterExecutor::new(Box::new(scan), |t| {
            if let Value::Integer(id) = t.values[0] {
                id > 5
            } else {
                false
            }
        });

        filter.init();
        assert_eq!(filter.next().unwrap().values[0], Value::Integer(10));
        assert_eq!(filter.next(), None);
    }

    #[test]
    fn seq_scan_should_filter_by_mvcc_visibility() {
        let tm = Arc::new(TransactionManager::new());
        let txn_1 = tm.begin(); // Committed later
        let txn_2 = tm.begin(); // Active

        tm.commit(txn_1).unwrap();

        let schema = Schema::new(vec![Column::new(String::from("id"), TypeId::Integer, 4)]);
        let tuples = vec![
            Tuple::new(txn_1, vec![Value::Integer(10)]), // Inserted by committed txn (Visible)
            Tuple::new(txn_2, vec![Value::Integer(20)]), // Inserted by active txn (Invisible)
        ];

        let mut scan = SeqScanExecutor::with_mvcc(schema, tuples, tm, txn_2 + 1);
        scan.init();

        assert_eq!(scan.next().unwrap().values[0], Value::Integer(10));
        assert_eq!(scan.next(), None);
    }
}
