use serde::Serialize;

/// Row from the execution_results table.
///
/// Stores the actual output data from rule/action executions.
/// Events store metadata (what ran, when). Results store data (what was returned).
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionResultRow {
    pub id: String,
    pub connector_name: String,
    pub rule_name: Option<String>,
    pub output_json: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub created_at: String,
}
