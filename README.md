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

The import layer is the first piece. The authored layer and the published
artefact (`registry-vN.bin` + manifest, digest-pinned) follow, and the charter
records the governance and licence.
