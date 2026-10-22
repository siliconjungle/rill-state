# Grouped byte-index prototype

Hypothesis: the existing single-slot probe chains contribute to lookup/miss
losses. Test safe integer operations on eight control bytes at once, with
8/16/32-slot groups and fill thresholds 25/50/70/85. Full keys always determine
identity. Packed controls filter candidates; subtraction can produce extra
fingerprint candidates, so every candidate receives full equality checking.

This prototype uses aligned groups, linear group probing, seven hash bits,
explicit empty/deleted controls, Option slots, owned builders and whole-table
COW. Those choices differ from hashbrown; none implies equivalent probing or
better performance. Tombstones do not terminate searches. Occupancy triggers
staged reconstruction, including same-capacity tombstone cleanup. User hashes
and clones can fail before movement starts; old snapshots must remain valid.

Register all 12 group/fill policies before comparisons. Required local proofs:
mask false negatives and empty/deleted behavior, full collisions, wraparound,
canonical/borrowed keys, delete/reinsert churn, complete randomized model and
old views, clone/hash failures, growth/cleanup and four actual reader threads at
20,000 records. Prove value reclamation as well. These are raw-map proofs; a
complete revision/feed adapter, parallel screens, serial timings and retention
checks remain required. No production/default change or performance claim.

Primary references: [Swiss table metadata design](https://abseil.io/about/design/swisstables),
[word byte detection](https://graphics.stanford.edu/~seander/bithacks.html#ValueInWord),
and the exact published hashbrown 0.17.1 control-group source (recorded hashes).
Use original safe Rust implementation; do not copy unsafe table machinery.

The core comparison binaries are building from frozen committed inputs.
Keep this experiment separate until those builds finish. No copied dependency
tree or alternate root build target is needed.
