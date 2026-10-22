# Traversal ordering after shared-record transfer

The complete matched v32t grid retains all sixteen fixed copying/transfer pairs
across six shapes, sixteen trace steps and fourteen seed/repetitions per cell.
Every tuple, full executable value check and independent checksum audit passes.
For fixed delay3/page16/reset and u64-owned256, the forward conversion batch
median is 8.6661 ms for copying versus 5.1890 ms for reuse. Its uncached third
scan is 0.2270 versus 0.3345 ms. With the existing after-two-scans cache, forward
batch medians are 9.2801 versus 4.9408 ms, but the third scan is 0.1308 versus
0.2313 ms. String-owned256 shows a similar tradeoff. These parallel development
medians overlap other jobs; they are not isolated confidence or a capacity claim.
Exact data are in `research/shared-transfer-v32t-review.json`.

A plausible next hypothesis is physical traversal locality. Copying creates new
record allocations; transfer retains existing record addresses. The measurements
do not establish locality as the cause: allocator placement, code generation and
concurrent load can also affect the observations. Test the hypothesis explicitly,
keeping both copying and transfer controls and recording construction costs.

Before implementing or timing, register this focused family:

- Keep ordinary after-two-scans handle caches as the baseline.
- Compare record-address order and coarse address-bucket order, using generic
  immutable Arc record handles. Retain both copying and transfer variants for
  each ordering. Any shift/bucket policy is fixed before timing.
- Ordering changes traversal only, which already has unspecified order. It must
  never replace full key equality, cache a result/checksum, use a query object's
  address for lookup, clone keys/values on reads, or change value identity.
- Charge collection and sorting to the first cache-building scan. Include first,
  second and later scans, update interleavings, repeated layout conversion,
  threaded cache initialization and retained-cache reclamation.
- Validate full borrowed values, canonical keys, duplicate/collision behavior,
  old snapshots, no read-time K/V clones, cache invalidation and final release
  before comparative timing. Preserve all current unsorted controls.
- Use fresh complete tuples and matched binaries over all current shapes and
  development seeds. Keep hashbrown/std/BTree/persistent/paged references. Full
  revision/feed admission, broader source admission, isolated serial comparisons
  and the untouched final holdout gate remain required.

No new ordering implementation or timing result exists yet. Do not label this a
proven fix, infer general heap locality from Arc pointer values, or select an
ordering per benchmark case. A fixed policy must pay its own sorting overhead
and survive cells where it loses.
