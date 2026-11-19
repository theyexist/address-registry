//! The schema for the address registry: the types, not the data.
//!
//! A consumer — mitos, a worker, a viewer — depends on this crate for the shape
//! of `imported/registry.json`, `resolved/registry.json` and the two published
//! lookups, `resolved/behaviour.bin` and `resolved/stake-services.bin`, and
//! fetches — or vendors — the data itself. The crate changes when the shape
//! changes, never when a contract is added, so pinning it is not the fragility
//! that pinning the data was.
//!
//! `format` is the handshake. A field added to the shape is a new `format`,
//! because deserialisation rejects unknown keys rather than ignoring them: a
//! reader that silently skipped a field it did not understand would be reading a
//! different registry and would not say so.
//!
//! No I/O, no network, no filesystem. Reading the bytes is the consumer's job;
//! this crate only decides whether they are a registry.

pub mod address;
pub mod bech32;
pub mod behaviour;
mod claim;
mod contract;
mod report;
mod resolved;
pub mod stake;

pub use address::{AddressError, Network, Payment};
pub use behaviour::BehaviourError;
pub use claim::{credential, Audit, Claim, EternlClaim, Source, StricaClaim};
pub use contract::{
    Category, Contract, ContractFile, DerivedFrom, Invalid, MarketplacePurpose, Shape,
};
pub use report::{
    Conflict, ConflictClaim, ImportCounts, ImportReport, Imported, IndexObservation, Observations,
    Skipped, SourceRef, Sources,
};
pub use resolved::{
    AuthoredRow, Behaviour, DerivedFromStatus, ResolveCounts, ResolveReport, Resolved,
};
pub use stake::{
    StakeExclusion, StakeLabel, StakeLabels, StakeService, StakeServiceKind, StakeServices,
    StakeTableError,
};

/// The import file's format. Unchanged when the authored layer changed shape:
/// a consumer of `imported/registry.json` reads claims, and claims did not move.
pub const IMPORT_FORMAT: u32 = 1;

/// The format of `resolved/` and the behaviour artefacts. Bumped to 2 when a
/// credential became a list of uses with an optional shape, to 3 when `label`
/// and `project` became optional (the compiled table names most contracts and
/// operates few), to 4 when the shape names stopped saying `deadline` — the
/// same layout is a stake date on `$PERP` staking — and to 5 when a marketplace
/// gained a typed `purpose` and a machine `market` key, and a use could name
/// more than one address form.
pub const FORMAT: u32 = 5;
