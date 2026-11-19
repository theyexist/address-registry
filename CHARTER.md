# address-registry charter

Version 1 — 2026-11-19

This is the governance of a data repository, not of a product. It says who may
state what a Cardano script is, what evidence that statement needs, and what
happens when somebody disagrees.

## What this is

A machine-readable registry of Cardano scripts, keyed by 56-hex payment
credential: what a contract is, what it does with what it holds, and who says so.

It is ecosystem-wide and public. `theyexist` is the steward; anyone may
contribute a row.

## What this is not

- **Not a source of prices, balances or ownership.** It records what a script
  is; a walk records what it holds at a moment. Those are different questions
  with different lifetimes.
- **Not token metadata.** A metadata consumer resolves through the shared
  metadata chain, not through this repository.
- **Not a fork of Strica or Eternl.** It reads both, records the revision it
  read, and reproduces their claims without correction.
- **Not a claim of authority over anybody's script.** A row says what this
  registry believes and why, and can be withdrawn.

Scope is Cardano, keyed by payment credential. Key-based stake services are a
different table; a credential here is the payment part of a script address.

## The two layers

| layer | path | whose statement it is | how it changes |
| --- | --- | --- | --- |
| imported | `imported/` | Strica's and Eternl's | regenerated from a recorded revision |
| authored | `contracts/` | this project's | a reviewed pull request |

The imported layer is **claims**: a project's name for a script, with the parts
of its label kept separate. It assigns no behaviour and resolves nothing. Where
two upstreams give one credential different names, the disagreement is recorded
in `imported/report.json` and neither wins by import order.

The authored layer is **behaviour**: `kind`, datum `shape`, label, project and
the evidence. It overlays the claims and never rewrites them. Nobody outside this
repository is quoted as asserting a `kind` or a `shape` — which is why every
authored row requires a `source`.

## Evidence

A `kind` or a `shape` without a source is a fabrication with a filename.

Accepted evidence, strongest first:

1. **the datum**, observed on chain, with the tool and the date that observed it;
2. **the contract's own** published documentation or source;
3. **a public decoder** that already reads the shape, named by path
   (`mitos-vesting-decode::perps_staking`).

Not evidence: an inference from a token name, a marketplace's UI, or "it looks
like the other one".

`derived_from` carries either an `address`, whose payment credential is decoded
and compared to the row when the file loads, or an `attested` reason why no
address is registered. Attested rows are counted in `resolved/report.json`, so a
row nobody can re-derive is visible rather than looking checked. They are not
second-class; they are unverified, and the report says so.

## Stewardship

`theyexist` holds the repository, the publish path and the maintainer list.

The steward does not own the data, cannot relicense it, and does not settle a
dispute by assertion — it confirms that the process below was followed.

## Maintainers

Listed in [`MAINTAINERS.md`](MAINTAINERS.md). A maintainer may merge an authored
row that meets the evidence standard, and may withdraw one under the dispute
process.

## Disputes

Anyone may dispute an authored row by opening an issue with evidence. This is the
part of the charter that matters most: a registry is only worth what it does when
it is wrong.

1. The dispute is recorded in [`disputes/<credential>.md`](disputes/README.md) —
   what was claimed, on what evidence, what is said against it, and the dates.
2. A maintainer who did not author the row triages it.
3. **Silence favours Unknown.** If a dispute is not resolved within 30 days, the
   row is withdrawn: `contracts/<credential>.toml` leaves the tree, and the
   credential becomes unknown to the registry.
4. **Withdrawal does not delete the claim.** The dispute file keeps the row as it
   stood, including its `source`, so what was asserted and on what grounds stays
   auditable.
5. Resolved in favour → the row returns, with the new evidence added to its
   `source`. Resolved against → the dispute record is the reason it is absent.

Unknown is represented by **absence, not by a value**: `Behaviour::get` returns
`None`. A consumer that sees `None` must not read that as "safe" — only as
"nobody has stated what this is". A consumer that needs the reason reads
`disputes/`.

Disputes apply to the authored layer. An imported claim is its upstream's
statement and is disputed upstream; this repository will follow the correction on
the next import.

## Changes

- Authored rows and disputes: a pull request, reviewed by a maintainer.
- `imported/` is never hand-edited. Run `import` against the recorded revisions
  and commit the result.
- `resolved/` is never hand-edited. Run `resolve` and commit the result.
- The schema changes through `address-registry-schema`. **A field added is a
  `format` bump**, because readers reject keys they do not know rather than
  reading a registry they only half understand.
- This charter changes by a pull request to this file.

## Licence

| path | licence |
| --- | --- |
| `contracts/`, `resolved/behaviour.json`, `resolved/behaviour.bin`, `resolved/report.json` | CC0 1.0 |
| `imported/`, `resolved/registry.json` | its sources': Apache-2.0 (Strica), MIT (Eternl) |
| `crates/` | MIT OR Apache-2.0 |

The authored layer and the code are this project's to license. **The imported
layer is not.** It is a redistribution of two upstreams' data and carries their
terms; it is not relicensed here, and this repository cannot place it in the
public domain. The upstream notices are reproduced verbatim in [`licenses/`](licenses/).

Attribution is data, not boilerplate: every claim carries its `source`, and
`imported/registry.json` carries each upstream's repository and revision. A
consumer that must attribute has what it needs without parsing a licence header.

## Amendments

A pull request to this file, merged by the steward. The git history is the
record, and an amendment applies from the commit that made it.
