//! The check that matters for a consumer: the committed artefact deserialises
//! with the crate mitos will depend on, and `resolve` still produces it.
//!
//! Nothing here reads the upstream clones — the import is committed, and the
//! authored layer is a handful of files — so the test runs with no network and
//! no vendor directory.

use address_registry_schema::{
    Behaviour, DerivedFrom, DerivedFromStatus, Kind, ResolveReport, Resolved, Shape,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const TOOL_STAKING: &str = "b0ebdd68bd00f3a633c5097fe0243d69fb27abacd7f6959bec63c4c4";
const TOOL_LOCK: &str = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f";
/// Claimed upstream, with no authored behaviour — the common case.
const UPSTREAM_ONLY: &str = "002059fc75b038a85b33ba4fdfaced4bed1247fbad24c538dd45245a";

fn repo() -> PathBuf {
    // The crate lives two levels below the repository root, which is where the
    // committed import, contracts and resolved artefact are.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the repository root")
        .to_path_buf()
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_committed_artefact_reads_with_the_published_schema() {
    let resolved: Resolved = read(&repo().join("resolved/registry.json"));

    assert_eq!(resolved.format, address_registry_schema::FORMAT);

    let staking = resolved
        .behaviour(TOOL_STAKING)
        .expect("TOOL staking is authored");
    assert_eq!(staking.kind, Kind::Staking);
    assert_eq!(staking.shape, Shape::OwnerOnly);
    assert!(matches!(staking.derived_from, DerivedFrom::Attested(_)));
    assert!(!staking.source.trim().is_empty());

    let lock = resolved
        .behaviour(TOOL_LOCK)
        .expect("TOOL lock is authored");
    assert_eq!(lock.kind, Kind::Lock);
    assert_eq!(lock.shape, Shape::DeadlineAndOwner);
    assert!(matches!(lock.derived_from, DerivedFrom::Address(_)));

    // Neither is claimed upstream, and both are still in `credentials` — that
    // empty list is the behavioural gap, not a missing row.
    assert!(resolved.claims(TOOL_STAKING).is_empty());
    assert!(resolved.claims(TOOL_LOCK).is_empty());
    assert!(resolved.credentials.contains_key(TOOL_STAKING));
    assert!(resolved.credentials.contains_key(TOOL_LOCK));
}

#[test]
fn the_behaviour_artefact_is_the_targeted_read() {
    let behaviour: Behaviour = read(&repo().join("resolved/behaviour.json"));

    assert_eq!(behaviour.format, address_registry_schema::FORMAT);
    assert_eq!(behaviour.contracts.len(), 2);

    let staking = behaviour
        .get(TOOL_STAKING)
        .expect("TOOL staking is authored");
    assert_eq!(staking.kind, Kind::Staking);
    let lock = behaviour.get(TOOL_LOCK).expect("TOOL lock is authored");
    assert_eq!(lock.kind, Kind::Lock);

    // A credential only an upstream claims is absent, not an error. This is the
    // lookup a consumer with a handful of addresses makes, and it costs the
    // curated rows rather than the 825 imported ones.
    assert!(behaviour.get(UPSTREAM_ONLY).is_none());
}

#[test]
fn the_binary_table_decodes_to_the_same_rows_as_the_json() {
    let behaviour: Behaviour = read(&repo().join("resolved/behaviour.json"));
    let bytes = fs::read(repo().join("resolved/behaviour.bin")).expect("behaviour.bin is written");
    let decoded = Behaviour::from_bytes(&bytes).expect("the binary decodes");

    assert_eq!(decoded.generated_at, behaviour.generated_at);
    assert_eq!(decoded.contracts, behaviour.contracts);

    let json = fs::metadata(repo().join("resolved/behaviour.json"))
        .expect("the JSON exists")
        .len() as usize;
    assert!(
        bytes.len() < json,
        "the binary ({} B) is not smaller than the JSON ({json} B)",
        bytes.len()
    );
}

#[test]
fn resolve_reproduces_the_artefact_from_the_committed_inputs() {
    let out = scratch();
    let status = Command::new(env!("CARGO_BIN_EXE_address-registry"))
        .arg("resolve")
        .arg("--imported")
        .arg(repo().join("imported/registry.json"))
        .arg("--contracts")
        .arg(repo().join("contracts"))
        .arg("--out")
        .arg(&out)
        .status()
        .expect("the binary runs");
    assert!(status.success(), "resolve exited {status}");

    let fresh: Resolved = read(&out.join("registry.json"));
    let committed: Resolved = read(&repo().join("resolved/registry.json"));

    assert_eq!(fresh.format, committed.format);
    assert_eq!(fresh.sources.strica.rev, committed.sources.strica.rev);
    assert_eq!(fresh.sources.eternl.rev, committed.sources.eternl.rev);
    assert_eq!(fresh.authored.len(), committed.authored.len());
    assert_eq!(fresh.credentials.len(), committed.credentials.len());
    for (credential, contract) in &committed.authored {
        let fresh_contract = fresh
            .behaviour(credential)
            .unwrap_or_else(|| panic!("{credential} dropped by a fresh resolve"));
        assert_eq!(fresh_contract.kind, contract.kind);
        assert_eq!(fresh_contract.shape, contract.shape);
        assert_eq!(fresh_contract.source, contract.source);
        assert_eq!(fresh_contract.derived_from, contract.derived_from);
    }

    let report: ResolveReport = read(&out.join("report.json"));
    assert_eq!(report.counts.authored, 2);
    assert_eq!(report.counts.authored_only, 2);
    assert_eq!(report.counts.derived_from_verified, 1);
    assert_eq!(report.counts.derived_from_attested, 1);

    let fresh_behaviour: Behaviour = read(&out.join("behaviour.json"));
    let committed_behaviour: Behaviour = read(&repo().join("resolved/behaviour.json"));
    assert_eq!(fresh_behaviour.contracts, committed_behaviour.contracts);

    let fresh_bytes = fs::read(out.join("behaviour.bin")).expect("the binary is written");
    assert_eq!(
        Behaviour::from_bytes(&fresh_bytes)
            .expect("the binary decodes")
            .contracts,
        committed_behaviour.contracts
    );

    let staking = report
        .authored
        .iter()
        .find(|row| row.credential == TOOL_STAKING)
        .expect("the staking row is reported");
    assert_eq!(staking.derived_from, DerivedFromStatus::Attested);

    let lock = report
        .authored
        .iter()
        .find(|row| row.credential == TOOL_LOCK)
        .expect("the lock row is reported");
    assert_eq!(lock.derived_from, DerivedFromStatus::Verified);

    let _ = fs::remove_dir_all(&out);
}

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!("address-registry-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    path
}
