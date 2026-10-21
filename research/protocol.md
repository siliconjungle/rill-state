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

## Packed dense-row experiment

The separate [registered plan](packed-index-plan.md) compares sixteen fixed
`PackedIndexMap` configurations: fills 25/50/70/85, shared/inline builder storage,
and lazy-per-operation/once-per-nonempty-batch detachment. Dense iteration avoids
scanning empty index slots; an extra indexed lookup and whole-table COW can
regress reads/publications. Cached 32-bit hashes never replace full key equality.
Report the explicit row/capacity limits and hash-truncation tradeoff. Include
native and matching-fast-hasher IndexMap controls. Successful unit/recovery tests
do not establish full-size behavior, general performance or final acceptance.

## Embedded sharded-inline experiment

After the flat slice's publication/retention regressions, register independently
growing inline COW tables in an embedded fixed-size directory. One global builder
hashes a key once; bits starting at 32 select the table, while its normalized full
cached hash selects the local probe home and guards full key equality. Zero/MAX
normalization collisions must still preserve distinct keys. Root clones copy the
directory's Arc handles, and mutations detach touched tables. This avoids a
separate heap directory load but adds routing and root-clone work; no speed or
memory gain is assumed.

Register 80 configurations for both scalar and generic executables: directory
counts 1/16/64/256/1024, maximum fills 25/50/70/85, shared/inline storage for the
same builder and per-operation/grouped batching. Grouped batching hashes all
input keys once, counts/scatters indices stably by table and detaches each table
once between grows. Both scratch Vec allocations, both passes and directory
scans are timed. Only per-key state order is required across reordered tables;
the unchanged external journal retains every original operation in exact order.
Clone/drop side effects across keys have no ordering guarantee, as with existing
COW cloning. These are configurations of one original family, not 80 libraries.

Prove collisions/wrap/deletion/growth, canonical metadata/reinsertion, borrowed
keys, zero/MAX fingerprints, retained old views and final payload release. A
counted-value probe must show only the touched table copied; a counted builder
must show exactly one hash per operation even during growth. Run small/full
scalar and eight-shape matrices with the existing seeds, plus actual four-reader
checks. Streaming retention must include multiple table counts and both batching
policies. Parallel timings remain selection evidence. Serial comparisons include
hashbrown, persistent-map references and the strongest previous fixed layouts.
Keep every regression visible; final holdout seeds and requirements are unchanged.

## Contiguous inline-slice experiment

Register InlineSliceMap with four maximum fill thresholds (25/50/70/85), shared
versus inline storage for the same hash builder, and contiguous-span versus
indexed-wrap reads: 16 configurations, not 16 additional libraries. Store the
cached hash as NonZeroU64 so Option can use its niche; raw hash zero normalizes
to MAX and may collide with MAX, with full key equality still required. Normalized
hashes consistently determine both lookup homes and deletion/growth placement.
Use one Arc slice of inline entries, with no per-entry allocation. Copying a
retained slice clones all live keys/values; owned heap payload regressions must
remain visible alongside inline-value results. Batched mutation acquires the
writable slice once between grows. This is a mutable-map research layout; the
unchanged common adapter supplies atomic revisions and exact ordered journals.

Before timing, require the collision/wrap/zero/MAX/borrowed-key oracle, canonical
key preservation/reinsertion, growth with retained views, Clone-panic detachment
and final payload reclamation. Run both development seeds and every original
scalar case plus every generic shape/case, followed by retained-reader probes.
Parallel screens select candidates only. Serial fixed-candidate and matched-
policy descriptions preserve every loss and cannot substitute for the final
holdout gate. All holdout seeds and acceptance requirements remain unchanged.

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
versus 24 bytes for independent shared keys/values). The canonical stored key must be cloned for
borrowed replacements, unlike SharedMap's canonical-key sharing. Key identity
remains consistent with the existing map adapters, even when Eq ignores metadata. Register
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

## Guarded-backend copy adapters

Pinned native progress probes hold an old reader while another thread attempts
three publications. flashmap, evmap and left-right each completed one, then
waited until that old guard was released; all three then completed and the old
value stayed unchanged. These are capability observations, not speed rankings.

Register flashmap-copy, evmap-copy and left-right-copy using a freshly owned
native backend for each staged nonempty revision. Clone copies the old complete
contents; native mutation is batched before publishing the new backend. The
common journal preserves every ordered original operation. Reads use native
guards. The evmap and left-right read handles are not Sync, so their adapter
read path uses a Mutex; writer Mutexes supply Sync eligibility but use get_mut
on exclusive mutation paths. flashmap uses its conditional Sync read handle
directly. flashmap/evmap use native default hashers; left-right uses an explicitly
initialized hashbrown map cloned into both sides so replay has identical hash
state. Copying, guard acquisition and synchronization are included in adapter
costs, which must not be described as native short-lived-reader throughput.

These are additional configurations/capability adaptations, not claims of three
new independent persistent snapshot algorithms or universal performance wins.
Full scalar content/revision/feed proofs and concurrent old-reader probes are
required before timing.

## Instrumented streaming retention/reclamation capability probe

The retention_profile example tracks live Value instances, value clones and the
sum of their owned Vec lengths through actual Drop/Clone operations. The Value
layout is recorded (40 bytes on this target); counters are instrumentation, not
the allocator or the total library heap. Process peak RSS includes metadata and
these instrumented fixture types and is not a speed or uninstrumented Vec result.

Stream one batch at a time, with 20,000 scattered u64 keys, 200 cyclic updates
per revision and 256 revisions. No precomputed future trace or duplicate payload
oracle retains obsolete values. Compare hashbrown and imbl copy/COW adapters,
ProbeMap, independent shared values, compact record flat/paged variants and actual
Rill Store. Every engine keeps an exact 64-batch/1,000,000-operation journal. Test
recent-view windows 0/16/64, with/without an independently pinned initial view,
and fixed256/mixed0..4096 payload sizes: 84 full-size cases. A smaller two-window
screen first covers 56 cases at 2,000 records.

Check every retained snapshot against a payload/epoch model, derive exact journal
counts independently from batch lengths, release the pinned old root, then recent
views, then the store. After enough full update cycles, releasing the initial root
must drop exactly 20,000 old stored values; after all owners drop, both tracked
instances and payload bytes must be zero. These are ownership/memory observations,
not speed claims or complete physical-heap bounds.

## Fixed-size array pages and canonical-key sharing

Register ArrayRecordMap using Arc<[Slot; PAGE]> instead of Arc<Vec<Slot>> for
COW pages. Each slot contains a full cached hash and an optional immutable
record Arc. Array pages remove the per-page Vec length/data header and separate
payload allocation. Safe Rust only; PAGE must be a power of two in 1..=4096.

Compare two compile-time key policies: CopyKeys clones the canonical stored key
into each replacement; SharedKeys retains that key through Arc and clones the
handle instead. Both preserve the original key's observable metadata, borrowed
lookup, exact Eq/Hash matching, old snapshots and backward-shift deletion. Sharing
adds key indirection and allocations, which must be measured even for small keys.

The scalar matrix adds 32 configurations: two policies × maximum fill thresholds
25/50/70/85 × page widths 16/64/256/1024. These are configurations, not independent
libraries. Generic shape selection adds all eight policy/page combinations at
maximum fill70, four policy/page16-or256 combinations at fill50, plus stronger
existing record-flat(fill50) and probe-flat(fill25) read baselines. Initial fill
and warm-up remain outside the interval; no per-case engine switch is allowed
for final acceptance. First prove small/full-size exact contents, revisions and
ordered feeds, then threaded old views. Parallel screens are selection only;
serial development comparisons retain all gains and regressions. No change to
original primary requirements or untouched final holdout seeds.

## Shared versus inline hash builders

Add an explicit HasherPolicy to array pages. SharedHasher remains the default
and preserves support for builders without Clone. OwnedHasher stores a Clone
builder inline and requires its clone to preserve hash outputs (the same law
needed when cloning a standard HashMap). This removes the per-lookup builder-Arc
indirection at the cost of cloning the builder on map-stage copies. No security
algorithm is changed: both timed variants use the same hashbrown builder type.

Register 32 inline-builder scalar configurations matching every v20 key-policy,
fill threshold and page width. Generic selection adds both key policies at
fill50/page16-or256 with inline builders, plus matched shared/inline-builder
fill25/page64-or1024 pairs. The latter thresholds were selected from the earlier
parallel scalar screen for stronger read settings; parallel ranks are not final
evidence. Prove seeded-builder cloning, non-Clone shared-builder support and
retained snapshots before comparing timings. All existing final criteria remain.

## Stronger standard-library and dense-index controls

Add std-fast and indexmap-fast scalar controls with the same hashbrown default
builder as the original fast layouts, keeping their native-default controls too.
Extend generic shapes with standard HashMap (native/fast), standard BTreeMap and
IndexMap (native/fast). Borrowed updates preserve canonical keys; explicit
swap_remove matches pinned IndexMap's deprecated remove alias. These are settings
and newly covered shapes of existing libraries, not additional independent crates.
Hash-builder security tradeoffs remain explicit. Require full state/revision/feed
and retained-reader proofs before development timing; no missing control counts
as beaten. New controls are compiled into a separately bound frozen executable.

## Borrowed-value full oracle and recovery capability

The next generic verifier compares every visited complete value directly with
the independent BTreeMap model, records unique cloned keys in a BTreeSet and
requires exact reported/visited cardinalities. It independently checks point
lookup for every model key, in addition to each existing missing/read query.
Old revisions and exact ordered feeds retain their checks. It avoids cloning
payloads merely to inspect them. Negative-control tests must reject duplicate,
missing and unknown rows, wrong lengths, middle-payload corruption and bad point
lookups; a non-Clone payload test prevents a hidden payload-clone requirement.
This strengthens coverage while reducing verification allocations; it is not
a library-speed claim. Prior frozen executable proofs continue unchanged.

Mutable research maps must also keep counts and probe reachability valid after
a caught user Clone/Drop panic, although a batch can partially apply. Negative
proofs and corrected regression inputs are retained. Atomic engine publication
continues to use external staging; no transactional claim is added to raw maps.
