use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Metadata about a file attachment flowing through the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Attachment {
    /// Original filename.
    pub filename: String,
    /// MIME type (e.g., "image/png", "application/pdf").
    pub mime_type: String,
    /// Raw bytes of the attachment.
    #[serde(skip)]
    pub data: Vec<u8>,
    /// Size in bytes (populated even when data is not serialized).
    pub size: usize,
}

/// Context that flows through a pipeline, carrying data between stages.
///
/// Each rule evaluation gets a fresh context. No shared mutable state
/// between concurrent rule evaluations.
#[derive(Debug, Clone)]
pub struct PipelineContext {
    /// Unique trace ID for this pipeline execution.
    pub trace_id: Uuid,

    /// The trigger payload that started this pipeline.
    pub input: serde_json::Value,

    /// Output from the most recent stage (stages read this as their input).
    pub output: serde_json::Value,

    /// Accumulated errors from stages that failed but were non-fatal.
    pub errors: Vec<String>,

    /// How many times this pipeline has been retried.
    pub retry_count: u32,

    /// Current chain depth (for detecting Chain→Chain→Chain overflow).
    pub chain_depth: u32,

    /// Maximum allowed chain depth.
    pub max_chain_depth: u32,

    /// File attachments flowing through the pipeline.
    pub attachments: Vec<Attachment>,

    /// Fuel remaining for this pipeline execution (None = unlimited).
    /// Set by the orchestrator at spawn time. The orchestrator manages
    /// the atomic fuel budget externally; this field carries the snapshot
    /// allocated to this specific pipeline.
    pub fuel_remaining: Option<u64>,
}

impl PipelineContext {
    /// Create a new context for a pipeline execution.
    pub fn new(input: serde_json::Value) -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            input: input.clone(),
            output: input,
            errors: Vec::new(),
            retry_count: 0,
            chain_depth: 0,
            max_chain_depth: 4,
            attachments: Vec::new(),
            fuel_remaining: None,
        }
    }

    /// Create a child context for a sub-pipeline (increments chain depth).
    /// Inherits fuel_remaining from parent (read-only snapshot).
    pub fn child(&self) -> Result<Self, super::error::PipelineError> {
        let new_depth = self.chain_depth + 1;
        if new_depth > self.max_chain_depth {
            return Err(super::error::PipelineError::ChainDepthExceeded {
                depth: new_depth,
                max: self.max_chain_depth,
            });
        }
        Ok(Self {
            trace_id: self.trace_id,
            input: self.output.clone(),
            output: serde_json::Value::Null,
            errors: Vec::new(),
            retry_count: 0,
            chain_depth: new_depth,
            max_chain_depth: self.max_chain_depth,
            attachments: Vec::new(),
            fuel_remaining: self.fuel_remaining,
        })
    }
}
