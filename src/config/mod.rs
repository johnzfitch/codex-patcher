pub mod applicator;
pub mod loader;
pub mod schema;
pub mod version;

pub use applicator::{apply_patches, ApplicationError, PatchResult};
pub use loader::{load_from_path, load_from_str, ConfigError};
pub use schema::{
    Constraints, HashAlgorithm, Metadata, Operation, PatchConfig, PatchDefinition, Positioning,
    Query, RelativePosition, ValidationError, ValidationIssue, Verify,
};
pub use version::{matches_requirement, VersionError};
