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
    pub is_primary_key: bool,
    pub is_not_null: bool,
}

impl Column {
    pub fn new(name: String, type_id: TypeId, length: u32) -> Self {
        Self {
            name,
            type_id,
            length,
            offset: 0, // Calculated by schema
            is_primary_key: false,
            is_not_null: false,
        }
    }

    pub fn with_primary_key(mut self, is_pk: bool) -> Self {
        self.is_primary_key = is_pk;
        if is_pk {
            self.is_not_null = true;
        }
        self
    }

    pub fn with_not_null(mut self, not_null: bool) -> Self {
        self.is_not_null = not_null;
        self
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_offsets() {
        let cols = vec![
            Column::new("a".into(), TypeId::Integer, 4),
            Column::new("b".into(), TypeId::Varchar, 16),
            Column::new("c".into(), TypeId::Boolean, 1),
        ];
        let schema = Schema::new(cols);

        assert_eq!(schema.tuple_length, 21);
        assert_eq!(schema.columns[0].offset, 0);
        assert_eq!(schema.columns[1].offset, 4);
        assert_eq!(schema.columns[2].offset, 20);
    }
}
