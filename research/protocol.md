# Comparison contract and admission protocol

This study is active. The objective is to develop a generic Rust snapshot/change-feed
library and test a broad set of implementations, seeking superiority over every
applicable measured competitor. It has not yet passed that objective.

## Shared contract

Single writer, multiple independently retained readers. A publication atomically applies
an ordered batch of puts/deletes. Every snapshot reflects one complete revision and
remains unchanged across later publications. Records and keys are generic in the library.
The change journal preserves every submitted operation in order, including duplicate keys,
empty revisions and deletion of nonexistent keys. Cursor gaps are explicit. Persistent
maps are comparison building blocks with a common external journal. Concurrent mutable
maps receive an explicit copy adapter because a native weak iterator is not an independently
retained immutable snapshot. Their native concurrent-map capabilities remain useful;
the adapter cost must not be described as native point-operation performance.

No durability, network, schema, game, transport, query language or connection benchmark
is included in this contract. 20,000 logical readers is not proof of 20,000 live sockets.
Application-retained snapshots/batches can retain unbounded memory regardless of journal
eviction. Native transactions with stronger isolation have additional capabilities.

## Selection

Register the complete candidate list and source hash in each run receipt before launch.
Run full-content BTreeMap-oracle checks and ordered mutation-vector checks in parallel.
Use development seeds 4815162342 and 2937471921. Vary directory layouts, page counts,
branch sizes, sorted pages, hash pages, overlay depth and existing persistent structures.
Parallel development timings may select designs; they cannot establish final superiority.
Keep failed candidates and failures visible in the selection record. The same initial
state, trace, batch semantics, snapshot retention and work counts apply to every engine.
Every initial state case pins its pre-measurement snapshot; `slow` additionally retains
up to 16 recent views. Unpinned reclamation is a separate required experiment.
No timing-dependent work omission, competitor timeouts counted as wins, or per-case oracle
selection of a different final engine is allowed.

Initial screen: 20,000 records, 20,000 logical readers (one lookup per reader per revision),
32-byte inline values and scattered u64 keys; 12 rounds, two seeds. Cases: idle publication,
single change, 1% sparse changes, hot-key changes, delete/reinsert churn, 50% burst,
full replacement, slow readers retaining 16 revisions, static point reads, complete scans,
20,000 snapshot acquisitions, and 20,000 change-cursor checks. Generality validation must
also add variable-length keys/values, misses, collisions, different sizes and real threaded
readers before making a broad conclusion. Short-screen p99 is diagnostic only.

## Final evidence required

Freeze one generic selected implementation and parameters before holdout timing. Run
fresh processes serially, alternating randomized paired order against every applicable
competitor. Holdout seeds 783294811237 and 923884193077; at least seven independent process
repetitions, at least 64 measured revisions, full contract proofs on the same traces.
Record binary/source/compiler/dependency hashes, wall time, publication/read breakdown,
process CPU and peak RSS. Measure steady-state reclamation and stalled-reader memory.
Independently check retained snapshots and replaying deletions/batches. Concurrency tests
must detect torn revisions and distinguish acquisition overhead from already-held views.

For an across-the-board speed claim, require a paired process-level log-ratio bootstrap
95% upper bound below 1 against every admitted competitor on every registered primary
workload, with a multiple-comparison correction. Report memory, tail latency and capability
tradeoffs separately; an average win cannot hide an individual loss. Native atomic-reference
swap primitives are a separate publication-layer comparison, not complete state engines.
Missing comparisons, compile failures, unproven contracts or any loss leave the original
objective incomplete. Publish the public library and research with that status explicit.
