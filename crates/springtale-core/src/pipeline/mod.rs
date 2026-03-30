pub mod compose;
pub mod context;
pub mod error;
pub mod stage;

pub use compose::compose_pipeline;
pub use context::Attachment;
pub use context::PipelineContext;
pub use error::PipelineError;
pub use stage::Stage;
