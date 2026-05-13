//! Structured-extraction capability layer.
//!
//! The trait + outcome + error types live here, separate from the
//! happy-path completion trait so adapters that don't support
//! constrained decoding (or local mocks / Noop) can implement
//! [`crate::AiAdapter`] without dragging in extraction-specific
//! types.
//!
//! Capability discovery: [`crate::AiAdapter::structured_extractor`]
//! returns `Option<&dyn StructuredExtractor>`. `None` is the safe
//! default — preflight catches this before the recipe deploys.

pub mod error;
pub mod trait_;
pub mod validate;

pub use error::ExtractorError;
pub use trait_::{ExtractOptions, ExtractOutcome, StructuredExtractor};
