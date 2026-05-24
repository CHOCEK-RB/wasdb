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
fn test_full_system_integration() {
    // 1. Setup Storage and Buffer Pool
    let temp_file = NamedTempFile::new().unwrap();
    let disk_manager = BasicDiskManager::<PAGE_SIZE>::new(temp_file.path()).unwrap();
    let replacer = Box::new(LRUReplacer::new(10));
    let buffer_pool = BufferPoolManager::new(10, disk_manager, replacer);

    // 2. Setup B+ Tree
    let btree = BTreeIndex::new(&buffer_pool, None);
    // Insert a few records
    btree.insert(10, 100).unwrap();
    btree.insert(20, 200).unwrap();
    btree.insert(5, 50).unwrap();

    assert_eq!(btree.search(10).unwrap(), 100);
    assert_eq!(btree.search(20).unwrap(), 200);
    assert_eq!(btree.search(5).unwrap(), 50);
    assert!(btree.search(99).is_err());

    // 3. Setup Catalog & Query Execution
    let schema = Schema::new(vec![
        Column::new(String::from("id"), TypeId::Integer, 4),
        Column::new(String::from("value"), TypeId::Integer, 4),
    ]);

    // Mock tuples representing rows in a table
    let tuples = vec![
        vec![Value::Integer(10), Value::Integer(100)],
        vec![Value::Integer(20), Value::Integer(200)],
        vec![Value::Integer(5), Value::Integer(50)],
        vec![Value::Integer(30), Value::Integer(300)],
    ];

    let scan = SeqScanExecutor::new(schema.clone(), tuples);

    // 4. Test Filter Executor
    // Filter rows where id > 10
    let mut filter = FilterExecutor::new(Box::new(scan), |tuple| {
        if let Value::Integer(id) = tuple[0] {
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
    assert_eq!(filtered_tuples[0][0], Value::Integer(20));
    assert_eq!(filtered_tuples[1][0], Value::Integer(30));

    // 5. Test External Merge Sort Executor
    // We recreate scan to get all tuples
    let tuples = vec![
        vec![Value::Integer(20), Value::Integer(200)],
        vec![Value::Integer(5), Value::Integer(50)],
        vec![Value::Integer(30), Value::Integer(300)],
        vec![Value::Integer(10), Value::Integer(100)],
    ];
    let scan = SeqScanExecutor::new(schema.clone(), tuples);

    // Sort by id (index 0)
    let mut sort = ExternalMergeSortExecutor::new(Box::new(scan), 0);
    sort.init();

    let mut sorted_tuples = Vec::new();
    while let Some(tuple) = sort.next() {
        sorted_tuples.push(tuple);
    }

    assert_eq!(sorted_tuples.len(), 4);
    assert_eq!(sorted_tuples[0][0], Value::Integer(5));
    assert_eq!(sorted_tuples[1][0], Value::Integer(10));
    assert_eq!(sorted_tuples[2][0], Value::Integer(20));
    assert_eq!(sorted_tuples[3][0], Value::Integer(30));
}
