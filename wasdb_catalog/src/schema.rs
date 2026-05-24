/// Supported data types in the DBMS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeId {
    Integer,
    Varchar,
    Boolean,
}

/// Represents a single column in a table schema.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub type_id: TypeId,
    pub length: u32,
    pub offset: u32,
}

impl Column {
    pub fn new(name: String, type_id: TypeId, length: u32) -> Self {
        Self {
            name,
            type_id,
            length,
            offset: 0, // Calculated by schema
        }
    }
}

/// Represents the structure of a row in a table.
#[derive(Debug, Clone)]
pub struct Schema {
    pub columns: Vec<Column>,
    pub tuple_length: u32,
}

impl Schema {
    pub fn new(mut columns: Vec<Column>) -> Self {
        let mut offset = 0;
        for col in &mut columns {
            col.offset = offset;
            offset += col.length;
        }
        Self {
            columns,
            tuple_length: offset,
        }
    }
}
