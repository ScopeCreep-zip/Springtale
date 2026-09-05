pub mod sign;
pub mod types;
pub mod verify;

pub use sign::sign_manifest;
pub use springtale_crypto::signature::SignatureAlgorithm;
pub use types::{ActionDecl, Capability, ConnectorManifest, DataDisclosure, TriggerDecl};
pub use verify::{verify_manifest, verify_manifest_signature};
