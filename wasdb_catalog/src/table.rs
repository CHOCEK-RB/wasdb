use crate::schema::Schema;
use wasdb_storage::PageId;

/// Represents metadata about a database table.
#[derive(Debug)]
pub struct TableMetadata {
    pub table_name: String,
    pub schema: Schema,
    pub root_page_id: PageId,
}
