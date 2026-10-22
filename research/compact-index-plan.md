# Adaptive dense-index controls

Status: implemented for validation; performance unproven. No default selection.

The packed v26 parallel screen favors dense scans in many shapes but has large
read/miss regressions. Inspecting its compiled scalar getter disproved the
suspected extra index reload: LLVM forwards the cached word and row address.
It leaves an additional successful-slot bounds check. Do not describe a direct
source loop as eliminating a machine-code reload that is already absent.

CompactIndexMap compares 32 configurations: adaptive versus always-wide words,
fills 25/50/70/85, shared/inline builder storage and lazy/once-per-batch COW.
Both word policies share the same lookup/row-storage code. Controls are boxed
slices: on 64-bit hosts the tagged control header is 24 bytes and Table is 48,
matching the earlier pair of Vec headers. Rows remain dense complete K/V pairs.
No identity-key shortcut, unsafe access or per-workload engine choice is used.

Adaptive controls initially use u32 words with 16 cached hash bits and a 16-bit
row index plus one. Complete key equality is always checked. Before capacity
would exceed 65536, construct u64 controls by hashing every existing key once,
then install the finished replacement. A user Hash panic must preserve the
installed representation and every existing binding. Wide words cache 32 hash
bits and a 32-bit row index, supporting the same large limits as PackedIndexMap.
Further wide growth reuses cached hashes. Conversion hashing/allocation is real
work and must be measured, not counted as an ordinary no-rehash growth.

At 20,000 rows the 25%-fill adaptive configuration has already widened during
initial construction: its capacity is 131072. It must not be described as a
four-byte-word measurement. The 50%-fill configuration uses 65536 four-byte
words, while 70/85 use 32768. Always-wide controls use eight-byte words at those
same capacities. Dynamic format dispatch and changed fingerprint collisions are
part of the tradeoff; smaller metadata is not evidence of lower latency.

Validate all 32 small configurations, complete key equality under deliberately
colliding 16/32-bit caches, canonical/borrowed keys, swap removal, retained roots
and destructor failures. At the 70% transition, fail a stored key's Hash during
widening, check current and old roots, retry, then grow beyond 100,000 rows.
Count the exact reconstruction hashes. Retained views must keep their old word
width and complete contents. Raw batches remain nontransactional; atomic
revision/journal staging is still supplied externally.

Use the ordinary scalar/eight-shape shared-contract executables with matching
fast-hasher controls and earlier fixed layouts. Register every variation before
parallel proofs/screens. Require full-size and actual threaded admission, then
serial comparisons with real job-interval isolation. Add explicit conversion
timing around the width boundary: the 20,000-row steady-state benchmark alone
does not measure that one-time cost. Keep owned-payload retention separate:
this family still copies whole dense tables and does not fix that limitation.
All original holdout seeds and acceptance requirements remain unchanged.
