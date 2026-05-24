use crate::schema::Schema;
use crate::table::TableMetadata;
use crate::CatalogError;
use std::collections::HashMap;
use wasdb_storage::PageId;

/// A simplified in-memory catalog representing pg_catalog.
/// In a full DBMS, this would be backed by its own B+ tree over the Buffer Pool.
#[derive(Default)]
pub struct Catalog {
    tables: HashMap<String, TableMetadata>,
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    /// Creates a new table entry in the catalog.
    pub fn create_table(
        &mut self,
        table_name: String,
        schema: Schema,
        root_page_id: PageId,
    ) -> Result<(), CatalogError> {
        if self.tables.contains_key(&table_name) {
            return Err(CatalogError::TableExists(table_name));
        }

        let meta = TableMetadata {
            table_name: table_name.clone(),
            schema,
            root_page_id,
        };

        self.tables.insert(table_name, meta);
        Ok(())
    }

    /// Retrieves metadata for a specific table.
    pub fn get_table(&self, table_name: &str) -> Result<&TableMetadata, CatalogError> {
        self.tables
            .get(table_name)
            .ok_or_else(|| CatalogError::TableNotFound(table_name.to_string()))
    }
}
