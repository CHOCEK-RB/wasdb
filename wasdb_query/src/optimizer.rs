use crate::executors::Tuple;
use wasdb_catalog::Catalog;

pub enum LogicalPlan {
    SeqScan {
        table_name: String,
    },
    IndexScan {
        table_name: String,
        key: i32,
    },
    NestedLoopJoin {
        left: Box<LogicalPlan>,
        right: Box<LogicalPlan>,
        predicate: fn(&Tuple, &Tuple) -> bool,
    },
    Filter {
        child: Box<LogicalPlan>,
        predicate: fn(&Tuple) -> bool,
        index_match: Option<(String, i32)>, // (column_name, value)
    },
}

pub struct Optimizer<'a> {
    catalog: &'a Catalog,
    data: &'a std::collections::HashMap<String, Vec<Tuple>>,
}

impl<'a> Optimizer<'a> {
    pub fn new(catalog: &'a Catalog, data: &'a std::collections::HashMap<String, Vec<Tuple>>) -> Self {
        Self { catalog, data }
    }

    pub fn optimize(&self, plan: LogicalPlan) -> Result<Box<dyn crate::executors::Executor>, String> {
        match plan {
            LogicalPlan::SeqScan { table_name } => {
                let table_meta = self
                    .catalog
                    .get_table(&table_name)
                    .map_err(|e| format!("{:?}", e))?;
                
                let tuples = self.data.get(&table_name).cloned().unwrap_or_default();
                Ok(Box::new(crate::executors::SeqScanExecutor::new(
                    table_meta.schema.clone(),
                    tuples,
                )))
            }
            LogicalPlan::IndexScan { table_name, key } => {
                let table_meta = self
                    .catalog
                    .get_table(&table_name)
                    .map_err(|e| format!("{:?}", e))?;
                if table_meta.indexes.is_empty() {
                    return Err(format!("No index found on {}", table_name));
                }
                
                let tuples = self.data.get(&table_name).cloned().unwrap_or_default();
                let index_col_idx = table_meta.indexes[0].column_index;
                
                Ok(Box::new(crate::index_scan::IndexScanExecutor::new(
                    table_meta.schema.clone(),
                    tuples,
                    key,
                    index_col_idx,
                )))
            }
            LogicalPlan::NestedLoopJoin { left, right, predicate } => {
                let left_plan = self.optimize(*left)?;
                let right_plan = self.optimize(*right)?;
                let schema = left_plan.get_output_schema().clone(); // Simplified schema merging
                Ok(Box::new(crate::executors::NestedLoopJoinExecutor::new(
                    left_plan,
                    right_plan,
                    predicate,
                    schema,
                )))
            }
            LogicalPlan::Filter { child, predicate, index_match } => {
                // Heuristic: If child is SeqScan, and filter has an exact match on an indexed column,
                // transform into IndexScan.
                if let LogicalPlan::SeqScan { ref table_name } = *child {
                    if let Some((col_name, val)) = index_match {
                        let table_meta = self.catalog.get_table(table_name).map_err(|e| format!("{:?}", e))?;
                        if table_meta.indexes.iter().any(|idx| {
                            table_meta.schema.columns.get(idx.column_index).map(|c| c.name.as_str()) == Some(col_name.as_str())
                        }) {
                            // We can use IndexScan!
                            return self.optimize(LogicalPlan::IndexScan {
                                table_name: table_name.clone(),
                                key: val,
                            });
                        }
                    }
                }
                
                let child_plan = self.optimize(*child)?;
                Ok(Box::new(crate::executors::FilterExecutor::new(
                    child_plan,
                    predicate,
                )))
            }
        }
    }
}
