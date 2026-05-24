use wasdb_catalog::catalog::Catalog;
use wasdb_catalog::schema::{Column, Schema, TypeId};
use wasdb_storage::PageId;

#[test]
fn create_table_should_store_metadata() {
    let mut catalog = Catalog::new();
    let schema = Schema::new(vec![
        Column::new("id".to_string(), TypeId::Integer, 4),
        Column::new("name".to_string(), TypeId::Varchar, 32),
    ]);
    let root_page_id = PageId {
        file_id: 0,
        page_num: 1,
    };

    catalog
        .create_table("users".to_string(), schema, root_page_id)
        .unwrap();
    let table = catalog.get_table("users").unwrap();

    assert_eq!(table.table_name, "users");
    assert_eq!(table.schema.columns.len(), 2);
    assert_eq!(table.schema.columns[1].offset, 4);
    assert_eq!(table.schema.tuple_length, 36);
    assert_eq!(table.root_page_id.page_num, 1);
}
