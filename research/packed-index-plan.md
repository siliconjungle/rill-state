# Packed index over dense rows

Status: implemented as opt-in `PackedIndexMap`; validation is in progress. No
performance improvement has been measured for this family.

The 80-way sharded scalar screen still has dense-publication and scan losses.
The previously measured flat inline table also clones empty-slot storage and
scans its capacity. Test a separate safe-Rust family with a dense `Vec<(K,V)>`
and a linear-probing `Vec<u64>` index. An occupied index word packs a cached
32-bit hash and a 32-bit row index plus one; zero means empty. Rows contain
complete keys and values. Always compare full key equality after the cached
hash matches, including raw zero/MAX and same-low-32-bit collisions.

This is an original implementation of a known dense-row/index pattern. The
pinned IndexMap 2.14.2 source is the relevant upstream control, not a claim
that the pattern is new. Its `src/lib.rs` Bucket stores a cached usize hash
beside K/V, and `src/inner.rs` uses hashbrown's index table and repairs the moved
index after swap removal. This experiment moves the cached truncated hash into
the index word, removing the per-row hash. Compare both native IndexMap and
IndexMap with exactly the same fast builder as the experiment.

Primary source: https://docs.rs/crate/indexmap/2.14.2/source/src/inner.rs

## Registered variations and limits

Sixteen fixed configurations: maximum fills 25/50/70/85, shared versus
inline storage for the same hash builder, and lazy per-operation versus once per
nonempty batch detachment. Once-per-batch pays for detachment even if the batch
contains only absent deletes; empty batches never detach. Use the same external revision and
exact ordered-journal adapter as other raw maps. Clone shares a whole table;
publication detaches it, so this remains a whole-table COW design and is not
expected to solve sparse owned-payload retention by itself. Dense iteration and
metadata-copy cost are hypotheses; an extra indexed row access may hurt reads.

Both row count and index capacity are explicitly bounded by the 32-bit encoding.
Reject growth beyond the supported index range before mutation. The 32-bit
cached hash is not a unique identity and is not the same adversarial collision
resistance as retaining all 64 builder output bits. Include deliberately equal
low-32-bit hashes in correctness tests and report the limitation. Keep standard
keyed-hasher and existing full-hash controls; do not silently substitute builders.

## Mutation and recovery

Growth redistributes cached index words without hashing/cloning rows. Insertion
must allocate/clone before making a new index word visible. For deletion, locate
the last row's index before mutation (a user Hash/Eq panic must leave the map
intact), redirect it to the removed row number, close the deleted probe hole,
and swap-remove the row. Drop removed user objects only after row/index validity
is restored. A failed table clone must leave retained roots and the current
table unchanged. Raw batches can partially apply; they are not transactions.

Test wraparound collisions, low-32-bit collisions, growth, borrowed keys,
canonical key preservation, delete/reinsert, all deletion victims, random traces,
retained views, final payload reclamation, and user Clone/Hash/Drop panics.

## Evidence sequence

First freeze/prove/measure the corrected sharded and upstream-control binary.
Then build this family separately. Run the sixteen configurations in parallel
through small/full scalar and generic exact-content/revision/feed checks and
four-reader probes. Select candidates using parallel development measurements,
then compare fixed configurations in serial fresh processes with hashbrown,
std-fast, indexmap-fast, persistent-map references and earlier fixed layouts.
Retain all losses. No per-workload oracle choice or holdout-seed use is allowed.
Final acceptance still requires the unchanged protocol's complete gate.
