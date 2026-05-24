use crate::schema::Schema;
use wasdb_storage::PageId;

/// Metadata for an index on a table.
#[derive(Debug, Clone)]
pub struct IndexMetadata {
    pub index_name: String,
    pub table_name: String,
    pub column_index: usize,
    pub root_page_id: PageId,
    pub is_unique: bool,
}

/// Represents metadata about a database table.
#[derive(Debug, Clone)]
pub struct TableMetadata {
    pub table_name: String,
    pub schema: Schema,
    pub root_page_id: PageId,
    pub indexes: Vec<IndexMetadata>,
}
