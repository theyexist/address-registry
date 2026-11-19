//! The schema for the address registry: the types, not the data.
//!
//! A consumer — mitos, a worker, a viewer — depends on this crate for the shape
//! of `imported/registry.json` and `resolved/registry.json`, and fetches the
//! data itself. The crate changes when the shape changes, never when a contract
//! is added, so pinning it is not the fragility that pinning the data was.
//!
//! `format` is the handshake. A field added to the shape is a new `format`,
//! because deserialisation rejects unknown keys rather than ignoring them: a
//! reader that silently skipped a field it did not understand would be reading a
//! different registry and would not say so.
//!
//! No I/O, no network, no filesystem. Reading the bytes is the consumer's job;
//! this crate only decides whether they are a registry.

pub mod bech32;
pub mod behaviour;
mod claim;
mod contract;
mod report;
mod resolved;

pub use behaviour::BehaviourError;
pub use claim::{credential, Audit, Claim, EternlClaim, Source, StricaClaim};
pub use contract::{Contract, DerivedFrom, Invalid, Kind, Shape};
pub use report::{
    Conflict, ConflictClaim, ImportCounts, ImportReport, Imported, IndexObservation, Observations,
    Skipped, SourceRef, Sources,
};
pub use resolved::{
    AuthoredRow, Behaviour, DerivedFromStatus, ResolveCounts, ResolveReport, Resolved,
};

/// The file format this crate reads and writes.
pub const FORMAT: u32 = 1;
