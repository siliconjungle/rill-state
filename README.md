# Rill State

An experimental generic Rust library for versioned in-memory state, immutable snapshots and bounded incremental change feeds. No game schemas, storage engine, transport or runtime requirement. Durable persistence remains the caller's responsibility. The default library has no dependencies and forbids unsafe code.

**Research is in progress. The objective of beating every applicable competitor has not passed.** Early parallel development measurements show several existing alternatives winning individual workloads. No universal performance superiority or 20,000 live-connection claim has been established. See the [registered protocol](research/protocol.md), [source catalog](research/catalog.json) and [initial results](research/results.md).

```rust
use rill_state::{FeedError, Mutation::{Put, Delete}, Store};

let mut state = Store::<String, u64>::default();
state.commit([Put("temperature".into(), 21)]).unwrap();
let old = state.snapshot();

state.commit([
    Put("temperature".into(), 22),
    Put("pressure".into(), 1013),
]).unwrap();
assert_eq!(old.get(&"temperature".into()), Some(&21));
assert_eq!(state.snapshot().get(&"temperature".into()), Some(&22));

for batch in state.changes_since(old.cursor()).unwrap() {
    // Process exact ordered operations, or clone the batch Arc for asynchronous use.
    assert_eq!(batch.operations().len(), 2);
}
```

`Store` has one owner. Clone snapshots for concurrent readers; they are `Send + Sync`
when the key, value and hasher are. A short `RwLock` around the latest snapshot or an
external atomic-reference publisher can distribute complete revisions. The store itself
does not silently serialize multiple writers or implement optimistic transactions.

The current layout uses a two-level copy-on-write directory of small hash partitions.
Unmodified partitions are shared. Changes are staged before publication; keys and values
must implement `Clone`. Wrap expensive values in `Arc<T>` to share their allocations.
Interior mutability inside a key/value remains the caller's responsibility: mutating it
can violate snapshot semantics, as with other shared ownership collections.

`Config` bounds journal batches and submitted operations. Every commit, including an empty
one, advances the revision. A journal is a mutation feed, **not** a minimal state diff:
it includes repeated keys and deletes of nonexistent keys. `FeedError::Gap` explicitly
requires a fresh snapshot; cursor identities reject use with another store. Cursor values
are process-local and not a durable/serializable synchronization protocol.

Journal limits do not bound memory held by application-retained snapshots or batch Arcs.
One oversized batch can exceed the journal limit transiently and evicts itself after
publication. Limits are counts, not bytes. Slow-reader memory and reclamation are part of
the unfinished performance investigation.

The default hasher is the standard keyed `RandomState`. `with_hasher` accepts a caller
choice. `rillfast-*` benchmark variants use hashbrown's faster default builder and must
not be presented as having the same adversarial-key resistance as the default.

## Reproduce

```sh
cargo test --locked --no-default-features
cargo doc --locked --no-default-features --no-deps
cargo bench --locked --features comparisons --no-run
# Use the state executable path printed by Cargo:
python3 tools/study.py prove --binary target/release/deps/state-EXACT_HASH --label my-proof --workers 3
python3 tools/study.py screen --binary target/release/deps/state-EXACT_HASH --label my-screen --workers 3
```

Each study refuses to overwrite a label and records sources, executable hash, compiler,
candidate list, inputs, output checksums and process peak RSS. Parallel timings only guide
selection. Native concurrent-map copy adapters and native transactional stores are named
explicitly. Publication primitives have a separate benchmark and are not complete engines.

Use the same Rust toolchain for every dependency. Some dependencies carry their own
`rust-toolchain.toml`; with Rustup, set `RUSTUP_TOOLCHAIN=stable` (or an exact installed
version) for reproducibility. No nightly feature bypass is used in the admitted matrix.

MIT OR Apache-2.0 for this project's original code; comparison dependencies retain their
own licenses. This repository is a source release, not a crates.io publication.


The opt-in `experimental` feature contains generic linear/split/shared-record/compact-record COW-page map research
APIs. They are not the selected default backend. The comparison suite also exercises
String keys, owned and shared payloads, missing queries and concurrent retained-view
reads. No performance admission follows merely from passing a small shape test.

The shared-record experimental layouts retain keys and values behind independent
Arcs, so copying a page shares untouched String/Vec allocations. This helps some
heap-value publication workloads, but adds pointer reads and allocations; it does
not improve every shape. The default Store has not switched to this prototype.

Fixed-size `ArrayRecordMap` pages additionally compare copied/shared canonical
keys and shared/inline hash-builder storage. Eighty-five contract/oracle tests and
Clippy pass. Full-size state/feed proofs and retained-reader probes pass for the
registered array configurations, followed by 11,082 matching-output serial
development runs. These layouts still lose important workloads to hashbrown.
The instrumented retention study releases every tracked payload after its final
owner is dropped. Payload accounting is not total heap usage or a speed result.

`InlineSliceMap` is a further experimental layout: one contiguous COW slice with
inline keys/values, a nonzero cached-hash niche and no per-record allocation.
Sixteen configurations compare fill thresholds, read probing and builder storage.
Its core behavior tests pass, as do 2,528 small and 2,528 full-size scalar/generic
checks and 512 four-reader probes. Its serial development sweep completed 10,902
processes with matching outputs. A fixed layout improves scalar read medians
but loses 59 of 64 generic cases to hashbrown; no overall winner is selected.
Streaming probes across four representative configurations reclaim
all payloads, but retain much more owned payload data than the paged layouts.

`ShardedInlineMap` embeds a fixed table directory and copies only touched inline
tables on writes. Eighty configurations compare directory sizes, maximum fills,
builder storage and per-operation/grouped batching. Grouping includes hashing,
stable counting/scattering and scratch allocation costs. Its core checks pass;
the original sweep passed 27,840 small/full/threaded proof processes. A corrected
executable with the stronger oracle is undergoing fresh proofs and comparison.
No speed improvement is claimed yet.
Interrupted clones, growth and destructors now preserve usable mutable maps;
staging is still required for atomic batches. The proof harness checks complete
borrowed payloads, unique iteration and every model key without cloning values.

`PackedIndexMap` is another opt-in experiment: complete K/V rows in dense storage,
with cached 32-bit hashes and row indices packed into 8-byte probing words.
Sixteen configurations vary fill, builder storage and lazy/once-per-batch COW.
Complete keys still determine equality. Hash truncation, explicit 32-bit index
limits and whole-table copying are documented tradeoffs. Collision, canonical-key,
swap-removal, retention and panic-recovery checks pass. Comparative performance
and full-size contracts for this new family remain unproven.

`CompactIndexMap` tests adaptive four/eight-byte controls against an always-wide
control using the same dense-row implementation. Thirty-two configurations vary
width policy, fill, builder storage and batching. Widening stages reconstruction
hashes before changing the installed index; recovery tests retain old roots and
continue past 100,000 rows. Local validation passes, but this family has no
comparative performance or full-size/threaded admission yet. Its plan includes
explicit conversion timing and keeps whole-table payload retention visible.

Default study lists omit the known investigative `scc-copy` native Clone chain,
whose capacity doubles with each copy in pinned scc 3.8.8. It is not counted as a
performance win or silently removed from the admission queue. A bounded native
reproduction is `cargo run --locked --release --features comparisons --example
scc_probe -- --capacity-only`; `--by-len` checks the corrected allocation strategy.
The explicit `scc-capture` comparison uses live record count and remains subject
to full contract proof. Study deadlines apply to an invocation's own process group
so an expired time wrapper cannot leave an unobserved candidate running.

Compact `RecordMap` experiments put key and value in one immutable allocation,
while preserving canonical stored-key metadata on replacement. The generic
comparison now includes mixed String lengths and mixed owned/shared payload
lengths. These extensions do not select a new default or establish superiority.

For instrumented ownership/reclamation evidence, `examples/retention_profile.rs`
streams batches without retaining future payload inputs, checks old snapshot
contents and releases the pinned root, recent views and store in stages. Its
counters cover owned payload lengths and Value instances, not total heap usage
or speed; `tools/retention_study.py` records 0/16/64-view windows and verifies exact
journal holdings. Run with `--features comparisons`; source/binary receipts and
all full-size cells are retained under `evidence/retention-v19-20k`.
