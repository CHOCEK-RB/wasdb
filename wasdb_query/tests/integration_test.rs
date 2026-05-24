use tempfile::NamedTempFile;
use wasdb_btree::tree::BTreeIndex;
use wasdb_buffer::buffer_pool::BufferPoolManager;
use wasdb_buffer::lru::LRUReplacer;
use wasdb_catalog::schema::{Column, Schema, TypeId};
use wasdb_query::executors::{Executor, FilterExecutor, SeqScanExecutor, Value};
use wasdb_query::sort::ExternalMergeSortExecutor;
use wasdb_storage::BasicDiskManager;

const PAGE_SIZE: usize = 8192;

#[test]
fn end_to_end_query_execution_should_succeed() {
    let temp_file = NamedTempFile::new().unwrap();
    let disk_manager = BasicDiskManager::<PAGE_SIZE>::new(temp_file.path()).unwrap();
    let replacer = Box::new(LRUReplacer::new(10));
    let buffer_pool = BufferPoolManager::new(10, disk_manager, replacer);

    let btree = BTreeIndex::new(&buffer_pool, None);
    let mut ctid1 = wasdb_storage::CTID::default(); ctid1.slot_idx = 100;
    btree.insert(10, ctid1).unwrap();
    
    let mut ctid2 = wasdb_storage::CTID::default(); ctid2.slot_idx = 200;
    btree.insert(20, ctid2).unwrap();
    
    let mut ctid3 = wasdb_storage::CTID::default(); ctid3.slot_idx = 50;
    btree.insert(5, ctid3).unwrap();

    assert_eq!(btree.search(10).unwrap(), ctid1);
    assert_eq!(btree.search(20).unwrap(), ctid2);
    assert_eq!(btree.search(5).unwrap(), ctid3);
    assert!(btree.search(99).is_err());

    let schema = Schema::new(vec![
        Column::new(String::from("id"), TypeId::Integer, 4),
        Column::new(String::from("value"), TypeId::Integer, 4),
    ]);

    let tuples = vec![
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(10), Value::Integer(100)]),
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(20), Value::Integer(200)]),
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(5), Value::Integer(50)]),
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(30), Value::Integer(300)]),
    ];
    let scan = SeqScanExecutor::new(schema.clone(), tuples);

    let mut filter = FilterExecutor::new(Box::new(scan), |tuple| {
        if let Value::Integer(id) = tuple.values[0] {
            id > 10
        } else {
            false
        }
    });

    let mut filtered_tuples = Vec::new();
    filter.init();
    while let Some(tuple) = filter.next() {
        filtered_tuples.push(tuple);
    }

    assert_eq!(filtered_tuples.len(), 2);
    assert_eq!(filtered_tuples[0].values[0], Value::Integer(20));
    assert_eq!(filtered_tuples[1].values[0], Value::Integer(30));

    let tuples_for_sort = vec![
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(20), Value::Integer(200)]),
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(5), Value::Integer(50)]),
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(30), Value::Integer(300)]),
        wasdb_query::executors::Tuple::new(1, vec![Value::Integer(10), Value::Integer(100)]),
    ];
    let scan_for_sort = SeqScanExecutor::new(schema.clone(), tuples_for_sort);
    let mut sort = ExternalMergeSortExecutor::new(Box::new(scan_for_sort), 0);
    
    sort.init();
    let mut sorted_tuples = Vec::new();
    while let Some(tuple) = sort.next() {
        sorted_tuples.push(tuple);
    }

    assert_eq!(sorted_tuples.len(), 4);
    assert_eq!(sorted_tuples[0].values[0], Value::Integer(5));
    assert_eq!(sorted_tuples[1].values[0], Value::Integer(10));
    assert_eq!(sorted_tuples[2].values[0], Value::Integer(20));
    assert_eq!(sorted_tuples[3].values[0], Value::Integer(30));
}
