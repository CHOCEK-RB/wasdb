use crate::executors::{Executor, SeqScanExecutor, NestedLoopJoinExecutor, FilterExecutor, Tuple};
use wasdb_catalog::schema::Schema;
use wasdb_catalog::Catalog;

pub enum LogicalPlan {
    SeqScan { table_name: String },
    IndexScan { table_name: String, key: i32 },
    NestedLoopJoin { left: Box<LogicalPlan>, right: Box<LogicalPlan>, predicate: fn(&Tuple, &Tuple) -> bool },
    Filter { child: Box<LogicalPlan>, predicate: fn(&Tuple) -> bool },
}

pub struct Optimizer<'a> {
    catalog: &'a Catalog,
}

impl<'a> Optimizer<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Converts a logical plan into a physical plan (currently just returns string representation of plan for simplicity)
    /// In a real implementation this would return Box<dyn Executor> or an enum of PhysicalPlan nodes.
    pub fn optimize(&self, plan: LogicalPlan) -> Result<String, String> {
        match plan {
            LogicalPlan::SeqScan { table_name } => {
                let table_meta = self.catalog.get_table(&table_name).map_err(|e| format!("{:?}", e))?;
                // If it has an index, maybe choose IndexScan instead if there's a filter (not possible to know here without filter conditions)
                Ok(format!("SeqScan on {}", table_meta.table_name))
            }
            LogicalPlan::IndexScan { table_name, key } => {
                let table_meta = self.catalog.get_table(&table_name).map_err(|e| format!("{:?}", e))?;
                if table_meta.indexes.is_empty() {
                    return Err(format!("No index found on {}", table_name));
                }
                Ok(format!("IndexScan on {} with key {}", table_name, key))
            }
            LogicalPlan::NestedLoopJoin { left, right, .. } => {
                let left_plan = self.optimize(*left)?;
                let right_plan = self.optimize(*right)?;
                Ok(format!("NestedLoopJoin({} | {})", left_plan, right_plan))
            }
            LogicalPlan::Filter { child, .. } => {
                // Heuristic: If child is SeqScan, and filter is on an indexed column, we could transform it into IndexScan.
                let child_plan = self.optimize(*child)?;
                Ok(format!("Filter({})", child_plan))
            }
        }
    }
}
