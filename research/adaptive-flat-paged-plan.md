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
