# address-registry

A machine-readable registry of Cardano scripts, keyed by payment credential —
what a contract is, what it does with what it holds, and who says so.

## Two layers

| layer | path | written by | regenerated |
| --- | --- | --- | --- |
| **imported** | `imported/registry.json` | `address-registry import` | on demand, from pinned upstreams |
| **authored** | `contracts/<credential>.toml` | a human, in a PR | never |

The imported layer carries **claims**: the project and contract name each
upstream gives a credential, with the upstream's revision recorded. It never
assigns a `kind` or a datum `shape`.

The authored layer carries the **behaviour**: what the script is, how a datum
names its owner, and the provenance for that claim. It overlays the imported
claims; it does not replace them.

## Authored

A contract is one file, `contracts/<credential>.toml`, named after the
credential it describes:

```toml
credential = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f"
kind = "lock"
shape = "deadline_and_owner"
label = "TSURU Lock"
project = "TSURU"
source = "token-ledger walk 2026-11-19: datum Constr 0 [Int(ms), Bytes(owner_pkh)]"

[derived_from]
address = "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j"
```

- `kind` is what the script is (`lock`, `staking`); `shape` is how the datum
  lays out its arguments (`owner_only`, `deadline_and_owner`). They are separate
  because the same shape can carry a deadline or a stake date, and only the
  `kind` says which.
- `source` is required and must not be empty. A behaviour with no provenance is
  a fabrication with a filename.
- `derived_from` is either an `address` whose payment credential must decode to
  `credential`, or an `attested` reason why no address is registered. The first
  is decoded and compared when the file loads — a mistyped hex character fails
  the build rather than naming an address that exists. The second is counted in
  the report, so an unverifiable row never looks like a checked one.
- Unknown keys are an error, not a silent drop.

```sh
cargo run -- resolve
```

`resolve` reads `imported/registry.json` and `contracts/`, and writes
`resolved/registry.json` — the authored rows plus the union of every credential,
with the imported claims beside them. A credential with an authored row and no
upstream claim appears with an empty claim list; that gap is the reason the
registry exists.

## Import

Both upstreams are read from local clones, never fetched by the tool, so an
import is reproducible from the revisions in `imported/registry.json`.

```sh
git clone --depth 1 https://github.com/StricaHQ/cardano-contracts-registry vendor/strica
git clone --depth 1 https://github.com/Tastenkunst/eternl-cardano-registry vendor/eternl

cargo run -- import --strica vendor/strica --eternl vendor/eternl
```

The tool writes:

- `imported/registry.json` — every claim, keyed by 56-hex payment credential
- `imported/report.json` — counts, rows dropped, and conflicting claims

## What it guarantees

- A credential is exactly 56 lowercase hex. A row that is not one is dropped
  and reported with its source file and value.
- Every claim names its source (`strica` or `eternl`) and carries the parts of
  its label (`project` + `contract`), never a pre-joined string.
- A credential two projects claim is a **conflict** in the report. Nothing is
  resolved silently, and no label wins by import order.
- The import is deterministic: files are read in sorted order, claims are
  sorted, and credentials are sorted.
- Eternl's generated `registry/scripts/script-index.json` is **not** a source.
  Its generator merges into whatever was there before, so it accumulates rows no
  project file claims. What it holds and how far it has drifted is recorded in
  `report.json` under `observations.eternl_script_index`, and none of it is
  imported.

## Status

The import layer and the authored layer are in place. The published artefact
(`registry-vN.bin` + manifest, digest-pinned) follows, and the charter records
the governance and licence.
