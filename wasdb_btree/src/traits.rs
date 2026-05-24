use wasdb_storage::CTID;

/// Trait for defining the pluggable indexing strategy (BTree, Hash, etc.)
pub trait Index {
    /// Insert a key pointing to a CTID
    fn insert(&self, key: i32, ctid: CTID) -> Result<(), Box<dyn std::error::Error>>;

    /// Exact match search
    fn search(&self, key: i32) -> Result<CTID, Box<dyn std::error::Error>>;

    /// Range search for range queries
    fn range_search(
        &self,
        start_key: i32,
        end_key: i32,
    ) -> Result<Vec<CTID>, Box<dyn std::error::Error>>;

    /// Delete a key
    fn delete(&self, key: i32) -> Result<(), Box<dyn std::error::Error>>;
}
