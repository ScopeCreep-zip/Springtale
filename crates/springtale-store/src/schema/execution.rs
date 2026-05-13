use serde::Serialize;

use specta::Type;
/// Input for inserting an execution result.
///
/// Groups the 7 parameters of `insert_execution_result` into a struct
/// to satisfy clippy's too_many_arguments lint.
pub struct ExecutionResultInput<'a> {
    pub id: &'a str,
    pub connector_name: &'a str,
    pub rule_id: Option<&'a str>,
    pub rule_name: Option<&'a str>,
    pub output_json: &'a str,
    pub success: bool,
    pub error_message: Option<&'a str>,
}

/// Row from the execution_results table.
///
/// Stores the actual output data from rule/action executions.
/// Events store metadata (what ran, when). Results store data (what was returned).
#[derive(Debug, Clone, Serialize, Type)]
pub struct ExecutionResultRow {
    pub id: String,
    pub connector_name: String,
    pub rule_name: Option<String>,
    pub output_json: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub created_at: String,
}
