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

## Expanded development data shapes

The next original candidate families are cached-hash linear probing with flat or COW
physical pages and split hash/value arrays, each generic over key/value/hasher types.
Five occupancies (25/50/70/85/90%) and five page sizes (flat/16/64/256/1024) provide 30
additional configurations. Add successful native adapters for contrie and leapfrog, and
keep SCC pending until its adapter stall is resolved. A deadline/stall is never a win.
The expanded inline suite adds exact 10%, 50% and 100% guaranteed-missing queries.

A separate `shapes` executable registers six data shapes: scattered u64 or 49-byte
String keys, each with 32-byte inline, owned 256-byte Vec, or shared Arc<[u8]> payloads.
It measures actual Rill Store (default and selected fast hash builders) alongside
hashbrown, imbl, imhm and the original probing candidates under the same atomic-batch,
retained-view and exact ordered-journal contract. Initial state, traces, batch inputs,
reader keys and warm-up are outside the interval. Each hit touches fixed-size payload
prefix/suffix data; complete byte-content/model comparisons and feed replay remain in
untimed proofs. Empty revisions reuse the state allocation. The initial view is pinned;
`slow` also pins up to 16 recent revisions, matching the inline protocol.

Register six key/value shapes, eight workload cases and both existing development seeds
before screening. Use parallel 512-record exact checks first and 20,000-record selection
runs. Generality selection complements the full primary matrix; it does not replace the
original registered workloads or authorize a holdout claim. Every final size/shape and
chosen engine still needs matching full-size proofs and the final serial acceptance gate.

## Fast persistent hashers and shared-record page extension

Add im/imbl/rpds fast-hasher variants using the same hashbrown DefaultHashBuilder
as the probing and rillfast candidates; retain each default-hasher baseline too.
This evaluates strongest available explicit settings, without conflating hasher
security guarantees. Generic im/rpds adapters use borrowed mutation to preserve
existing stored keys and avoid cloning replacement keys.

The next original shared-record family stores canonical keys and values behind
independent Arcs inside the existing probing pages. COW clones untouched handles
instead of String/Vec allocations; writes retain the canonical key, allocate a new
value and preserve old readers. Extra allocations, pointer loads and reference
counts are part of its cost, including for inline values. Register occupancy
50/70/85 and flat/16/64/256 pages (12 scalar layouts), with four occupancy70 layouts
in the generic six-shape matrix. Apply the same complete-content, ordered-feed,
revision and four-reader proofs before considering development timing. No changes
to the frozen holdout seeds or final acceptance criteria.

## Compact shared records and mixed-size extension

RecordMap packs the key and value into one shared immutable allocation. Slots
hold a cached full hash and one Arc handle (16 bytes on the tested 64-bit target,
versus 24 bytes for independent shared keys/values). The key must be cloned for
borrowed replacements, unlike SharedMap's canonical-key sharing. Register
occupancy 50/70/85 × flat/16/64/256 pages as 12 scalar configurations, with the
four occupancy70 layouts in the generic matrix. SharedMap's scalar adapter now
applies its existing borrowed input batch directly, removing an unnecessary
intermediate operation Vec; prior scalar comparisons include that avoidable cost.
Earlier generic shape timings do not have that cost and remain separate.

Add mixed-owned and mixed-shared shapes: unique ASCII String key prefixes of 19
bytes with deterministic suffix lengths [0,0,0,8,8,16,24,32,48,64,96,128,192,256,512,1024].
Payload length depends on (id+epoch) modulo 32: 0 (8/32), 8 (4/32), 32 (4/32),
128 (8/32), 512 (4/32), 2048 (3/32), 4096 (1/32) bytes. Thus key sizes vary across
records and payload sizes change over revisions. Owned Vec and shared Arc payloads
use identical logical bytes. Timed reads touch only prefix/suffix and length;
untimed proofs compare complete bytes, exact feeds and retained old revisions.

Run small exact proofs before full-size proofs. Selection uses both existing
development seeds and eight existing generic cases. This extends required
generality evidence and does not replace the original primary scalar matrix,
stalled-reader/reclamation work or final holdout gate.
