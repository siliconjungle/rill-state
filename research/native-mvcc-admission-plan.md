# Native mvcc-core admission

The exact 0.1.0 published source and selected binary prove useful native
capabilities. Three tests cover 20,000-record complete retained snapshots across
32 revision traces with duplicate writes, deletes/reinserts and missing deletes;
partial transaction rollback and empty commits; and four actual reader-owned
transactions while a writer commits. Each reader begins on its own thread before
the writer proceeds and checks complete old values again after writing finishes.
The source/lock/compiler/frozen-binary/log receipt is
`evidence/native-mvcc-contracts-v1-receipt.json`.

Separate expected compiler failures prove `Transaction<Snapshot>` implements
neither Send nor Sync on this pinned build. Its epoch Guard and boxed internal
operation traits prevent transfer. Wrapping a transaction in a mutex does not
make it Send. This rules out the previously proposed direct Arc/Mutex adapter,
not the library's useful concurrency capabilities: reader-owned transactions
on separate threads pass. There is no public begin-at-arbitrary-timestamp API
in this source. The crate is in-memory only, so the older admission note about
optional durability was incorrect.

Keep two explicit adapter paths in the admission queue:

1. A native scoped-reader panel, with transactions remaining on their owning
   threads. Run the same scalar traces, exact revisions, ordered external feed,
   retained values and native transaction costs. Compare Rill through the same
   scoped interface. Full-value generic reads need a guard-bounded callback, not
   an escaped `&V`, no extra payload padding, and no unsafe lifetime extension.
2. An exported-view adapter for the existing transferable snapshot contract.
   Keep authoritative data only in the native database. After atomic commit,
   materialize the native snapshot into an immutable transferable map, charging
   the entire capture and allocation cost. No hidden shadow index or cached
   checksum. Name this export explicitly and retain the native scoped panel so
   export overhead is not misrepresented as intrinsic native read performance.

Both paths need full model/revision/ordered-feed and retained-reader proofs,
empty/duplicate/deleted-absent batches, generic payloads, and independent
reclamation accounting before comparative timing admission. Native transaction
retention affects epoch collection and watermark advancement; track stalled
readers and eventual release. Actual durability, schema indexes and multiple
writers are outside the current shared single-writer contract. Passing these
three capability tests is not passing those remaining requirements. No timing,
universal winner, source exclusion or 20,000-socket claim follows.

To reproduce the native proofs, enter `research/mvcc-core-admission-proof`, run
`ink-cache prepare-sources "$PWD"` outside measurements, then use the local Cargo
entry point with `RUSTUP_TOOLCHAIN=stable` and `cargo test --locked --lib --
--test-threads=1`. The `transfer-send` and `transfer-sync` binary features are
expected to fail `cargo check`; exact commands and logs are in the receipt.
All generated validation builds disable debug and incremental output. The
selected proof is unoptimized and supplies no performance comparison.
