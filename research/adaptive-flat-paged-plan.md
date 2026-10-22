# Adaptive flat/paged experiment proposal

Measured motivation: standard/hashbrown flat controls still win many lookup
workloads; the paged record reference retains about 16 MiB of owned payloads
where whole-table copies retain about 320.5 MiB in the same pinned fixture.
Directly cloning embedded directories can make snapshot acquisition dramatically
worse. One fixed representation has not resolved those tradeoffs.

Prototype a generic data-driven flat/paged backend, separate from the frozen
binaries. Flat storage uses std HashMap, and paged storage uses the existing
RecordMap. A snapshot retains one outer Arc for either representation; it never
copies the page directory when acquired. Conversion clones complete canonical
keys/values into staging and installs the new root only after success. No
Nocturne/game schemas, case labels, pointer identity shortcuts or trace lookahead.

Register 48 policies before timings: small-batch limits 1/10/100 percent,
consecutive nonempty batches 1/3, an always/needs_drop gate, page sizes 16/64, and
dense traversal caching off/after two complete traversal requests. Empty batches
advance revisions/feed normally and do not force a conversion. The needs_drop
rule is a generic heuristic, not a guarantee about clone cost. Initial flat reads
and large-update behavior must remain visible; conversion cost is included.

Optional dense traversal caching is an algorithm experiment, not checksum or
result memoization: every traversal still visits all complete keys/values. Cache
construction and cloned payload memory must be measured, clone failures must
leave logical state unchanged, and ordinary one-off traversals must not eagerly
materialize it. Borrowed-value/identity expectations and interior mutability need
explicit review before this option could become public/default. Keep a cache-off
control. Include interleaved writes/scans and cold first-use diagnostics rather
than relying solely on static repeat scans.

All 48 policies require complete model, canonical/borrowed-key, collision,
conversion/clone/hash/drop recovery, exact revision/feed and real reader-thread
proofs. Retention must count derived cache payloads as well as primary storage.
Use both native and matching-fast builders and keep all stronger controls. Freeze
one fixed policy before any final holdout; never select a backend using case names
or choose an oracle winner per workload. This proposal does not change the default
Store, establish admission, or replace any pending registered comparison.

Before timings, refine the cache design to preserve borrowed value identity and
perform no K/V clones on reads. Cache only Arc<SharedRecord> handles, in both
shared-flat and paged roots. Shared-flat storage duplicates lookup keys and adds
record allocation/indirection costs; it must not be confused with direct flat
values. Add 24 matching shared-flat cache-disabled policies. The initial 48 plus
these 24 controls make 72 fixed policies, exercised with both standard and fast
builders. Conversion traverses primary storage and never triggers a scan cache.
A cached root is invalidated before mutation; cloning for staging starts with an
empty derived cache. Conversion is one-way. All conversion, cache construction,
cache invalidation and ownership costs remain visible.

Retain the irreversible policy as an explicit control and add an optional reset
for submitted batches at least ceil(current_len/2). This is a fixed density rule
for all inputs, including duplicate-heavy batches where it may predict poorly.
The unchanged harness has small-update warmup followed by a bulk restore; a
one-way policy remains paged afterwards. Reset behavior responds to those actual
operations, with no warmup flag or case-name access. Include both policies and
require cold/repeated-transition diagnostics. The final registered panel has
144 layout/transition configurations, each with standard/fast builders (288),
plus 12 fixed controls. It supersedes the provisional one-way-only matrix before
any adaptive timing. See v30-development-plan.json for the exact names.

Before any cold diagnostic timing, register v30-diagnostics-plan.json. Its raw
mixed-density panel compares 24 adaptive policies (three flat modes, delay 1/3,
page 16/64, one-way/reset; fixed fast builder, 1-percent threshold and drop gate)
with six controls, including std BTreeMap. Each process begins with an initial
root and runs three cycles of four 1-percent batches and one unique half-map
batch. It retains the initial and previous roots, validates all their complete
values outside timing, and times three complete traversals after every batch.
This exposes first-use cache construction and repeated conversion. It does not
flush CPU caches or include releasing the previous reader in the batch timer.
The registered full panel is 2,520 fresh processes, seven repetitions per tuple,
at 20,000 records across six key/value shapes and two development seeds. A
separate Python oracle rebuilds the random trace and checksums. Parallel raw
timings do not replace complete-engine or isolated performance admission.

The same registration selects six adaptive memory policies and seven controls
for 312 streaming retention processes, with scans disabled or two traversals per
revision. It counts live payload instances, owned payload lengths and cached
shared-record handles by distinct retained revision. Every traversal must clone
no payload values; dropping all roots, journal batches and caches must reclaim
every tracked payload. Cache handle counts are not total heap bytes. The 1,000-
record smoke cohorts run first; their exact matrices are part of the plan.
