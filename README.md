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


The opt-in `experimental` feature contains generic linear/split/shared-record COW-page map research
APIs. They are not the selected default backend. The comparison suite also exercises
String keys, owned and shared payloads, missing queries and concurrent retained-view
reads. No performance admission follows merely from passing a small shape test.
