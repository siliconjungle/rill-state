# Results and open work

The original objective (generic public Rust library outperforming every applicable
competitor across the registered workloads) is **not achieved**. This document records
progress, not acceptance. The public implementation is experimental.

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
