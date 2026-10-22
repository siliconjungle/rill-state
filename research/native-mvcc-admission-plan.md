# Native mvcc-core admission

The exact 0.1.0 published source and selected binaries prove useful native
capabilities. Four tests cover 20,000-record complete retained snapshots across
32 revision traces with duplicate writes, deletes/reinserts and missing deletes;
partial transaction rollback and empty commits; and four actual reader-owned
transactions while a writer commits. Each reader begins on its own thread before
the writer proceeds and checks complete old values again after writing finishes.
The source/lock/compiler/frozen-binary/log receipt is
`evidence/native-mvcc-contracts-v2-receipt.json`, which links the original
negative trait checks. An additional capacity proof holds 20,000 distinct native
transactions simultaneously across four owning threads during sixteen writer
commits. Every session checks sampled old values before and after writes; first
and last sessions on each worker additionally check full contents. Native active
transaction counts are exactly 20,000 before/after writing and zero after all
readers join. These are logical native sessions, not 20,000 sockets or threads.

Separate expected compiler failures prove `Transaction<Snapshot>` implements
neither Send nor Sync on this pinned build. Its epoch Guard and boxed internal
operation traits prevent transfer. Wrapping a transaction in a mutex does not
make it Send. This rules out the previously proposed direct Arc/Mutex adapter,
not the library's useful concurrency capabilities: reader-owned transactions
on separate threads pass. There is no public begin-at-arbitrary-timestamp API
in this source. The crate is in-memory only, so the older admission note about
optional durability was incorrect.

The registered v34m2 scalar panel implements two explicit adapter paths:

1. A native scoped-reader panel, with transactions remaining on their owning
   threads. Run the same scalar traces, exact revisions, ordered external feed,
   retained values and native transaction costs. Compare Rill through the same
   scoped interface. Full-value generic reads need a guard-bounded callback, not
   an escaped `&V`, no extra payload padding, and no unsafe lifetime extension.
2. An exported-view adapter for the existing transferable snapshot contract.
   Keep authoritative data only in the native database. Materialize the
   transaction's own staged writes before commit, then publish its transferable
   map after successful commit. Charge the entire native scan, map construction
   and allocation cost. A capture failure drops the uncommitted transaction and
   rolls it back. No hidden shadow index or cached
   checksum. Name this export explicitly and retain the native scoped panel so
   export overhead is not misrepresented as intrinsic native read performance.

Both paths need full model/revision/ordered-feed and retained-reader proofs,
empty/duplicate/deleted-absent batches, generic payloads, and independent
reclamation accounting before comparative timing admission. Native transaction
retention affects epoch collection and watermark advancement; track stalled
readers and eventual release. Actual durability, schema indexes and multiple
writers are outside the current shared single-writer contract. Passing these
four capability tests is not passing those remaining requirements. No timing,
universal winner, source exclusion or 20,000-socket claim follows.

To reproduce the native proofs, enter `research/mvcc-core-admission-proof`, run
`ink-cache prepare-sources "$PWD"` outside measurements, then use the local Cargo
entry point with `RUSTUP_TOOLCHAIN=stable` and `cargo test --locked --lib --
--test-threads=1`. The `transfer-send` and `transfer-sync` binary features are
expected to fail `cargo check`; exact commands and logs are in the receipt.
All generated validation builds disable debug and incremental output. The
selected proof is unoptimized and supplies no performance comparison.


The scalar comparison uses the original unpadded 32-byte values. Native lookups
copy only those original values from scoped Ref guards. Native scoped snapshots
clone Rc/RefCell session handles on their owning thread; that acquisition is not
a transferable snapshot score. Export modes separately capture into capacity-
reserved hashbrown/std-fast maps or BTreeMap, with per-operation and last-write
coalescing controls. Ordered external journal operations remain uncoalesced.
A maintained scalar length changes only from actual native update/delete results;
there is no authoritative shadow map. Empty revisions retain the previous
exported map and still advance the journal cursor.

The optional root comparison dependency disables derive macros; its pinned
Versioned row implementation uses the crate's public hidden schema helper types.
It adds only mvcc-core to the existing lock; every prior locked package identity
is preserved. The default library remains dependency-free. This optional native
comparison requires the upstream crate's Rust 1.96 MSRV.

The original timing loop, warmup/input preparation and exact oracle are byte-
identical to the earlier harness. `run` retains an explicit Send/Sync view bound;
only the private common loop accepts the native reader-owned thread hook. See
`research/native-mvcc-harness-change-review.json` for section hashes. New timings
must compare controls in this new binary. The v34m2 plan registers eight fixed
configurations of one upstream crate plus fourteen references. Its extra
contract probes cover duplicate/absent-delete feeds, retained roots, empty
revisions, gap/future boundaries and precommit capture failure. Generic payload
and reclamation probes remain pending, as does any performance admission.

The unmeasured initial v34mvcc build is superseded before any runtime study.
The selected v34m2 source handles empty logical revisions as journal/revision
metadata only, matching the existing COW controls, without opening an avoidable
native transaction or rebuilding a snapshot. Its exact prior source remains in
Git; unselected generated build outputs are retired explicitly.
