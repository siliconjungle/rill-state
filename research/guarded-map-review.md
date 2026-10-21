# Guarded-map admission review

Pinned primary sources were read directly from the checksum-audited registry cache.
File hashes are in guarded-map-review.json; no dependency tree was copied.
This review resolves native capability questions, not benchmark admission or speed.

## flashmap 0.2.0

[ReadHandle/ReadGuard source](https://docs.rs/crate/flashmap/0.2.0/source/src/read.rs)
implements Send and Sync conditionally for both handles and guards. An earlier
assumption that its read handle required a mutex was incorrect. Guards hold a
specific map index and therefore provide a consistent view. However,
[WriteHandle::guard](https://docs.rs/crate/flashmap/0.2.0/source/src/write.rs) calls
synchronize before returning a write view, and that synchronization can wait for
old readers. Retaining a native read guard indefinitely is therefore not an
adapter for arbitrarily retained revisions with continuing publication.

A full-contract comparison can create an independent native backend per new
revision, copying its old bindings before applying the complete ordered batch
under one native write guard. Published backends then remain immutable. The
write handle is not declared Sync; a safe adapter may put it behind a Mutex but
use get_mut for its exclusively owned mutation path. Reads should use native
read guards and report their acquisition cost. A copy into an unrelated std map
must not be presented as flashmap native-read throughput. This adapter remains
to be implemented and proved; no speed result or exclusion is inferred.

## evmap 11.0.0 and left-right 0.11.8

[evmap ReadHandle::enter](https://docs.rs/crate/evmap/11.0.0/source/src/read.rs)
explicitly documents that publication cannot proceed while a guarded reference
is retained. [left-right WriteHandle::publish](https://docs.rs/crate/left-right/0.11.8/source/src/write.rs)
waits for old readers before replaying the operation log into their former copy.
It also offers try_publish, which can report that it could not publish; that
changes progress behavior rather than supplying another retained revision.

A comparison must either capture independently owned immutable state or retain
a fresh native backend per published revision. Copy/capture and journal costs
belong inside publication timing. Native short-lived-reader throughput remains
a separate useful contract. Read-handle Clone creates another reader of current
state, not an independently retained immutable revision. Native guarded APIs
therefore cannot silently replace the registered snapshot contract. Implemented
and measured capture adapters are still required before any broad win claim.

## Native observation and implemented comparison

The frozen native guarded-retention example confirmed the same trace in all
three pinned backends: one publication completed while an old guard was held,
the next attempt waited during the 250 ms observation window, and publications
two and three completed after release. The retained value remained zero. This
is not a throughput result and is consistent with the primary-source wait paths.
Exact source/executable/compiler bindings and full traces are retained under
evidence/guarded-native-v18.

Independent-backend adapters are now implemented in benches/state.rs: native
flashmap/evmap map copies and a left-right operational state containing a
hashbrown table initialized once and cloned to both sides. Native keys and
32-byte inline values are unchanged. Evmap and left-right read handles use
Mutex for the registered Sync view requirement; exclusive writes use get_mut.
Each adapter passed 90 small checks, 90 20,000-record checks collectively
(15 cases × two seeds × three configurations), and 12 four-reader old-view
probes. These are contract-preserving adaptations; native guard acquisition
and copying costs must remain explicit in subsequent measurements.
