# Additional actual upstream map admission

The initial catalog still contains applicable maps without executed adapters.
Add pinned chashmap 2.2.2 and crossbeam-skiplist 0.1.3 to a dedicated scalar
binary over the existing state/revision/ordered-feed harness. This is an
additional comparison, not a new Rill default or a performance result.

Both native lookup APIs return guarded values. The scalar harness already
copies the same original 32-byte value for every backend, so these adapters can
read native values directly without a shadow map, padded values or unsafe
lifetime extension. The generic shape harness requires an unguarded borrowed
reference, which cannot escape these native guards. Its admission remains open;
a scoped borrowed-read comparison must not be confused with this scalar panel.

Published native backends are immutable and independently owned. Each nonempty
revision stages a separate native backend, applies state changes and publishes
it through the same outer Arc/revision/journal wrapper as existing controls.
Keeping an old root does not hold a native read guard across publication. This
measures retained-version adaptation, not native concurrent-writer throughput.

CHashMap's native Clone copies its bucket layout and hash builder. Compare it
with length-based reconstruction using the native retain visitor. The crate has
no borrowed public iterator: compare retain-with-true traversal, including its
per-bucket write locks and a stack-local FnMut bridge, with native clone followed
by consuming iteration. No native capture/allocation cost is excluded from the
scan timer. Each copying/scanning combination uses either individual submitted
operations or a BTreeMap of final per-key operations. This gives eight fixed
CHashMap configurations. Coalescing allocation/sorting runs inside publication;
the external journal still preserves every original operation in exact order.

Crossbeam SkipMap reconstruction visits the immutable old map and inserts each
binding into a fresh native skip list. Its native Entry guard is scoped to each
lookup/traversal. Compare individual updates against the same final-operation
batching rule, yielding two fixed configurations. Neither native backend shares
subsequent mutations with a retained published root.

Before timings, register ten adapter configurations and twelve fixed controls
in v31admit-development-plan.json. Controls include standard/fast hash builders,
an ordered B-tree, persistent maps, paged records and four fixed adaptive
policies. Run all 660 small proofs, 660 full-size proofs, 88 actual four-reader
probes and 660 parallel development selections. The registered 1,980-process
serial development stage stays pending until this agent's other build/proof/
screen work finishes. No holdout seeds are used. Losses, unavailable capabilities
and missing comparisons leave the original objective incomplete.
