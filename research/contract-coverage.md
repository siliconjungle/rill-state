# Contract coverage and promotion requirements

This inventory clarifies existing evidence. It does not change the registered
primary workloads, final timing protocol or across-the-board acceptance gate.
A component proof or an adapted benchmark result is not automatically a proof
of every public Store API guarantee.

| Observable | Default public Store | Raw experimental maps | Current comparison adapters |
| --- | --- | --- | --- |
| Independently retained values | Native snapshots; conditional Send/Sync | Raw retained-root models; thread probes cover representative modes | Complete state/old-view checks per admitted tuple |
| Atomic publication after input/Clone failure | Staged commit; iterator and clone-panic checks | Caller must stage; raw apply may partially mutate | Staged wrapper or native transaction; generic error cases require their own proof |
| Exact ordered submitted operations | Native bounded journal, including duplicates and empty batches | No journal | Explicit external journal or Store wrapper; ordered-vector checks |
| Foreign/future/gap cursor errors | Domain-qualified Cursor with explicit error variants | No cursor API | Common harness accepts integer revisions; it cannot represent a foreign-domain cursor |
| Native transferable snapshot handle | Conditional Send/Sync | Conditional on actual backend/data traits | Normal run requires Send/Sync; native MVCC sessions use a separately labelled owner-thread path |
| Byte memory bound | No; history limits count batches/operations | No | Payload counters and whole-process RSS are reported separately |
| Live network capacity | Not implemented or tested | Not implemented or tested | 20,000 logical readers/records do not prove 20,000 sockets |

`Store::changes_since` validates store identity, future revision and journal gap
in `src/lib.rs`. `tests/contracts.rs` checks foreign cursors and retained journal
boundaries; the core unit suite checks the future case. The common scalar and
generic Engine traits in `benches/state.rs` and `benches/shapes.rs` instead take
`after: u64`. The Store adapter resolves that revision through its retained
cursor vector and invokes the public API. Other adapters maintain external
revision/journal metadata. These interface and adapter costs are real; label
them and do not describe an adapted result as a native map operation score.

Raw AdaptiveMap explicitly requires caller staging for atomic batches. Its
clone/hash/visitor failures and old-root checks establish the stated raw scope,
not a newly integrated Store backend. Before promoting any candidate to the
public Store, exercise its actual public commit/snapshot/cursor/feed surface
with input-iterator failure, key/value Clone and hash failures, empty/repeated/
absent-delete operations, retained roots, foreign/future/gap cursors and relevant
Drop behavior. Preserve the existing documented limits for interior mutability,
application-retained memory and destructors that fail after publication.

Native mvcc-core session handles remain on their creating threads. Their scalar
reads copy the original 32-byte value from a scoped guard; Rc acquisition is not
a transferable snapshot score. Exported views pay native scan/materialization
costs before commit. Generic guarded-value callbacks/exports and instrumented
native reclamation remain separate pending requirements. The independently
proved 20,000 simultaneous native transactions are logical sessions, not a
network benchmark or proof of 20,000 operating-system threads.

All successful development matrices retain their exact scope and bindings.
No library is counted as beaten because a capability is absent, an adapter is
pending, a timeout occurs, or another configuration wins a different cell.
