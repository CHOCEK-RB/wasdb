use crate::traits::Index;
use parking_lot::RwLock;
use std::collections::HashMap;
use wasdb_storage::CTID;

/// A basic in-memory hash index for demonstration purposes
pub struct HashIndex {
    map: RwLock<HashMap<i32, CTID>>,
}

impl Default for HashIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl HashIndex {
    pub fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }
}

impl Index for HashIndex {
    fn insert(&self, key: i32, ctid: CTID) -> Result<(), Box<dyn std::error::Error>> {
        let mut map = self.map.write();
        if map.contains_key(&key) {
            return Err("Duplicate key".into());
        }
        map.insert(key, ctid);
        Ok(())
    }

    fn search(&self, key: i32) -> Result<CTID, Box<dyn std::error::Error>> {
        let map = self.map.read();
        map.get(&key).copied().ok_or("Key not found".into())
    }

    fn range_search(
        &self,
        _start_key: i32,
        _end_key: i32,
    ) -> Result<Vec<CTID>, Box<dyn std::error::Error>> {
        Err("Hash indexes do not support range search efficiently".into())
    }

    fn delete(&self, key: i32) -> Result<(), Box<dyn std::error::Error>> {
        let mut map = self.map.write();
        if map.remove(&key).is_some() {
            Ok(())
        } else {
            Err("Key not found".into())
        }
    }
}
