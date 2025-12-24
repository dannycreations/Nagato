pub mod binary;
pub mod hunk;
pub mod line;
pub mod patch;

pub use binary::{BinaryFragment, BinaryPatchKind};
pub use hunk::Hunk;
pub use line::{Line, LineKind};
pub use patch::Patch;
