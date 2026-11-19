# Disputes

One file per disputed credential, `disputes/<credential>.md`. It exists so that
withdrawing a row never deletes what was claimed or why — the claim is withdrawn,
not erased.

The charter's rule: a dispute unresolved after 30 days withdraws the row, and the
credential becomes Unknown. Withdrawal is this file plus the deletion of
`contracts/<credential>.toml`, in the same pull request.

## Shape of a dispute file

````md
credential: <56 lowercase hex>
status: open | withdrawn | upheld
opened: YYYY-MM-DD
opened_by: <github handle, or "anonymous">

## The claim, as it stood

```toml
<the contracts/<credential>.toml file, in full, including source>
```

## Evidence against

<what is wrong, and what shows it>

## Evidence for

<what still supports the row, if anything>

## Log

- YYYY-MM-DD — opened by <handle>: <one line>
- YYYY-MM-DD — <decision, by whom, and why>
````

## Rules

- The `## The claim, as it stood` block is a verbatim copy. Editing it as the
  dispute proceeds would destroy the record it exists to keep.
- Evidence is held to the same standard as an authored row: a datum observation,
  the contract's own documentation, or a public decoder. Not a token's name.
- `status: upheld` means the row stands. That is a real outcome and belongs in
  the file, not in a deleted issue.
