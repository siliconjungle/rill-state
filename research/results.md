# Results and open work

The original objective (generic public Rust library outperforming every applicable
competitor across the registered workloads) is **not achieved**. This document records
progress, not acceptance. The public implementation is experimental.

## Additional native backends and shared-record transfers

The 22-engine scalar admission panel now completes 660 small and 660 full-size
state/revision/ordered-feed proofs, 88 actual four-reader probes and 660 parallel
development selections with matching outputs. It uses actual chashmap and
crossbeam-skiplist packages, with ten copying/scanning/batching configurations
and twelve fixed controls. The native-map selection review preserves every
fixed candidate and loss. Generic scoped-read admission and isolated/final
timings remain pending; native concurrent-writer throughput is not ranked.

The branch32 v28 panel now completes all 10,670 selected full-size and threaded
proof processes, independently audited. Branch4 generic/threaded work remains
in progress. The adaptive v30 panel completes all 47,400 small scalar/generic
proofs and 1,080 selected full-size scalar proofs. Its optimized raw cold panel
completes 2,520 processes and 40,320 independently audited rows. Those parallel
raw timings include substantial conversion costs and do not establish final
engine admission or service latency. All 312 optimized full-size streaming
retention/reclamation processes also pass, including populated traversal caches.

The audited uncached 20,000-record memory fixture identifies 20,000 extra payload
clones during shared-flat conversion. The opt-in ReuseSharedFlat policy now
transfers existing immutable record handles in both directions, keeping the
copying policies as controls. All 100 core/experimental tests and all-target
admission Clippy pass. Thirteen source-bound frozen tests cover 240 raw policies,
unchanged-value clone bombs and pointer identity, changed-value clone counts,
partial deltas, rehash/reverse key failures and final tracked-value release.
Four actual readers cover five representative modes at 20,000 records; this
does not prove every policy's full engine/threaded contract. The registered
paired transfer performance panel has not yet run. See
`research/shared-record-transition-plan.md` and
`evidence/adaptive-contracts-v2-receipt.json`.

## Adaptive cold-transition and cache smoke proofs

The registered v30 diagnostic smoke cohorts now pass 568 fresh processes at
1,000 records: 360 mixed-density raw-map processes and 208 streaming retention
processes. The cold audit independently reconstructs the Rust random trace and
checks every checksum, representation transition and traversal-cache count over
5,760 rows. It covers 24 fixed adaptive policies plus six controls, including
std BTreeMap, across six key/value shapes and two development seeds.

Retention checks cover six adaptive policies and seven controls, two retained
view windows, initial-root retention on/off and fixed/mixed payload lengths,
with zero or two complete read traversals after each revision. Traversals clone
no tracked payload values, old roots retain their complete expected values and
dropping the final roots/journal/cache owners releases all tracked payloads.
The cache counters count shared handles, not total heap bytes.

These are smoke correctness proofs. Their unoptimized example executables reuse
completed optimized Cargo library artifacts. The independent optimized Cargo
diagnostic builds are complete. Their current proof/measurement status is recorded
above. Receipts bind source, compiler, commands
and dependency artifact hashes. Smoke timings are excluded from speed rankings.
Transfer-policy diagnostics, full-engine comparison and final isolated holdout
acceptance remain pending. See
`evidence/adaptive-cold-v30smoke-audit.json`,
`evidence/retention-v30smoke-audit.json` and
`research/v30-diagnostics-plan.json`.

The separate 20,000-record cold correctness cohort also passes all 360 processes
and 5,760 independently audited rows across the same 30 implementations and six
shapes. Its proof-only executable and timings remain excluded from speed rankings.
The corresponding full-size streaming retention cohorts are running. See
`evidence/adaptive-cold-v30proof20k-audit.json` and the additional immutable
`research/v30-full-proof-diagnostics-plan.json` registration.

Both comparison binaries and the optimized diagnostic examples have completed
their frozen builds. The completed proof totals and remaining stages are recorded
above; no adaptive performance admission follows yet.

## First development comparison

79 configurations: 28 existing-library/baseline adapters, 18 independent alternative
configurations and 33 layouts of the first Rill implementation. These are configurations,
not 79 different third-party libraries. The registry catalog queried 52 names, including
unavailable names and out-of-scope or pending entries; discovery is not a benchmark result.

- 1,896 exact behavioral checks at 512 records passed, two seeds and 12 workload shapes.
- 632 full-content checks at 20,000 records passed, four changing-state workload shapes.
- 1,896 parallel development measurements at 20,000 records/readers completed with output
  agreement. They ran with up to three candidate processes, overlapping other required
  validation. They are not isolated, serial, statistically confirmed superiority evidence.
- Native MVCC was added next: 120 small exact checks passed; the 20,000-record screen then
  caught output disagreement in the initial skipdb adapter on deletion/repeated-key churn.
  This adapter is invalid for performance acceptance. Its failure remains retained; the
  corrective coalesced adapter requires fresh full-size proof and measurements.

The first store does not win. Fast hashbrown whole-table-copy adapters led large update
cases and point reads. imhm led the first sparse/slow-reader screens. Compact sorted pages
led full scans. Default Rill snapshot cloning initially performed two reference-count
operations; a subsequent version moved the hasher into the shared root, reducing it to one.
Later comparisons add faster-hasher variants and indexed value pages; their outcomes must
be evaluated independently. A fast-hasher result must not be advertised as the performance
of the default keyed hasher.

## Capability and fairness notes

The benchmark adapts map primitives to revisioned state plus an ordered change journal.
It does not imply that every source library natively implements a change feed. Mutable
concurrent maps are cloned before mutation for independent snapshot retention. Their
results describe that adapter and do not rank their native concurrent throughput.
Persistent collections receive native COW updates; immutable-chunkmap also receives a
sorted/coalesced batch adapter, including sorting cost. Native skipdb supplies stronger
transaction facilities than the single-writer common contract needs.

Every measured state case pins the pre-measurement initial snapshot; `slow` additionally
pins up to 16 recent views. This is explicitly a retained-snapshot workload. Unpinned
steady-state reclamation, varied retention windows and live worker concurrency remain
required. The initial state screen uses scattered u64 keys and 32-byte inline records;
generic String/heap-value correctness exists, but broad generic performance is unproven.

The default library has no dependencies, no unsafe code and no storage/network requirement.
Retained old snapshots do not change when new batches commit. Cursor identity, revision
exhaustion, journal gaps, operation ordering, zero limits, hash collisions, panicking input
and panicking value clones are covered by independent tests. A release/reclamation test
checks that dropping the last pinned snapshot releases an evicted value allocation.

## Reproducibility and publication status

Exact initial source: commit `5ff9cb7ddc86063d71121c01bc145af250522def`.
Native MVCC comparison was committed subsequently; each run binds the complete Rust
source/manifest/lock hash and executable hash in its receipt. Frozen executables are local
retained proof inputs, not architecture-portable release binaries. The source history and
lockfile are the public reproduction inputs.

The target machine is Apple arm64 with 8 logical CPUs and 8 GiB RAM. Other active workers
were not stopped. No final holdout gate, across-the-board win, sustained p99 guarantee or
20,000 network-connection capacity result is established. Public-repository creation is
a source publication; this is not a crates.io release.

## Required next evidence

Finish full-size proof and development selection across all current variants; broaden
variable-sized key/value shapes, hit/miss distributions and state sizes; compare remaining
applicable source libraries and primitives; measure threaded publication and stalled-reader
memory; optimize a single generic implementation; then freeze it and run the strict serial
holdout protocol. Any individual loss remains a failure of the original performance goal.

## Expanded second screen

The expanded version measured 125 configurations: 29 external/baseline adapters,
30 independent alternatives and 66 Rill layouts/hasher configurations. It completed
3,000 small full-content proofs, 1,000 full-content proofs at 20,000 records (including
the corrected skipdb adapter), and 3,000 parallel development measurements with matching
outputs. These counts do not represent distinct third-party libraries. The native-MVCC
adapter now coalesces repeated keys for state writes while journaling every original op.

Five reference-publication primitives were probed at 1, 2, 4 and 8 threads. Nineteen
of twenty initial probes passed. `swap-arc` 0.1.1 failed the quiescent final-revision
check at eight threads on this arm64 machine; a separate eight-thread rerun failed
9 of 12 attempts. This is a failing benchmark-contract observation, not a completed
upstream root-cause analysis or a claim about other architectures. It remains visible
and investigative in CI; no fresh-snapshot performance result for it is admitted.
The other publication observations are behavior probes, not isolated speed results.

The first core optimization removed a redundant snapshot reference count. Fast-hasher
variants and indexed value pages are promising in individual cases but do not yet
establish superiority. The default generic library remains the original COW-page
family with the keyed standard hasher. Indexed-slot alternatives currently live in
the comparison harness pending a justified generic implementation selection.

## Baseline review correction

The first adapter copied mutable maps even for empty revisions. This is unnecessary:
an empty revision can reuse the immutable map allocation and advance only snapshot
metadata and the journal. Consequently the initial idle/static-read/scan/acquisition
measurements are provisional and must not support a superiority claim. A follow-up
changes all adapters to reuse unchanged state and adds explicit revision values and
old-revision checks to every snapshot view. The earlier raw measurements remain
available as an audit trail. New proofs and measurements must use the corrected driver.

The corrected driver passed 3,000 small and 1,000 full-size complete-content/revision
checks across all 125 configurations. A selected 12-engine serial development comparison
uses the same development seeds, 32 revisions and three process repetitions. It is not
the frozen holdout comparison, does not cover every candidate, does not establish a
multiple-comparison-corrected win, and is not the final acceptance gate.

The serial development comparison completed all 864 fresh processes with output
agreement. At 20,000 records/readers, the corrected hashbrown adapter led the selected
single/sparse/burst/replacement/point-read cases. Indexed value pages narrowed the gap
but did not beat it. Compact sorted pages led scans; acquisition results clustered
closely. These descriptive outcomes reinforce that the current Rill implementation
has not passed the original goal. See `evidence/serial-v7-development` for raw timings,
per-process medians and the registered candidate list. No holdout seeds were consumed.

## Generic probing and missing-key extension

The generic original probing modules are opt-in research APIs (`experimental` feature),
not the selected default engine. They support arbitrary cloned keys/values, compatible
borrowed keys, collision-safe lookup, resize, backward-shift deletion and retained COW
pages. Independent String/heap-value tests include collision wraparound, repeated-key
batches, deletes, growth, old-view immutability and final allocation release.

The 157 non-stalled configurations passed 4,710 512-record complete-content/revision/
ordered-feed checks across 15 workloads and both development seeds. This includes
contrie and leapfrog adapters, plus 30 new original layouts. Leapfrog's reserved value
sentinels are represented with an explicit tag so the complete 32-byte benchmark value
space is preserved; the resulting storage cost belongs to that adapter. SCC is pending:
its native simple API and retention probes completed, but the snapshot adapter stalled
in the oracle phase. This is unresolved, not a claim that SCC itself is incorrect or
slower. The executor-reset interrupted v8 runs are explicitly marked incomplete; their
partial output is not a completed measurement or proof gate.

The public Store now exposes its current cursor without snapshot acquisition and avoids
an unnecessary root reference-count pair on empty commits. It still advances the empty
revision and journals it. The generic data-shape performance experiment is separately
registered and must finish before drawing any conclusion about heap/string performance.


## Heap shapes and snapshot thread eligibility

The initial generic-shape phase passed 960 exact small checks and completed 960
20,000-record selection processes with matching outputs. These are six actual data
shapes, not a performance claim across all generic types. Baseline review then found
avoidable replacement-key clones in the first hashbrown/imbl/imhm shape adapters;
borrowed mutation now avoids those clones. Earlier shape timings are provisional.

The new 20,000-record inline proof phase passed 530 of 532 tasks; the two failures were
leapfrog churn length mismatches. A minimal native u64 example confirms that published
leapfrog 0.3.3 can report length zero after a delete/reinsert while lookup and iteration
still contain the replacement. The single-owner snapshot adapter now maintains exact
external cardinality. It also uses a Mutex for shared reads: native leapfrog HashMap
is Send but not Sync. Its earlier unlocked timings are not admitted to the cross-thread
snapshot contract. A distinct native concurrent LeapMap adapter is included separately.
All comparison map/view types now require Send + Sync; no unsafe Send/Sync assertion
was added to bypass a library's ownership requirements.

Actual four-reader retained-view checks are a separate proof mode; readers keep an
old immutable revision while the owner publishes the changing trace, then the final
state is compared against the independent model. They are behavior probes, not speed
claims or 20,000 network-connection evidence.

SCC remains unresolved. Its simple native probes pass even after extending retirement
to 128 iterations, while two snapshot-adapter forms hit the bounded diagnostic deadline.
The native copied form and the capture-before-insert form are not performance wins and
are not admitted as fully correct engines. Other candidate work proceeds independently.
The original performance goal is still unpassed.

## Strengthened full-size proof and lookup optimization

The corrected borrowed-key generic baseline passed 960 exact checks at 20,000
records and 240 four-reader retained-view probes (v12). The padding-free leapfrog
adapter then passed all 30 large scalar checks (v13). Earlier failed observations
remain retained rather than counted as performance wins.

The contiguous-span ProbeMap lookup change passed 420 full-size scalar layout
checks, 288 full-size generic shape checks and 72 four-reader probes (v14). Both
v13 and v14 serial development comparisons completed 648 fresh processes each
with matching outputs: six engines, six shapes, three cases, two development seeds
and three repetitions, with 12 measured revisions per process. These descriptive
comparisons are not the registered final gate. The paged16 prototype's read median
fell by roughly 3.5–16.7% across the six tested shapes, but it still lost reads to
hashbrown. Some flat-layout cells regressed. The paged prototype led the selected
sparse String/owned256 case; hashbrown led the selected read and dense-update cases.
See evidence/shapes-v14-serial-development/descriptive-change.json for all reported
changes, including regressions. No holdout seeds have been consumed.

The original broad performance objective remains unpassed.

## Faster persistent baselines and shared-record COW pages

The im/imbl/rpds fast-hasher variants use the same explicit hashbrown builder as
the probing candidates, while default-hasher baselines remain available. Generic
im/rpds adapters avoid replacement-key cloning through borrowed updates. Five
new generic competitor configurations passed 480 exact 20,000-record checks
(v15). The new original shared-record family shares canonical keys and values
inside probing pages, trading reference counts and pointer reads for less copying
of untouched heap data. Its four occupancy70 generic layouts passed 384 exact
full-size checks. The combined new competitors and shared layouts passed 216
four-reader retained-view checks (v16).

The full affected scalar suite passed 1,380 exact 20,000-record checks: all 30
Probe/Split layouts, 12 shared-record layouts, three fast persistent competitors
and hashbrown, across 15 cases and both development seeds. Core/experimental
tests (17) and Clippy passed locally. These results prove tested behavior rather
than performance superiority. Serial development measurements follow separately;
the final holdout gate has not been run.

SCC follow-up: a standalone native API program now reproduces the stall in a
clone chain with retained old maps and full contents/read checks. All four bounded
variants hit the 10-second diagnostic deadline near revision 21: native clone or
explicit capture into a fresh-hasher map, each with retirement or with all roots
kept. Thus simply delaying root retirement or replacing the copied hasher did not
resolve this observation. The frozen binary, source commit/hash and complete logs
are bound in evidence/scc-native-modes-v16/receipt.json. This is a reproducible
native observation on this arm64/toolchain combination, not a completed upstream
root-cause analysis, a universal library-failure claim or a benchmark victory.

The 1,728-process serial v16 development comparison completed with exact output
agreement (12 engines × six shapes × four cases × two development seeds × three
process repetitions; 12 measured revisions). In the selected String/owned256
burst fixture, the shared flat layout median was 2.090 ms/revision versus
hashbrown's 3.215 ms (about 35% lower); sparse shared-paged64 was 0.642 ms versus
1.873 ms. However, every read-only shape still favored hashbrown, and numeric
inline burst favored hashbrown by a large margin. Shared storage also introduces
regressions for several inline/shared-value cases. All 288 descriptive cells,
publication/read breakdowns and process RSS are retained, without significance
or across-the-board claims. The original objective remains unpassed.

Native SCC source inspection after stack sampling identified a capacity-growth
hypothesis: Clone passes capacity() to with_capacity_and_hasher(), which treats
its input as an entry count and adds table slack before power-of-two rounding.
Our capture adapter repeated the same pattern. Repeating it can grow a table
without growing its live records. The sample showed a 4.9 GB physical footprint,
with active iteration/drop work rather than a confirmed mutex deadlock. A new
bounded capacity trace and allocation-by-live-length correction must prove the
cause and validate the corrected adapter; prior deadline observations remain
retained and are never counted as wins.

The bounded native capacity trace confirmed growth from 1,024 to 262,144 slots
in eight copies at a fixed 512 records. Both live-count modes kept exactly 1,024
slots for all 128 copies, with retirement and with every old root retained. The
corrected scc-capture adapter then passed all 30 full-size exact case/seed checks
at 20,000 records and four four-reader probes. Native Clone's capacity growth
remains a pinned-version capability issue, but the corrected capture adapter can
now enter the measured comparison. This replaces the earlier unresolved cause;
the failing frozen observations remain available.

The default driver lists leave native scc-copy investigative, with an explicit
record in the admission queue; it is not counted as a win or silently omitted
from the research. Native capacity reproduction is bounded to eight copies, and
CI requires the corrected 128-copy native mode and capture adapter. A separate
owned-process-group timeout proof confirms that a deadline retires both its own
time wrapper and child, rather than leaving an orphan benchmark running.

The corrected SCC comparison completed all 450 serial development processes with
matching outputs: five selected engines × all 15 scalar cases × two development
seeds × three repetitions, at 32 measured revisions per process. Complete
75-cell descriptive medians are retained in evidence/state-v17-scc-serial-development,
including publication/read breakdowns and process RSS. This is a newly admitted
adapter comparison, not native SCC concurrent throughput or the final holdout
gate. No missing or failed native-copy variant is counted as a win.

## Compact records, mixed sizes and broader retention proof

The new original compact-record family puts canonical keys and values in one
shared allocation. Its slot is 16 bytes versus 24 for separate shared key/value
handles on the tested 64-bit layout. Replacements preserve stored-key metadata
even when equality ignores it. Twelve maximum-load/page configurations are
registered; they are parameter configurations, not independent libraries.
Twenty-one core/experimental tests and Clippy passed. An unnecessary intermediate
scalar SharedMap mutation Vec was also removed; earlier scalar timings include
that avoidable adapter cost, while earlier generic-shape timings do not.

All 1,650 small and 1,650 full-size scalar layout checks passed. The generic
matrix passed 2,944 small and 2,944 20,000-record complete state/revision/feed
checks, plus 736 four-reader old-view probes: 23 configurations, eight data
shapes, eight cases and both development seeds. New mixed shapes vary actual
String key lengths and owned/shared payload lengths across records/revisions.
The scalar parallel development screen completed 1,740 matching-output processes
across 58 configurations. Parallel results are selection evidence only.

Independent guarded-backend adapters for flashmap, evmap and left-right passed
90 small and 90 full-size checks collectively, plus 12 four-reader probes. Their
240-process parallel development comparison completed with matching outputs;
copying, guards and required read-handle Mutexes remain explicit adapter costs.
They are not claimed as native short-reader throughput or native persistent
snapshot engines.

The instrumented streaming retention probe passed 56 small and 84 full-size
cases across seven configurations, three retention windows, two initial-pin
modes and two payload distributions. Every retained view matched its model,
journal holdings matched independently derived exact counts, unpinning a fully
replaced initial state released exactly 20,000 tracked stored values, and dropping
all owners released every tracked value/payload byte. With 64 recent snapshots,
a pinned initial root, fixed256 payloads and the common 64-batch journal, the
compact paged and independently shared layouts ended at about 15.97 MiB of owned
payloads versus 320.51 MiB for the hashbrown whole-table-copy adapter. This is
payload ownership accounting on an instrumented 40-byte Value type, not total
heap usage or a speed result. Full process RSS and every cell are retained.

Frozen executable receipts are now checked against original Git source hashes,
so new implementations can build while earlier versions continue running without
mislabeling their inputs or copying whole worktrees. Final holdout seeds remain
untouched. The original across-the-board performance objective remains unpassed.

## Complete serial development sweep after compact records

The generic serial sweep completed 8,832 fresh processes with output agreement:
23 configurations × eight shapes × eight cases × two development seeds × three
repetitions, at 12 measured revisions per process. All 1,472 per-configuration
cells and all raw rows are retained, including defaults, fast-hasher variants,
mixed sizes and every tested regression. Separately, the guarded-backend serial
comparison completed 720 matching-output processes across eight configurations,
all 15 scalar cases, both development seeds and three repetitions at 32 measured
revisions. No other candidate process ran concurrently in these serial sweeps;
other users' workers were not stopped. These are development comparisons, not
the registered 64-revision/seven-repetition holdout acceptance gate.

For the fixed record-paged16 candidate versus fixed shared-paged16, read-only
medians fell by about 1.3–12.7% across the eight tested shapes. Hashbrown still
led every read-only shape. The compact candidate had a lower total median than
hashbrown in 26 of 64 shape/workload cells and a higher median in 38; it therefore
fails the original across-the-board target. Dense String/mixed-key workloads
also regressed versus independently shared keys, consistent with cloning the
canonical String into each compact replacement record. No per-case envelope of
different original layouts is presented as one universally winning engine.
See fixed-candidate-comparison.json for all 64 direct comparisons.

Disassembly of the retained compact String getter shows the paged path loading
a Vec header's length and separate payload pointer before scanning a page. A
next read-locality experiment should compare fixed-size Arc array pages with
these existing Vec-backed pages, preserving safe Rust, independent snapshots
and the same full contract. The assembly was inspected without retaining an
insight-only full dump. This is a hypothesis, not a measured improvement.

The original goal remains open, with source admission gaps and read-performance
losses still explicit. Final holdout seeds remain unused.

## Fixed array pages and hash-builder storage: completed proofs

The v20/v21 experiments remove the Vec header and separate payload allocation
from each compact-record page by storing fixed-size Arc arrays. Canonical keys
can be copied into replacement records or shared through a separate Arc. A
second policy compares sharing the hash builder with keeping that same builder
inline; it does not change the hashing algorithm. Thirty core/experimental
tests and Clippy pass, including collisions, zero hashes, borrowed lookups,
canonical-key metadata, key reincarnation, seeded builders and final reclamation.

Both versions passed 960 small and 960 full-size scalar proofs separately.
The v20 generic matrix passed 1,792 small and 1,792 full-size checks plus 448
four-reader probes. The v21 generic matrix passed 1,536 small and 1,536 full-size
checks plus 384 four-reader probes. Every full-size run uses 20,000 records and
20,000 logical reads per measured revision; these are not network connections.
Configurations and policy combinations are not counted as additional libraries.

Each version's parallel scalar screen completed 1,170 processes with matching
logical outputs across 39 configurations. Those screens still show hashbrown
leading read-only, sparse-update and dense-update cases. Removing an indirection
has not established an actual speed improvement. Fresh serial matched-policy
comparisons are running separately; no final holdout measurement has started.

The v20 streaming retention extension passed 56 small and 84 full-size probes
across four array layouts and three existing references. All retained snapshots
matched their models, independently derived journal counts matched, and all
tracked payloads were released after their final owner. With fixed256 payloads,
64 recent snapshots and an initial pin, the array layouts retained 15.97 MiB of
owned payloads versus 320.51 MiB for the whole-table-copy hashbrown adapter.
Larger pages nevertheless retained more metadata: instrumented peak process RSS
was 34.80 MiB for array-copy16 and 58.09 MiB for array-copy256, versus 450.97 MiB
for hashbrown in that cell. These are instrumented process/payload measurements,
not library-only heap sizes or timing results. Every cell and reproduction input
is retained under evidence/retention-v20-20k.

## Matched array policies: complete serial development evidence

The v21 serial sweeps completed 11,082 fresh processes with matching logical
outputs: 8,832 generic processes (23 configurations, eight shapes, eight cases,
two development seeds and three repetitions at 12 revisions) and 2,250 scalar
processes (25 configurations, 15 cases, both seeds and three repetitions at 32
revisions). No other owned candidate proof/build/timing process ran concurrently.
Other users' workers remained running. Every cell is retained, and the reporting
tools require exact engine/shape/case/seed/repetition coverage before summarizing.

For the fixed array-owned-copy-low16 candidate, keeping the builder inline gave
lower medians in 40 of 64 cells than its otherwise matched shared-builder
counterpart and higher medians in 24; five read-only shapes improved and three
regressed. Against hashbrown, that same fixed candidate was lower in 29 cells
and higher in 35, losing all eight read-only shapes. Removing a pointer alone
therefore does not solve the original objective. These are descriptive medians,
not paired confidence intervals or corrected acceptance results.

The scalar fixed probe-25-0 candidate had a 0.10881 ms read-only median per
revision versus hashbrown's 0.10199 ms (about 6.7% higher). In burst updates it
took 0.44423 ms versus 0.19849 ms. Those measurements motivate the next inline
slice experiment, preserving inline values without individual record allocations.
Its nonzero-hash niche uses 48-byte numeric slots versus the prior tagged layout's
56 bytes on this 64-bit target; complete key equality still distinguishes raw
zero/MAX hash collisions. Sixteen configurations are registered. Thirty-eight
core/experimental tests and Clippy pass; full-size proofs precede any timing.
The selected default Store and the final holdout gate remain unchanged.

The inline-slice phase passed 480 small and 480 full-size scalar checks across
all 16 configurations, plus 2,048 small generic checks. Its 690-process parallel
screen completed with matching outputs; that screen overlapped untimed proof/
validation work and is selection evidence only. Full-size generic and actual
concurrent-reader checks are still running. The expanded all-target comparison
Clippy check passes after naming complex types, making intentional shared-empty
initialization explicit and calling IndexMap's swap_remove directly (the pinned
deprecated remove method simply delegates to swap_remove).

The inline streaming retention study passed 48 small and 72 full-size probes:
four representative inline configurations and two references, all retention
windows, both initial-pin modes and both payload distributions. Every full view
and independently derived journal count matched; all tracked payloads were
reclaimed after final ownership release. With 64 recent views, an initial pin
and fixed256 values, all four flat variants retained 320.51 MiB of owned payloads,
the same as the whole-table-copy hashbrown adapter, versus 15.97 MiB for the fixed
record-paged16 reference. Instrumented peak RSS ranged from 463.92 to 819.47 MiB
for the flat variants, versus 440.38 MiB for hashbrown and 34.09 MiB for the paged
reference in that cell. This confirms a real retention regression, not a total
heap or timing conclusion. All 72 cells remain visible.

The remaining 2,048 full-size generic checks and all 512 actual four-reader
probes subsequently passed. The independent audit verifies exact tuple coverage,
all executable completion markers and frozen source/binary/compiler/driver
bindings across all five v22 proof matrices (5,568 processes total). It does not
replace the internal complete-view/revision/feed assertions or establish speed.
Fresh serial development comparisons are now running against the same fixed
references; the final holdout seeds remain unused.

The new all-target CI Clippy gate exposed a newer-toolchain deprecation of
AtomicU64::fetch_update. Rust 1.85 compatibility requires the older API, so only
that constructor statement has a documented deprecation exception. The allocation
algorithm and measured frozen binaries are unchanged; local all-target Clippy
passes. Both Rust 1.85 and current-stable CI remain required.

## Inline-slice scalar serial comparison

All 2,070 scalar serial processes completed with matching outputs: 23 fixed
configurations, all 15 cases, two development seeds and three repetitions at 32
measured revisions. The fixed inline-owned-index-25 candidate had a read-only
median of 0.08956 ms per 20,000-lookups revision versus hashbrown's 0.10272 ms,
about 12.8% lower. It had lower total medians in four cases and higher medians
in eleven. Its sparse, burst, full-replacement and slow-reader medians were
respectively 2.136, 1.885, 1.736 and 3.538 times hashbrown's. This is a specific
read improvement with substantial remaining regressions, not final acceptance.

The matched shared-builder/span-probing inline candidate versus the older
probe-25-0 candidate had lower read, burst and replacement medians by about
10.6%, 14.2% and 11.5% in this same binary's sweep. Both use the same builder
storage and probing style; the new layout changes the Arc slice and slot
representation. Indexed probing also improved many but not all matched cells.
Every fixed candidate and policy contrast remains visible. Whole-process CPU and
RSS include setup outside the timing interval. Short-run revision-percentile
medians remain diagnostics, without service-capacity or tail-latency claims.

## Complete inline generic serial comparison

The eight-shape sweep completed all 8,832 serial processes with matching outputs:
23 configurations, eight shapes, eight cases, two development seeds and three
repetitions at 12 measured revisions. Together with the scalar sweep this is
10,902 processes for v22; the v21 and v22 serial phases total 21,984 fresh runs.
Every fixed layout, matched policy and regression remains in the complete data.

The fixed inline-owned-index-25 candidate had lower total medians than hashbrown
in five of 64 generic cells and higher medians in 59. Its read-only ratios were
0.958/0.984 for mixed-owned/shared, 0.992/1.041/1.022 for String inline/owned/shared,
and 1.052/1.076/0.974 for u64 inline/owned/shared respectively. Thus four read
shapes improved descriptively and four regressed; the 12.8% scalar read advantage
does not establish a general getter advantage across these call sites and data
shapes. The fixed array-owned-copy-low16 reference remains lower in 29 cells
and higher in 35, also losing every read-only shape. Neither is an overall winner.

The next implementation needs to reduce copying on publication while preserving
read locality; the flat layout's owned-payload retention and update regressions
are measured constraints. Source admission remains open. Rust 1.85, current-stable
and comparison CI all passed for code revision 1fe9e7d19e5ce4dfd5e05fc4383d95c3523a21cd.
No holdout seed or final acceptance criterion has changed. Experimental layouts
remain opt-in, and the original across-the-board objective has not passed.

## Embedded directories, stronger controls and recovery hardening

Eighty sharded-inline configurations compare five embedded directory sizes,
four maximum fills, two builder-storage policies and two batching strategies.
All 2,400 small and 2,400 full-size scalar normal-behavior checks passed, as did
10,240 small generic checks. The 2,610-process parallel scalar screen completed
with matching outputs; it overlapped required proof/compilation work and is
selection evidence only. All 10,240 full-size generic checks and 2,560 four-reader probes also passed.
Independent audits verify the complete tuple sets, completion markers and frozen
source/binary/compiler/driver bindings for these original v23 executables.

The streaming memory extension passed 56 small and 84 full-size cases. Every
tracked payload was reclaimed, each retained view matched its model and exact
journal counts matched. With 64 recent views, an initial pin and fixed256
payloads, shard-shared-group-70-1024 retained 69.95 MiB of owned payloads versus
320.51 MiB for both the flat read-leading layout and hashbrown's copy adapter,
and 15.97 MiB for record-paged16. Instrumented process RSS in that cell was
117.27, 730.44, 402.27 and 34.52 MiB respectively. Other directory/load choices
retained much more memory. These are v23 instrumented payload/process results,
not total library heap, current-version speed or a universal memory win.

The control matrix now includes native/fast standard HashMap and IndexMap plus
generic standard BTreeMap, preserving native defaults and the explicit hasher
security distinction. Its scalar extension passed 90 small and 90 full-size
checks plus 12 four-reader probes; its six-control generic matrix passed 768
small and 768 full-size checks plus 192 four-reader probes. No new control is
counted as another independent crate or as beaten without matching measurements.

Fault injection exposed recovery errors in the experimental raw maps. The
corrected source stages replacement tables, prepares affected probing pages
before deletion, defers removed-object drops and guards outer sharded counts
while unwinding. All 61 contract/oracle tests and all-target Clippy pass. The
original narrow paged-failure probe assumed an iterator/growth order; it was
replaced with every 32-by-32 victim/failure pair, which reproduced actual lookup
corruption against the retained pre-fix library. The selected failing binaries,
exact inputs and correction review are retained. Default Store publication
staging is unchanged; raw-map batches remain nontransactional.

The updated generic oracle compares borrowed full values, checks unique iteration
and checks every model-key lookup without cloning payloads. Malformed iterator,
length, payload and lookup controls pass their rejection tests. Fresh corrected
executables will be proved and measured separately. No new backend is selected;
the original across-the-board performance objective and final holdout gate remain
open, and the holdout seeds remain untouched.


## Corrected execution panel and packed dense-row prototype

All original sharded proofs, including the full generic and four-reader sweeps,
now pass. The independently audited v23/v24 matrices total 29,760 fresh processes.
The corrected v25 executables are frozen at source 840abc6c31e98ee4d734315ec1f5d385001b52d5.
A predeclared development panel runs fresh small/full/threaded proofs, broad
parallel screens and then serial comparisons. Its serial panel has 20 fixed
scalar configurations and 19 generic configurations, including matching-fast
standard HashMap/IndexMap controls. All phases have separate immutable labels;
no new serial result is claimed before those phases finish.

The next original family, PackedIndexMap, implements dense complete K/V rows and
8-byte cached-32-bit-hash/index words. Sixteen options vary fill, builder storage
and when a writable table is acquired. Empty batches never detach; once-per-batch
pays to detach even for a batch containing only absent deletes. All 72 tests pass,
including deliberate equal-low-32-bit hashes, zero/MAX words, every removal victim
across three collision homes, canonical/borrowed keys, retained roots, final
payload release and Clone/Hash/Drop failures. Growth's counted builder proof uses
exactly one hash per inserted input, without hashing existing rows. A deletion
can hash the last dense row to repair its index; that work is not omitted.
This new family has no comparative timing or full-size admission yet. Its whole
COW table still clones owned payloads, and cached hash truncation and bounded
indices remain explicit limitations. No default selection or holdout gate changed.


The packed retention extension passed 48 corrected small and 72 full-size
instrumented cases. Every retained view and exact journal count matched, and all
tracked payloads were reclaimed. In fixed256/window64/pinned-initial, all four
packed configurations retained 320.51 MiB of owned payloads and made 5,191,200
instrumented Value clones, matching hashbrown's copy adapter. The record-paged16
reference retained 15.97 MiB and made 71,200 clones. Packed process RSS ranged
from 412.30 to 462.47 MiB versus hashbrown's 433.97 MiB and the paged reference's
34.62 MiB. These are same-build instrumented process/payload observations;
structural/allocator overhead is excluded from payload counts and no speed or
universal heap improvement follows. Dense storage does not solve whole-table
owned-payload retention. The initial 64-round small launch was invalid because
it did not replace every initial record; it produced no measurements. Its exact
receipt/reproduction and the subsequent driver preflight correction are retained,
with the repeated staging errors retired.

Rust 1.85, stable and comparison CI passed for core source 5e8035a6495287226ed8c6cf634d37bc301b714f (run 39692602369 at evidence head 8e5f1441539eaa178e464522943a84633e6b26f9). This is portability/smoke evidence, not full-size packed-map admission.


## Packed selection and adaptive control widths

The corrected v25 panel completed all six exact proof matrices: 17,496 fresh
processes with complete tuple/binary/compiler/source/driver audits. Its broad
screens and fixed serial panels remain separate phases; serial acceptance is
not inferred from proof completion or parallel medians.

The v26 packed family passed 750 small scalar and 3,200 small generic checks,
independently audited, followed by 750 scalar and 3,200 generic parallel timing
runs with matching outputs. All 16 packed policies and nine fixed references
remain visible in the raw rows and explicitly labeled selection reports. These
runs overlapped required proof/compilation work. For the fixed
packed-owned-once-70 candidate, the scalar scan median ratio was 0.278 to
hashbrown, while read and miss100 ratios were 2.101 and 3.087. Its generic panel
had lower total medians in 23 cells and higher medians in 41; every read-only and
miss-only shape was higher. Seven scan shapes were lower and one higher.
These are noisy selection observations, not isolated speed or confidence claims.

Disassembly of a frozen scalar getter shows LLVM already forwards the matched
index word and dense row pointer. The suspected second control load is absent;
a successful-slot bounds check remains. No change was made to PackedIndexMap on
the unsupported reload hypothesis. A new CompactIndexMap instead tests adaptive
four/eight-byte controls against an always-wide control sharing its fused lookup
and dense storage. Thirty-two policies are registered. Controls use boxed slices
to keep the Table header at 48 bytes on 64-bit hosts. At 20,000 rows the 25%-fill
adaptive policy has already widened; it must not be reported as a narrow-word
measurement. Smaller metadata is an unproven latency hypothesis.

The new source passes 85 local tests and all-target comparison Clippy. Width
reconstruction hashes are counted exactly; a stored-key Hash panic preserves
current and retained narrow views, retry succeeds, and the map continues beyond
100,000 rows with a further wide growth. All builder/fill/batch/width variants
receive collision/model checks. No comparative timing, timed conversion or
full-size/threaded admission is established for CompactIndexMap yet. The default
Store, final holdout seeds and original across-the-board objective are unchanged.


## Complete corrected sharded serial panel

The v25 serial phases finished all 1,800 scalar and 7,296 generic processes with
matching outputs: respectively 20 and 19 fixed configurations, both development
seeds and three fresh repetitions. The recorded own build/proof/screen work and
final validation log writes precede these serial phases; v27 compilation starts
after the pipeline finishes. This interval audit does not certify an otherwise
idle host or turn descriptive medians into corrected confidence bounds.

No sharded configuration wins every cell. Fixed shard-shared-group-25-16 has a
scalar read ratio of 1.108 to hashbrown and higher generic read medians in all
eight shapes; its generic total medians are lower in eight cells and higher in
56. The larger shard-owned-group-50-1024 is lower in 25 generic cells and higher
in 39. The fixed array-owned-copy-low16 reference remains lower in 29 cells and
higher in 35. The stronger std-fast control is lower than hashbrown in 11/15
scalar and 35/64 generic cells, reinforcing the need for multiple controls.
Complete fixed-candidate tables retain every loss. These development counts do
not select a per-case engine or establish an overall winner. The original
objective remains unmet, and the holdout seeds remain unused.

## Compact metadata selection and native ownership panel

The frozen v27 panel completed 1,320 scalar and 5,632 generic small exact
state/revision/feed proofs. An independent tuple/source/binary audit verifies all
6,952 processes. Its parallel selection panel completed the same number of
matching-output timing processes. Full-size and four-reader probes are running
separately; no full-size admission follows from these small checks.

Compact adaptive four-byte metadata does not solve the lookup losses. The fixed
compact-adaptive-owned-once-70 candidate has lower medians than hashbrown in
3/15 scalar and 25/64 generic cells. All eight generic reads and all eight
all-miss cells remain higher. Its scalar scan ratio is 0.271, read ratio 1.949
and all-miss ratio 4.559. These overlapping-work selection observations are not
isolated speed results. The fixed-width contrasts and every loss remain in
compact-v27-selection-review.json; no per-workload winner is selected.

The v28/v29 panels register 25 native-backend snapshot ownership variants with
all matched Arc-wrapped controls. Identical backend, staging, revision and journal
logic compare direct backend cloning against an extra Arc envelope. Native roots
can reduce indirection but also increase snapshot acquisition cost. This is a
benchmark prototype, not a changed default Store. Two frozen builds separately
exercise PVec branch32 and branch4; neither branch configuration may be silently
unified with the other. Ten actual vector adapters include a common indexed COW
Vec control; four OrderMap adapters name shift/swap removal and builder policies.
OrderMap wraps IndexMap and is not counted as an independent indexing algorithm.

Before engine timing, the exact upstream PVec and OrderMap API proofs passed at
20,000 records, with full retained-payload checks, four reader threads, injected
Clone failures and complete tracked-value reclamation. PVec pop failures and both
branch sizes are exercised. These are API capability checks, not engine/feed
proofs or performance rankings. The vector adapter additionally passes a
collision-heavy randomized canonical-key/deletion/old-view model with both
branch sizes. The source/lock and selected API proof receipts are retained in
research/native-vector-map-proof. All-target comparison Clippy passes for both
branch builds. The 87 core/experimental/root tests pass. The original holdout
acceptance gate remains unused and unmet.

The v27 20,000-record scalar phase subsequently completed all 720 registered
processes with no failures. A fresh independent audit verifies these alongside
the 6,952 small checks (7,672 processes total), including exact tuple coverage and
frozen source/binary bindings. This does not cover the still-running generic
20,000-record phase or the queued concurrent-reader phases.

## Completed Compact contracts and ownership/upstream selection

The v27 small, full-size and actual-reader panels finish 11,608 exact proof
processes: 6,952 small, 720 full scalar, 3,072 full generic and 864 threaded.
The complete independent audit verifies all tuple sets and frozen bindings.
Registered serial development and the unchanged final holdout remain pending;
passing these contracts does not erase its lookup regressions.

The v28 ownership/upstream panel completes 10,902 small exact proof processes,
independently audited, and 10,902 matching-output parallel selection processes.
All 69 fixed engines remain visible. For scalar compact-adaptive-owned-once-70,
native/Arc read medians have ratio 0.703, but the native all-miss median is still
3.159 times hashbrown. Direct cloning of the embedded 1,024-entry directory
regresses scalar acquisition by about 2,086 times its Arc-envelope control.
The generic review preserves all 64 cells for each fixed policy. These noisy
selection medians overlap owned proof/compilation work and cannot establish
isolated superiority. Native ownership has not changed the default library.
Full-size and threaded v28 proofs are now running; branch4 v29 is a separate build.

The separate raw-growth diagnostic finishes 1,344 processes in 192 cases with
seven fresh repetitions; its independent audit recomputes exact tuples, bindings
and medians. At fill70, unpinned 65,536-to-131,072 widening has adaptive/wide raw
insert medians about 1.518 for u64-inline and 3.006 for string-owned256. All
contrasts, including those favoring adaptive controls, remain in the full review.
These are raw insertion costs, not complete-engine or service latency.

All 120 Compact/control instrumented retention cases pass exact journal/value
and final reclamation checks and an independent coverage audit. In the pinned
64-view fixed256 cell, each of six Compact policies retains 320.51 MiB of owned
payload and makes 5,191,200 value clones, matching the hashbrown copy adapter.
The paged reference retains 15.97 MiB and makes 71,200 clones; imbl-fast retains
67.51 MiB with 981,297 clones. RSS includes all fixture/instrumentation overhead.
Narrower controls do not resolve whole-table payload copying.

## Safe grouped-byte raw-map prototype

A separate minimal generic Rust prototype tests eight-byte integer filters with
8/16/32-slot groups and four fills. It retains full key equality, including the
word mask's possible false candidates, and stages reconstruction before moving
values. Nine selected frozen tests pass: all 12 policies receive randomized
collision/canonical-key/old-view checks and four actual readers over 20,000 rows.
Clone/hash/drop recovery, same-capacity tombstone cleanup and tracked-value
release also pass. The actual complete-engine/feed contract remains pending.

Its selected raw held-getter baseline runs all 336 fresh processes across 24
builder/group/fill policies and four actual std/hashbrown controls. The independent
unsigned model reproduces every checksum and median. This is parallel diagnostic
selection over two development seeds, not final confidence or engine performance.
Fixed group-fast-25-8 is lower in two of six cells: u64 read ratio 0.740 and u64
all-miss 0.757. Its String read ratio is 2.675. Fixed group-fast-70-8 loses all six
cells; fixed group-sip-50-16 is lower in four but has a u64 read ratio of 1.824.
All losses remain visible. No prototype is promoted and no per-case winner is
selected. The default Store and holdout seeds remain unchanged.

The separately compiled v29 branch4 panel also completes 10,902 audited small
proof processes and 10,902 matching-output parallel selections. The actual Cargo
compiler-artifact feature audit confirms arc in both builds and small_branch only
in v29, with identical committed root inputs. Compare each PVec policy to its
matching controls within the same binary; raw times across builds do not isolate
branch factor. Complete fixed-policy tables remain in the branch review. The v28
full-size scalar panel additionally passes all 1,650 selected exact checks and an
independent audit. Generic/threaded full-size phases for both branches are now
running. No serial or final performance acceptance follows.

## Adaptive flat/paged implementation

The new opt-in AdaptiveMap starts with std flat storage and can stage paged
immutable records after qualifying nonempty batches. A needs_drop gate is only a
generic heuristic about clone cost. One-way and half-density-reset controls keep
transition behavior explicit. The backend sees actual operations and type
properties, with no case names, warmup flag or lookahead. Shared-flat and paged
caches clone Arc handles rather than keys/values; get/traversal value-instance
identity is checked. A shared cache-disabled control isolates the caching effect.
Conversion still copies complete keys/values and its cost must remain visible.

The source passes 97 local core/experimental tests and all-target comparisons
Clippy. Ten selected frozen adaptive tests cover 144 raw policies against the
independent collision/canonical-key/retained-value model. They also check
conversion in both directions, payload/key/hasher failure recovery, failed partial
deltas, final tracked-value release, clone-free reads and three representative
flat modes with four actual readers over 20,000 records. Complete per-policy
engine/revision/feed/threaded proofs are separate and remain pending.

The registered v30 panel contains all 288 standard/fast builder configurations
and 12 fixed controls. Dedicated entry points reuse the original scalar and
generic traces, warmup, staging, revisions, ordered feed and borrowed-value oracle
harnesses. The source and registry are frozen before comparative execution.
Cold conversion, repeated transitions, interleaved updates/scans and derived-cache
retention diagnostics remain required. No adaptive timing or selected default
improvement has been established; original holdout seeds remain untouched.


## Completed adaptive scalar selection and transfer registration

The v30 scalar development screen completes all 9,000 fresh processes across
288 fixed adaptive configurations and twelve controls. Exact tuple coverage and
all output signatures were independently rechecked when generating
`research/adaptive-v30-scalar-selection-review.json`; the complete 4,500-cell
median report is retained under `evidence/state-v30-screen`. These parallel
measurements overlap other work and supply no isolated speed acceptance. No
fixed adaptive configuration has a lower total median than hashbrown in all
fifteen scalar cases; the largest count is nine. All fixed candidates' losses
and ratios remain visible. No per-case oracle or default switch is selected.

The correctness-only full-size diagnostic controller also completes both
156-process retention cohorts, with zero/two traversals per revision. The
independent audit verifies all 312 exact tuples, frozen inputs, journal counts,
cache handles and final payload reclamation. These unoptimized example timings
remain excluded from rankings. The separate optimized v30d diagnostic results
remain the applicable original copying-control measurements.

`research/v32t-diagnostics-plan.json` now registers a matched transfer panel:
38 fixed raw policies/controls across six key/payload shapes, two development
seeds and seven repetitions (3,192 full-size processes), preceded by 456 smoke
processes. Copying and Arc-preserving policies are compiled together. The
17-engine memory panel covers 408 full-size and 272 smaller ownership cases,
including cache creation, pinned roots, fixed/mixed payloads and retained windows.
The new modes pass diagnostic Clippy; comparative results remain pending. Clone
reduction alone does not establish a speed win or complete-engine admission.


## Shared-record transfer smoke and additional native admission

The new matched v32t examples complete all 456 cold and 272 retention smoke
processes. Independent audits verify all trace/checksum/representation/cache
states, exact grids, journal holdings and final payload release. In the fixed
1,000-record, 100-revision, ten-update, initial-pinned, window16 case, shared
copying controls make 3,000 Value clones; reuse controls make 2,000, with or
without two scans per revision. Page16 copying peaks at 972,800 tracked owned
payload bytes versus 839,680 for reuse. These are exact instrumented payload
observations, not total heap or performance acceptance. All policies remain
visible in the raw smoke evidence. The registered full-size v32t cohort is now
running, with no final confidence or engine result inferred.

The v33t complete-engine plan separately registers forty fixed policies across
five flat modes, two builders, two page sizes and two transition directions,
plus thirteen references. It keeps the original traces, staging, borrowed-value
oracles, revision/feed and four-reader checks. Small proof and screen phases
each comprise 8,374 processes across scalar and eight generic shapes. New
benchmark targets pass Clippy; their runtime proof and timing admission remain
pending. Original v30 binaries and plans are retained unchanged.

Both v28 branch32 and v29 branch4 now complete their selected full-size and
threaded matrices: 10,670 independently audited processes each. These are
behavioral proofs, not additional isolated performance comparisons.

The actual mvcc-core 0.1.0 source builds and passes three selected frozen native
capability tests, including 20,000-record retained full-value models, transaction
rollback/empty commit and four reader-owned native transactions during writes.
Compiler failures prove the native Transaction is neither Send nor Sync. The
previously proposed direct Arc/Mutex retained transaction adapter cannot satisfy
the transferable-view contract. Native scoped-reader and explicit exported-view
adapters remain pending, with distinct costs; the source is not counted as
beaten or excluded. The older optional-durability note was incorrect: this pinned
crate is in-memory only. Exact evidence and remaining requirements are linked in
`research/native-mvcc-admission-plan.md`. Thirty-four catalog package sources are
now locally verified against their published archives; that is not a count of
independent completed benchmark admissions.

The further selected frozen v2 native proof passes all four tests, including
20,000 distinct simultaneously live native transactions across four real reader
threads during sixteen writer commits. Every session checks sampled old values
before/after writes; first/last sessions per worker check full old contents.
Native active-transaction counters are exactly 20,000 before and after writing
and zero after all readers join. This is a native capacity/retention behavior
proof, not a timing, socket capacity or full payload-reclamation measurement.
