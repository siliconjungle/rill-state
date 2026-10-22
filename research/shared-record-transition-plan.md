# Shared-record layout transfer

The independently audited 20,000-record uncached retention fixture records
91,200 payload clones for shared-flat adaptive controls versus 71,200 for the
always-paged record controls. Inputs are 256 revisions of 200 updates, with
64 retained recent views, the initial view pinned and fixed 256-byte payloads.
Both end with 15.967 MiB of tracked owned payloads; their peaks are 20.850 and
16.064 MiB respectively. These are payload accounting results, not total heap
or speed measurements. Exact rows are in retention-v30proof20k-scans0.

The extra 20,000 clones come from rebuilding immutable records during layout
conversion. Add an explicit ReuseSharedFlat policy alongside the unchanged
SharedFlat copying control. Both flat and paged representations already own
Arc<SharedRecord> handles. Forward transfer can insert those handles directly
into a fresh paged index; reverse transfer can reuse each record and clone only
its flat lookup key. No cached checksum, pointer-identity lookup or data-type
specialization is introduced. Conversion still pays for index construction,
hashing, reference counts, allocations, key copies and cache invalidation.

Keep the same submitted-density, delay, needs_drop, page-size and direction
rules. The five flat modes (inline, shared, shared-cache, reuse, reuse-cache)
produce 240 raw policies. Preserve the original 144 controls. Validate the
additional 96 against the same full-key/canonical-metadata/collision/old-view
model, then check payload clone bombs, exact changed-value clone counts, value
instance identity across transitions, partial-delta failure, failed rehash and
reverse key-clone/hash failure. Actual four-reader probes at 20,000 records
must cover both added modes. All payloads must be reclaimed after final owners
drop. Transfer methods assume unique canonical source records; custom flat
policies remain responsible for their declared representation semantics.

Before any transfer-policy performance measurement, register a raw paired
diagnostic panel: old/new shared modes, cache off/after two scans, delays 1/3,
pages 16/64 and one-way/reset, using the fast builder, 1-percent threshold and
drop gate. These are 32 fixed policies plus hashbrown, std-fast, std-btree,
imbl-fast and record-paged16/64 controls (38 engines). Reuse the original cold
trace, six shapes and two development seeds, seven fresh repetitions per tuple:
3,192 processes at 20,000 records. First run the 456-process 1,000-record smoke
cohort. The existing copying controls must run in the same new frozen binary;
do not infer a speedup from cross-binary timing differences.

Extend the streaming memory panel with four transfer policies (cache off/on,
page 16/64) beside the original thirteen policies/controls. Preserve scans 0/2,
fixed/mixed payloads, initial pin off/on and retained windows 0/16/64: 408 full
20,000-record processes. Track live payload instances, owned lengths, clone
counts and per-revision cache handles; keep process RSS distinct from library
heap. No timing result or production-backend switch is justified by the
clone-counter improvement alone. Full revision/feed admission and the original
all-competitor isolated holdout gate remain required.
