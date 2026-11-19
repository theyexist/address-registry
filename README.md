# address-registry

A machine-readable registry of Cardano scripts, keyed by payment credential —
what a contract is, what it does with what it holds, and who says so.

The governance, the evidence standard, the dispute path and the licence are in
[`CHARTER.md`](CHARTER.md).

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

One file per credential, `contracts/<credential>.toml`, named after the
credential it describes. A file declares the credential once and carries one or
more `[[use]]` tables, because one address can do more than one job:

```toml
credential = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f"

[[use]]
category = "lock"
shape = "timestamp_and_owner"
label = "TSURU Lock"
project = "TSURU"
source = "token-ledger walk 2026-11-19: datum Constr 0 [Int(ms), Bytes(owner_pkh)]"

[use.derived_from]
address = "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j"
```

- `category` is what the thing is: `exchange`, `marketplace`, `launchpad`,
  `burn`, `airdropper`, `minter`, `defi`, `staking`, `vesting`, `lock`, or
  `wallet`. The first nine are the compiled table's names, so a consumer migrates
  by rename; `lock` and `wallet` are the two it never had.
- `shape` is optional and says how the datum lays out its arguments:
  `owner_only`, `timestamp_and_owner`, or `timestamp_and_owners` for a list. It
  is absent for anything that holds nothing under a datum — a `wallet`, which is a
  key address an operator uses, or a script whose datum nobody has read yet.
- `datum_example` is optional and needs a `shape`: a real datum observed at this
  credential, as lowercase hex CBOR. The registry does not decode it, so it cannot
  claim the bytes match the shape. What it can claim is where they came from, and
  a consumer's decoder is what asserts they read as declared.
- `source` is required and must not be empty. A behaviour with no provenance is
  a fabrication with a filename.
- `derived_from` is either an `address` whose payment credential must decode to
  `credential`, or an `attested` reason why no address is registered. The first
  is decoded and compared when the file loads — a mistyped hex character fails
  the build rather than naming an address that exists. The second is counted in
  the report, so an unverifiable use never looks like a checked one.
- A second `[[use]]` is how an overloaded address is described. The registry
  lists the uses and never picks between them: only on-chain evidence can, at the
  consumer — a datumless UTxO is the wallet use, a datum matching
  `timestamp_and_owner` is the lock use.
- Unknown keys are an error, not a silent drop.

```sh
cargo run -p address-registry -- validate contracts/<credential>.toml
```

`validate` applies the same rules `resolve` applies — the file name must be the
credential, an `address` must decode to it, an `attested` must say why — and
writes nothing. It names every failing file rather than the first, because a
contributor is fixing files rather than discovering one error per run.

```sh
cargo run -p address-registry -- resolve
```

`resolve` reads `imported/registry.json` and `contracts/`, and writes
`resolved/registry.json` (the union), `resolved/behaviour.json` and
`resolved/behaviour.bin` (the curated rows only). A credential with an authored
use and no upstream claim appears with an empty claim list; that gap is the reason
the registry exists.

## Import

Both upstreams are read from local clones, never fetched by the tool, so an
import is reproducible from the revisions in `imported/registry.json`.

```sh
git clone --depth 1 https://github.com/StricaHQ/cardano-contracts-registry vendor/strica
git clone --depth 1 https://github.com/Tastenkunst/eternl-cardano-registry vendor/eternl

cargo run -p address-registry -- import --strica vendor/strica --eternl vendor/eternl
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

## Checks

CI runs the commands a contributor should:

```sh
nix build .#                                  # the package installs
nix flake check                               # the suite, plus the binary's usage line
nix develop -c cargo fmt --all --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix run . -- validate                         # every contract file, no writes
```

A second CI job runs `validate` over the contract files a pull request changes,
so a malformed file is reported by name instead of as a failed artefact
comparison. It reads the rest of `contracts/` too: a new file that repeats a
credential another file already declares is only wrong in context.

`nix flake check` runs the suite, which **regenerates the artefacts in a
temporary directory and compares them with the committed ones**. A `contracts/`
edit that was not followed by a `resolve` therefore fails CI rather than drifting
into the repository, and a `format` bump that was not regenerated fails the same
way. Lint runs through the devshell, so CI and a contributor use one toolchain.

## Using it from another repo

Two different things, and keeping them apart is the point:

| you want | depend on | changes when |
| --- | --- | --- |
| the shape of the files | `address-registry-schema` (this repo) | a field, kind or shape is added |
| the registry itself | a published object (see below) | a contract is added or renamed |

There are three artefacts, because consumers ask different questions:

| you need | fetch | today |
| --- | --- | --- |
| a classification | `resolved/behaviour.bin` | 14 KB (7.5 KB gzipped) |
| every field as text | `resolved/behaviour.json` | 25 KB (7.7 KB gzipped) |
| that, plus who else names them | `resolved/registry.json` | 450 KB (63 KB gzipped) |

The binary's advantage is the **index**, not the byte count: gzipped the two
behaviour files are within 3% of each other, because the labels and sources are
the bulk and compress the same way. What the binary buys is addressability — a
fixed-width record per use, so a reader computes an offset instead of parsing a
document.

The first two are the curated rows and nothing else. A consumer holding five
credentials resolves all five from them; it does not pay for 825 imported claims
it will never look at. The claims are the bulk of the third file and are almost
always the wrong question.

`behaviour.bin` is the one shaped for lookup: a magic and format header, then
`count × 36` fixed-width index records sorted by credential, then the strings. An
index record is the 28-byte credential, the record's absolute offset, its length,
and a `category` and `shape` code — so a reader that only classifies fetches 36
bytes, and one that wants the label fetches one more range. A credential with two
uses has two adjacent records, so a lookup is a binary search and a scan of the
run. The layout is specified in `crates/address-registry-schema/src/behaviour.rs`
beside its encoder.

The crate is the schema, not the data. `address-registry-schema` has no I/O, no
network and one dependency (`serde`):

```rust
// The targeted read. `get` returns every use: empty when nobody has stated one.
let behaviour: address_registry_schema::Behaviour = serde_json::from_str(&text)?;
for use_of in behaviour.get(&credential) {
    // use_of.category, .shape, .label, .project, .derived_from, .source
}

// From an address, which is what a walk actually holds. A reward address is
// refused by name: its credential sits where a payment one would and is a
// different thing.
let uses = behaviour.get_for_address("addr1…")?;

// The union, when the names matter too.
let resolved: address_registry_schema::Resolved = serde_json::from_str(&text)?;
if resolved.format != address_registry_schema::FORMAT {
    // a file from a newer schema than this build understands
}
let names = resolved.claims(&credential); // empty when nobody upstream claims it
```

A consumer that pins the crate therefore stops doing the thing that was fragile:
adding a contract to the registry no longer needs a rev bump anywhere. Adding a
*field* does, and bumps `format`, because deserialisation rejects keys it does
not know rather than reading a registry it only half understands. The same rule
is what makes a typo in a `contracts/*.toml` an error instead of a silent drop.

## Publication

**Published from this repository.** The artefacts in `resolved/` are the
published tree, `main` is the pointer, and a commit is the publish:

```
https://raw.githubusercontent.com/theyexist/address-registry/main/resolved/behaviour.bin
https://raw.githubusercontent.com/theyexist/address-registry/main/resolved/behaviour.json
https://raw.githubusercontent.com/theyexist/address-registry/main/resolved/registry.json
```

Pin an exact version by putting a commit before the path:
`…/address-registry/<sha>/resolved/behaviour.bin`.

Measured against these hosts on 2026-11-19:

| host | `Range` | CORS | cache |
| --- | --- | --- | --- |
| `raw.githubusercontent.com` | yes — `206` with `content-range` | `*` | 300 s |
| `cdn.jsdelivr.net` | no — a ranged request returned the whole body | `*` | 7 d / 12 h |

Why a bucket is not needed yet: the objects are self-describing (`format` and
`generated_at` are inside them), every published byte has a commit and a diff,
and there is no credential to hold. What that costs: no SLA, up to five minutes
of pointer lag (`?v=` did not bust the cache when measured), and the repository
must stay public — a private one makes `raw` need auth and breaks anonymous
consumers.

R2 earns its keep when one of these becomes true: a guaranteed SLA, an instant
pointer flip, a custom domain or response headers, a volume story GitHub should
not be carrying, or a consumer doing large-index range reads. None is true at
423 KB, and `raw` already answers range requests, so the binary works where it
is. When that changes the same artefacts go to `<prefix>/latest.json` plus
digest-named immutable objects, the pointer is written last, and the publisher
gains a `--run`-gated upload step; no file shape changes.

One property to keep in mind either way: the index is 36 bytes a row, so a
full-index read costs 36 × rows — 36 KB at a thousand curated rows, 360 KB at
ten thousand. Past ~100 KB of index (about 2,800 rows), shard to
`behaviour/<first byte>.bin`. Below that, one small fetch beats the bookkeeping.

## Status

The import layer, the authored layer, the resolved artefacts and the charter are
in place, and the artefacts are published by being committed. The authored layer
carries **61 uses**, covering every credential the compiled
`shared-crates/address-registry` table carried before this repository existed.
