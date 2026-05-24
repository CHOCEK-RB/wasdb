use thiserror::Error;

#[derive(Error, Debug)]
pub enum CatalogError {
    #[error("Table {0} already exists")]
    TableExists(String),
    #[error("Table {0} not found")]
    TableNotFound(String),
}

pub mod catalog;
pub mod heap;
pub mod schema;
pub mod table;

pub use catalog::Catalog;
