# Performance measurements: 0.4.269

Three changes follow the hot-path measurement, allocation reuse, and throughput
guidelines in `rust-performance.md`:

1. Typed minimization keeps each measure in the retained row buffer and compacts
   it there. This removes the separate measure allocation and the copy from the
   measure buffer into typed rows. Dense measures also avoid measure-buffer growth.
2. Phantom-hold correction stores one `u16` validity mask per row, replacing
   per-lane `usize` tail indices. Four-lane tracking falls from 32 to 2 bytes per
   row; eight-lane tracking falls from 64 to 2. Typed minimization supplies its
   existing rows to correction, eliminating a second row allocation and parse.
   The shared matcher preserves blockers, nested heads, and its eight-head limit.
3. Matrix aggregation counts consecutive measures in a stack array, merging into
   the BPM map once per active BPM run. It avoids hashing every stream measure
   and preserves duplicate-beat, skipped-segment, and invalid-BPM behavior.

## Component benchmarks

Baseline: `c9a370b`, version 0.4.268. Optimized: 0.4.269. Windows MSVC,
Rust 1.98.1, Intel Xeon E5-2696 v4, system allocator, repository bench profile
(fat LTO, one codegen unit, debug symbols). Both versions use identical benchmark
code and byte-identical fixtures. The measurement thread is pinned to logical
CPU 2; compilation and input construction occur outside measurement.

Each process reports the median of seven batches after four warmup calls. These
results are medians across three old/new process pairs with alternating order,
2,000 calls per batch. Allocation counting runs separately from timing and cycle
measurement. Cycles come from Windows `QueryThreadCycleTime`.

| Case | CPU cycles, old → new | Allocations / reallocations, old → new | Requested bytes, old → new | Throughput gain |
| --- | ---: | ---: | ---: | ---: |
| Four-lane taps, reused | 192,648 → 175,180 | 3 / 0 → 3 / 0 | 46,185 → 46,185 | 9.9% |
| Four-lane taps, first use | 194,136 → 179,312 | 5 / 0 → 4 / 0 | 63,233 → 62,977 | 8.3% |
| Four-lane dense taps, first use | 183,084 → 164,875 | 5 / 2 → 4 / 0 | 63,329 → 61,537 | 11.1% |
| Eight-lane dense taps, first use | 205,065 → 190,356 | 5 / 2 → 4 / 0 | 97,869 → 94,285 | 7.8% |
| Four-lane phantom heads, reused | 375,546 → 285,429 | 5 / 0 → 4 / 0 | 194,049 → 54,377 | 31.6% |
| Eight-lane phantom heads, reused | 463,040 → 343,754 | 5 / 0 → 4 / 0 | 357,661 → 70,493 | 34.7% |
| Matrix, long BPM segments | 284,372 → 136,534 | 2 / 1 → 2 / 1 | 5,200 → 5,200 | 108.3% |
| Matrix, short BPM segments | 286,751 → 137,258 | 2 / 1 → 2 / 1 | 5,104 → 5,104 | 108.8% |

Row cases contain 4,096 rows in 16-row measures, or 256-row dense measures.
Phantom cases insert unmatched heads every 16 rows. Matrix cases contain 16,384
measures and 32 timing segments with repeated BPMs. Timing includes construction
and destruction of returned results. Allocation counts include owned outputs;
warmed scratch storage is excluded. Requested
bytes sum successful allocation and reallocation requests, including the full
requested resize size; they measure churn rather than peak live memory.

Full Camellia fixture analysis was also measured in three alternating pairs,
200 calls per batch for component cases and 20 for full analysis. Its median
time fell from 201.46 to 187.03 ms; fast analysis fell from 27.02 to 24.42 ms.
Whole-fixture timings varied substantially and their ranges overlapped, so those
gains are indicative. Full analysis retains 110 allocations and 5,263,624
requested bytes with warmed scratch. Owned chart results still require storage.

## Reproduction

```powershell
cargo bench -p rssp --bench hotpath_perf

$env:RSSP_HOT_ITERS = '2000'
$env:RSSP_HOT_FILTER = 'typed' # Or 'matrix' or 'analyze'.
cargo bench -p rssp --bench hotpath_perf
```

For the old implementation, use a separate checkout of `c9a370b`, copy
`crates/rssp/benches/hotpath_perf.rs`, add its `harness = false` bench entry to
`crates/rssp/Cargo.toml`, and copy the working checkout's benchmark fixtures
byte-for-byte. Build both executables first, then run them alternately without
concurrent compilation. Remove `RSSP_HOT_FILTER` to run every case.

## Behavior validation

Focused tests assert exact minimized bytes, typed rows, beat positions, counts,
cross-measure note tails, phantom nesting/blockers, the hold-stack limit, and
Matrix profile entries. Existing timing and generated-profile tests cover the
composed production paths. All 189 workspace library/binary unit tests pass in
release mode. Clippy passes for both libraries and the new benchmark with
`-D warnings`. The required command
`cargo test --release --test all_parity -- --test-threads=22` exited with status
101, retaining the exact same ten failures as the untouched baseline. Its final
two lines were verified exactly:

```text
test result: FAILED. 30479 passed; 10 failed
error: test failed, to rerun pass `-p rssp --test all_parity`
```

# Performance measurements: 0.4.270

Baseline: `2e02886`, version 0.4.269. This pass applies `M-HOTPATH`,
`M-INITIAL-CAPACITY`, `M-MEM-REUSE`, and `M-THROUGHPUT` from
`rust-performance.md` to timing statistics:

1. Timing hold tracking stores a `u16` head mask, `u8` tail count, and judgability
   flag in one four-byte record per row. This replaces per-lane `usize` tail
   indices and a separate tail-count allocation. Tracking storage falls from
   36 to 4 bytes per row for four lanes, and from 68 to 4 for eight lanes.
   Layouts wider than 16 lanes retain the previous representation. Fake heads
   never become active; tails remain active through their own row. Rare repair
   of invalid holds is marked cold to keep it outside the ordinary state machine.
2. `compute_no_hold_stats` visits borrowed rows from minimized text and counts
   them directly. Analysis uses it when minimization has confirmed there are no
   valid hold or roll heads. This avoids row materialization and hold tracking,
   including on charts with fake intervals. Owned parsing and direct counting
   share row validation through the same visitor.
3. Raw timing minimization reduces each measure in the retained row buffer.
   It removes the separate measure buffer, copying rows into the output, and
   growth of that measure buffer on dense charts. The result still owns its
   rows and beats at this parsing boundary.

## Measurements

Hardware, allocator, compiler, and affinity match the 0.4.269 report above.
Both executables were built before measurement. Each result below is the median
of three process pairs with alternating order, seven batches of 1,000 calls per
process, and four warmup calls. Allocator counting runs separately. Inputs are
identical, deterministic, and constructed outside measurement; fixture bytes
were also checked against the baseline checkout. CPU cycles use Windows
`QueryThreadCycleTime`; throughput uses elapsed time.

| Case | CPU cycles, old -> new | Allocations / reallocations, old -> new | Requested bytes, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| Four lanes, holds with fake intervals, typed rows | 232,221 -> 233,549 | 2 / 0 -> 1 / 0 | 147,456 -> 16,384 | -0.8% |
| Eight lanes, holds with fake intervals, typed rows | 299,795 -> 269,774 | 2 / 0 -> 1 / 0 | 278,528 -> 16,384 | +11.3% |
| Four lanes, no holds, text with fake intervals | 281,121 -> 209,385 | 3 / 0 -> 0 / 0 | 164,248 -> 0 | +34.4% |
| Eight lanes, no holds, text with fake intervals | 351,456 -> 228,440 | 3 / 0 -> 0 / 0 | 311,744 -> 0 | +55.0% |
| Four lanes, dense raw holds | 396,710 -> 308,864 | 5 / 2 -> 3 / 0 | 182,064 -> 49,200 | +28.4% |
| Eight lanes, dense raw holds | 478,786 -> 373,457 | 5 / 2 -> 3 / 0 | 331,300 -> 65,572 | +28.4% |
| Four lanes, sparse raw taps | 299,127 -> 319,173 | 3 / 0 -> 2 / 0 | 33,840 -> 33,584 | -6.3% |
| Complete fast analysis, fake intervals and lifts | 645,560 -> 603,234 | 34 / 0 -> 31 / 0 | 223,276 -> 59,028 | +6.9% |

Timing cases contain 4,096 rows, with two nested heads and tails every 16 rows
for hold cases. Phantom cases insert a blocker before the tails. Measures have
16 rows normally and 256 in dense raw cases. Tap cases include periodic lifts.
The composed case analyzes a complete SSC containing those taps and lifts,
four fake intervals, and the regular analysis outputs, with technique and
pattern counting disabled. Its requested bytes fall by 73.6%.

For the `no_hold_text` comparison, the baseline calls the existing general
text timing API used by its analysis caller; the optimized executable calls
the new API used by its updated caller. Other benchmark calls are unchanged.
Requested bytes measure allocation churn, including full requested resize
sizes, rather than peak live memory or process RSS.

CPU gains depend on the case. Four-lane hold tracking primarily benefits
allocation count and memory. Small raw tap and general text cases have mixed
timing medians; sample ranges overlap. For example, sparse raw four-lane taps
range from 134.71 to 140.37 us before and 132.97 to 154.73 us after. General
four-lane hold text has 9.3% lower median throughput with overlapping ranges, while
the composed fast analysis improves by 6.9%. These results support the storage
reductions and the targeted throughput gains without implying every small
call is faster. The six existing direct minimization cases also retain their
allocation counts and requested bytes; their throughput medians improve by
0.7-5.7% in this comparison.

## Reproduction and behavior

```powershell
$env:RSSP_HOT_ITERS = '1000'
$env:RSSP_HOT_FILTER = 'timing' # Or 'fast_fake_lifts' or 'direct'.
cargo bench -p rssp --bench hotpath_perf
```

For the baseline, use `2e02886`, copy the current benchmark and its fixtures
byte-for-byte, and replace its single `rssp::stats::compute_no_hold_stats`
benchmark call with `rssp::stats::compute_timing_aware_stats_with_row_to_beat`.
Build both executables, then run three pairs in alternating order without
concurrent compilation. The baseline already registers `hotpath_perf`.

New tests assert explicit statistics for nested and fake heads, tail inclusion,
blockers, the eight-head stack limit, decreasing beats, empty measures,
comments, terminators, CRLF and short rows, every four/eight-lane tap mask,
special cells, 16/17-lane layouts, and fast/full analysis across all four
supported lane counts. Validation uses production functions directly.

All 198 workspace library/binary unit tests pass in release mode. Strict Clippy
passes for both libraries and `hotpath_perf`, and formatting/diff checks pass.
After confirming the optimizations, the required command
`cargo test --release --test all_parity -- --test-threads=22` exited with status
101. The ten failure names match the 0.4.269 baseline exactly, and its final two
lines were verified exactly before committing:

```text
test result: FAILED. 30479 passed; 10 failed
error: test failed, to rerun pass `-p rssp --test all_parity`
```

# Performance measurements: 0.4.271

Baseline: `ca14356`, version 0.4.270. This pass removes work before changing
the remaining arithmetic, following `M-HOTPATH`, `M-MEM-REUSE`,
`M-AVOID-INDIRECTION`, and `M-THROUGHPUT` in `rust-performance.md`:

1. Peak NPS stops cleaning, parsing, storing, and building runtime tables for
   speeds, scrolls, and fakes. These cannot affect elapsed measure time. Raw
   chart tags still participate in timing ownership, including empty auxiliary
   tags that suppress inherited song timing. Stops, delays, warps, offsets,
   and SSC version behavior are preserved.
2. BPM snapshots borrow clean timing tags with the existing `Cow` cleaning
   functions. Dirty tags retain the owned fallback. Two field-identical tag
   structs, their conversion/overlay helpers, a macro, and a forwarding function
   are removed; fixed arrays hold the seven timing sources directly.
3. Fixed-BPM tier calculation finds the maximum density within eligible runs
   using integers, then performs the original floating-point multiplication and
   division once. Category changes and the four-measure minimum are preserved.

Production code shrinks by 81 lines. Public APIs and returned fields are unchanged.

## Measurements

Windows MSVC, Rust 1.98.1, Intel Xeon E5-2696 v4, system allocator, repository
bench profile (fat LTO, one codegen unit, debug symbols). The original and updated
implementations use identical benchmark source, fixture bytes, and dependency
versions. Both executables were built before measurement, with no concurrent
compilation or corpus scans. The measuring thread is pinned to logical CPU 2.

Each process reports medians of seven batches after four warmup calls. Values
below are medians of three process pairs with alternating old/new order, 1,000
calls per batch (100 for peak NPS on Camellia). Input construction and I/O are
outside measurement; destruction of returned outputs is included. CPU cycles
come from `QueryThreadCycleTime`. Allocation counting runs separately.

| Case | CPU cycles, old -> new | Allocations / reallocations, old -> new | Requested bytes, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| Peak NPS, plain | 25,317 -> 25,586 | 15 / 0 -> 15 / 0 | 3,287 -> 3,287 | -1.0% |
| Peak NPS, global auxiliary timing | 176,735 -> 25,486 | 27 / 1 -> 15 / 0 | 23,183 -> 3,287 | +593.9% |
| Peak NPS, local auxiliary timing | 650,192 -> 53,197 | 96 / 4 -> 56 / 0 | 83,679 -> 4,095 | +1122.0% |
| Peak NPS, Camellia fixture | 11,430,923 -> 11,231,758 | 26 / 2 -> 26 / 2 | 256,595 -> 213,459 | +1.8% |
| BPM snapshots, plain | 17,231 -> 17,177 | 27 / 2 -> 26 / 2 | 3,559 -> 3,554 | +0.5% |
| BPM snapshots, global auxiliary timing | 166,657 -> 160,789 | 36 / 3 -> 32 / 3 | 24,168 -> 20,794 | +3.7% |
| BPM snapshots, local auxiliary timing | 642,739 -> 611,054 | 108 / 14 -> 79 / 14 | 86,947 -> 73,346 | +5.2% |
| BPM snapshots, Camellia fixture | 269,402 -> 228,431 | 41 / 0 -> 37 / 0 | 4,587 -> 4,511 | +18.0% |
| Fixed-BPM tier, stream | 130,265 -> 91,063 | 0 / 0 -> 0 / 0 | 0 -> 0 | +43.1% |
| Fixed-BPM tier, mixed densities | 127,897 -> 92,468 | 0 / 0 -> 0 / 0 | 0 -> 0 | +37.9% |

The timing batches have four charts, each with 32 measures of four rows. Auxiliary
cases contain 128 entries each for speeds, scrolls, and fakes, globally or on
each chart. Local cases also contain BPM changes, a stop, a delay, and a warp.
Tier cases contain 16,384 measures at 137.125 BPM. Requested bytes sum successful
allocation and full reallocation requests; they measure churn, not peak live
memory. Peak NPS local timing reduces this churn by 95.1%.

Small timing differences should not be treated as universal speedups. Plain
peak-NPS samples overlap (old 11.50-12.63 us, new 11.58-11.76 us), with unchanged
allocation counts and a 1% median difference. Snapshot measurements include
transient outliers; Camellia's new process medians range from 104.26 to 165.64 us.
The allocation reductions are deterministic, while tier throughput improves in
both measured cases. No behavioral regressions were found.

## Reproduction and behavior

```powershell
$env:RSSP_HOT_ITERS = '1000'
$env:RSSP_HOT_FILTER = 'peak/' # Or 'snapshot/' or 'tier/'.
cargo bench -p rssp --bench hotpath_perf

# Emit every component output, with numeric fields represented by exact bits.
$env:RSSP_HOT_VERIFY = '1'
cargo bench -p rssp --bench hotpath_perf
Remove-Item Env:RSSP_HOT_VERIFY
```

For comparison, check out `ca14356` separately, copy the current
`crates/rssp/benches/hotpath_perf.rs`, and use the same fixture corpus and
dependency lockfile (adjust only the three workspace package versions to
0.4.270). Build both executables first and run three alternating pairs. The
baseline already registers this benchmark. Verify mode emits the relative file
path, every peak-NPS field, every BPM-snapshot field, and errors in sorted file
order. Compare the output files byte-for-byte.

The original and updated component outputs match exactly across 30,489 valid
simfiles and 56,125 charts. The 354 invalid-input errors also match. The UTF-16
PowerShell output has SHA-256
`32b29a5626b1696dcac85509e48d1a7f4a93162ccc0303c6fd971c9c823f252e`.
Five new integration tests cover auxiliary-only timing overrides, old SSC
versions, time-affecting segments, dirty/empty tag fallback, display BPM, run
boundaries, extreme densities, subnormal BPM, and invalid BPM. Tests call
production APIs directly; the variable-timing tier path supplies an independent
bit-exact comparison for the fixed-timing path.

All 198 existing workspace library/binary unit tests and all five new integration
tests pass in release mode. Strict Clippy passes for both libraries, the benchmark,
and the new tests. Formatting and diff checks pass. The original implementation
passes all 30,489 cases in `all_parity`. After confirming the optimizations, the
required final command also passed all 30,489 cases with zero failures before
committing:

```powershell
cargo test --release --test all_parity -- --test-threads=22
```

```text
test result: ok. 30489 passed; 0 failed
```

# Performance measurements: 0.4.272

Baseline: `9561f82`, version 0.4.271. This pass applies `M-HOTPATH`,
`M-MEM-REUSE`, `M-AVOID-INDIRECTION`, and `M-THROUGHPUT` from
`rust-performance.md` by deleting work that does not contribute to outputs:

1. Measure density counts tap/hold/roll rows with a branchless lane scan.
   Removing all-zero rows cannot change that count, so the flag buffer,
   reduction scan, recount, scratch struct, and forwarding fill function are removed. Peak NPS
   reuses a plain density vector across charts.
2. Duration extraction checks its existing raw timing key before cleaning chart
   maps. Cache hits avoid all four clean operations. Its key and data now share
   one optional entry, removing separate validity checks and two `expect` calls.
   The redundant forwarding wrapper is also removed. The cache retains its
   call-local, single-entry lifetime and performs the same builds on misses.
3. BPM snapshots use the shared BPM/stop parsing step directly, preserving
   legacy SM conversion and native f32 output precision. They stop cleaning and
   constructing delays, warps, speeds, scrolls, and fakes, including default
   tables and inherited timing rebuilt for auxiliary-only local tags. General
   timing construction reuses that same parsing step.

Production code shrinks by 113 lines. Public APIs and returned fields are unchanged.

## Measurements

Windows MSVC, Rust 1.98.1, Intel Xeon E5-2696 v4, system allocator, repository
bench profile (fat LTO, one codegen unit, debug symbols). Both versions use
identical benchmark code, fixture bytes, and dependency versions. The baseline
executable was saved before production edits; its source matches `9561f82`.
The updated executable was rebuilt after the final changes. Both were built
before measurement, with no concurrent compilation or corpus scans.

The measuring thread is pinned to logical CPU 2. Results below are medians of
three process pairs with alternating old/new order. Each process takes the
median of seven batches after four warmup calls, with 1,000 calls per batch
(100 for Camellia duration and peak NPS). Setup and I/O are outside measurement;
returned result destruction is included. CPU cycles use `QueryThreadCycleTime`.
Allocation counting runs separately from timing.

| Case | CPU cycles, old -> new | Allocations / reallocations, old -> new | Requested bytes, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| Density, 4 lanes, sparse | 94,043 -> 69,841 | 2 / 2 -> 1 / 0 | 8,656 -> 8,208 | +35.2% |
| Density, 4 lanes, dense | 98,044 -> 76,506 | 2 / 0 -> 1 / 0 | 8,464 -> 8,400 | +27.8% |
| Density, 5 lanes, sparse | 105,388 -> 61,839 | 2 / 2 -> 1 / 0 | 8,656 -> 8,208 | +69.8% |
| Density, 5 lanes, dense | 107,222 -> 69,272 | 2 / 0 -> 1 / 0 | 8,432 -> 8,368 | +55.9% |
| Density, 8 lanes, sparse | 148,375 -> 74,205 | 2 / 2 -> 1 / 0 | 8,648 -> 8,200 | +101.6% |
| Density, 8 lanes, dense | 145,762 -> 79,077 | 2 / 0 -> 1 / 0 | 8,376 -> 8,312 | +84.9% |
| Density, 10 lanes, sparse | 179,686 -> 77,786 | 2 / 2 -> 1 / 0 | 8,648 -> 8,200 | +129.5% |
| Density, 10 lanes, dense | 176,622 -> 80,979 | 2 / 0 -> 1 / 0 | 8,352 -> 8,288 | +118.8% |
| Duration, clean repeated timing | 340,868 -> 203,606 | 21 / 1 -> 21 / 1 | 25,815 -> 25,815 | +67.8% |
| Duration, dirty repeated timing | 494,790 -> 244,077 | 37 / 1 -> 25 / 1 | 51,463 -> 32,227 | +111.7% |
| Duration, dirty distinct timing | 944,930 -> 845,917 | 70 / 4 -> 70 / 4 | 120,223 -> 120,223 | +19.3% |
| Duration, Camellia fixture | 17,278,928 -> 16,802,339 | 23 / 5 -> 23 / 5 | 9,395 -> 9,395 | +3.9% |
| BPM snapshots, plain | 18,111 -> 16,882 | 26 / 2 -> 26 / 2 | 3,554 -> 3,554 | +7.7% |
| BPM snapshots, global auxiliary timing | 170,466 -> 17,128 | 32 / 3 -> 26 / 2 | 20,794 -> 3,554 | +886.7% |
| BPM snapshots, local auxiliary timing | 646,995 -> 38,074 | 79 / 14 -> 35 / 10 | 73,346 -> 4,162 | +1605.3% |
| BPM snapshots, Camellia fixture | 242,878 -> 238,601 | 37 / 0 -> 37 / 0 | 4,511 -> 4,511 | +1.3% |
| Peak NPS, plain | 25,434 -> 23,592 | 15 / 0 -> 14 / 0 | 3,287 -> 3,223 | +5.5% |
| Peak NPS, global auxiliary timing | 25,301 -> 23,455 | 15 / 0 -> 14 / 0 | 3,287 -> 3,223 | +8.4% |
| Peak NPS, local auxiliary timing | 53,208 -> 51,776 | 56 / 0 -> 55 / 0 | 4,095 -> 4,031 | +1.1% |
| Peak NPS, Camellia fixture | 11,492,967 -> 8,813,575 | 26 / 2 -> 25 / 0 | 213,459 -> 213,011 | +30.7% |

Density inputs contain 4,096 rows in four, five, eight, or ten lanes. Sparse
inputs have a tap every 16 rows and 256-row measures; dense inputs have one tap
per row and 16-row measures. Duration inputs have four charts with 128 entries
each for BPMs, stops, delays, and warps. Dirty maps contain whitespace and control
characters; distinct timing changes chart offsets to force cache misses.
Snapshot/peak timing inputs have four charts with 32 four-row measures. Auxiliary
cases contain 128 entries each for speeds, scrolls, and fakes; local cases also
have a BPM change, stop, delay, and warp. Requested bytes sum allocation and full
reallocation requests, measuring churn rather than peak live memory.

All measured medians improve, but small gains should be treated as indicative.
Peak-NPS local samples overlap (old 23.18-25.34 us, new 23.07-26.06 us), so
its 1.1% median gain does not establish a universal speedup. The 1.3% Camellia
snapshot gain is also small. Targeted allocation reductions are deterministic:
density cases halve allocation count and eliminate sparse-measure reallocations;
repeated dirty duration timing uses 12 fewer allocations and 37.4% less churn;
local snapshots use 44 fewer allocations and 94.3% less churn. Mixed-chart peak
NPS improves with nonoverlapping process medians (old 5.27-5.41 ms, new
4.07-4.52 ms). No behavioral regressions were found.

## Reproduction and behavior

```powershell
$env:RSSP_HOT_ITERS = '1000'
$env:RSSP_HOT_FILTER = 'density' # Or 'duration/', 'snapshot/', or 'peak/'.
cargo bench -p rssp --bench hotpath_perf

$env:RSSP_HOT_VERIFY = '1'
cargo bench -p rssp --bench hotpath_perf
Remove-Item Env:RSSP_HOT_VERIFY
```

For the baseline, check out `9561f82` separately, copy the current benchmark,
use the same fixtures and dependency lockfile (adjust only the three workspace
package versions to 0.4.271), and build first. Run both executables alternately
without other test or compilation workloads. The baseline already registers
this benchmark. Verify mode emits sorted file paths, explicit density vectors,
all duration/peak/snapshot fields, and errors; numeric fields use exact bits.

Original and final verification outputs are byte-identical across 30,489 valid
simfiles and 56,125 charts, with the same 354 invalid-input errors for each API.
The native UTF-8 verification output SHA-256 is
`bc0d8b3f57bff93ed791de3631dc6fb31130bdebbb09ea47cbb21adf71b57ce7`.
Three new integration tests cover reducible/odd/dense measures, all supported
lane counts, jumps, hold/roll heads, ignored note types, comments, CRLF,
termination, trailing/empty measures, dirty duration cache hits, offset/BPM
changes, auxiliary-only snapshot tags, and old/new SSC timing behavior.

All 198 existing workspace library/binary unit tests and all eight integration
tests pass in release mode. Strict Clippy, formatting, and diff checks pass.
After confirming the final benchmarks, the required command passed all 30,489
cases with zero failures before committing:

```powershell
cargo test --release --test all_parity -- --test-threads=22
```

```text
test result: ok. 30489 passed; 0 failed
```

# 0.4.273: duration scans, streamed SN formatting, and direct stream totals

## Changes

1. Duration scanning reads note rows directly. It removes the 96-row stack
   buffer, spill vector, reduction, compaction, and the forwarding helper.
   All-zero rows leave hold state unchanged, and reduction scales row indices
   and row counts by powers of two, preserving the exact beat calculation.
   Nested holds, blocking notes, and matched tails retain their original rules.
2. SN breakdowns consume an iterator of category runs instead of constructing
   a token vector. Single-output formatting reuses the existing pending-run
   logic, removing indexed merging and its duplicate implementation. A small
   reservation, capped at 160 bytes per output before actual text growth,
   avoids repeated allocations for short charts. The combined analysis path
   keeps its existing caller-owned token storage and exact capacity estimate.
3. Standard total-stream formatting counts eligible measures directly. It
   avoids segment construction, the intermediate formatting buffer, and the
   second output string. Nonempty totals require one 26-byte allocation;
   no-stream totals allocate only the existing `No Streams!` label. Detailed,
   partial, simple, and three-output standard formatting retain their original
   implementation.

These changes remove 92 net production lines. Workspace versions increase once
from 0.4.272 to 0.4.273. Public signatures and output text remain unchanged.

## Benchmark method

Original production code comes from `0aa0bcd` (0.4.272). Final comparisons compile
both original and optimized production code with the same workspace version,
0.4.273, in the same checkout and target directory. This holds crate metadata
constant: preliminary comparisons across versions also showed timing changes
in unchanged control functions. The benchmark, fixtures, dependency lockfile,
compiler, and build settings are identical. Baseline production changes are
absent when its executable is built; final production code is restored before
the optimized build and all final validation.

Measurements use rustc 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc, on an Intel
Xeon E5-2696 v4 (22 cores / 44 logical processors). The workspace bench profile
uses fat LTO and one codegen unit. Both executables pin their measuring thread
to logical CPU 2. Each process warms up, takes the median of seven batches, and
measures allocator requests separately. Tables report medians from three
alternating baseline/optimized process pairs. Component cases use 500 calls per
batch; Camellia duration uses 50. Analysis cases use 30 calls, with three calls
per batch for Camellia. Setup and fixture generation occur outside timed loops.
Cycles are Windows `QueryThreadCycleTime` values. Requested bytes sum allocation
and full reallocation requests, measuring churn rather than peak live memory.
Other activity on this shared machine can affect elapsed time and CPU counters;
allocation counts and requested byte reductions are deterministic.

## Duration results

Synthetic inputs contain 4,096 rows in four, five, eight, or ten lanes, including
hold heads, tails, mines, and taps. Sparse inputs place an event every 16 rows in
256-row measures; dense inputs place one per row in 16-row measures; odd inputs
place one every seven rows in 129-row measures. Existing timing-cache cases and
the five-chart Camellia fixture measure the composed public duration API.

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| duration_rows4/sparse | 79,354 -> 70,139 | 10 / 2 -> 9 / 0 | 3,864 -> 1,176 | +13.2% |
| duration_rows4/dense | 109,001 -> 92,498 | 9 / 0 -> 9 / 0 | 1,176 -> 1,176 | +16.9% |
| duration_rows4/odd | 102,666 -> 67,029 | 10 / 1 -> 9 / 0 | 2,328 -> 1,176 | +53.7% |
| duration_rows5/sparse | 85,193 -> 73,902 | 10 / 2 -> 9 / 0 | 4,535 -> 1,175 | +15.7% |
| duration_rows5/dense | 128,006 -> 103,874 | 9 / 0 -> 9 / 0 | 1,175 -> 1,175 | +23.0% |
| duration_rows5/odd | 114,502 -> 71,961 | 10 / 1 -> 9 / 0 | 2,615 -> 1,175 | +61.0% |
| duration_rows8/sparse | 89,170 -> 81,327 | 10 / 2 -> 9 / 0 | 6,552 -> 1,176 | +9.6% |
| duration_rows8/dense | 145,323 -> 130,620 | 9 / 0 -> 9 / 0 | 1,176 -> 1,176 | +11.0% |
| duration_rows8/odd | 141,448 -> 76,299 | 10 / 1 -> 9 / 0 | 3,480 -> 1,176 | +86.0% |
| duration_rows10/sparse | 96,420 -> 88,466 | 10 / 2 -> 9 / 0 | 7,895 -> 1,175 | +9.0% |
| duration_rows10/dense | 165,619 -> 139,716 | 9 / 0 -> 9 / 0 | 1,175 -> 1,175 | +18.2% |
| duration_rows10/odd | 161,262 -> 85,296 | 10 / 1 -> 9 / 0 | 4,055 -> 1,175 | +87.9% |
| duration/clean_hit | 202,429 -> 207,042 | 21 / 1 -> 21 / 1 | 25,815 -> 25,815 | -3.3% |
| duration/dirty_hit | 290,611 -> 231,305 | 25 / 1 -> 25 / 1 | 32,227 -> 32,227 | +30.3% |
| duration/dirty_miss | 966,877 -> 912,818 | 70 / 4 -> 70 / 4 | 120,223 -> 120,223 | +17.3% |
| duration/camellia | 18,386,540 -> 13,983,853 | 23 / 5 -> 18 / 0 | 9,395 -> 3,635 | +46.2% |

All twelve row-scan medians improve. Spill allocations disappear regardless of
measure length; sparse ten-lane requested bytes fall from 7,895 to 1,175.
Camellia uses five fewer allocations and 61.3% less requested allocation memory.
The tiny clean-timing cache case is dominated by unchanged timing/metadata
work; its small timing difference is discussed with the controls below.

## SN breakdown results

Uniform and no-stream inputs contain 4,096 measures. Fragmented inputs repeat
`[0,16,16,0,20,20,0,0,32,32,32,0,0,0,24]` over 4,096 measures. Short inputs repeat
`[16,0,20,0,0,24,32]` over 32 measures. Each single mode and the three-output
API is measured directly.

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| sn/uniform_detailed | 13,531 -> 11,619 | 2 / 0 -> 1 / 0 | 16,389 -> 160 | +15.7% |
| sn/uniform_partial | 13,876 -> 11,606 | 2 / 0 -> 1 / 0 | 16,389 -> 160 | +19.3% |
| sn/uniform_simple | 14,355 -> 12,022 | 2 / 0 -> 1 / 0 | 16,389 -> 160 | +17.9% |
| sn/uniform_three | 15,384 -> 12,343 | 4 / 0 -> 3 / 0 | 16,399 -> 480 | +28.8% |
| sn/fragmented_detailed | 154,830 -> 87,882 | 2 / 2 -> 1 / 6 | 125,603 -> 20,320 | +74.3% |
| sn/fragmented_partial | 88,214 -> 79,724 | 2 / 2 -> 1 / 5 | 125,603 -> 10,080 | +10.6% |
| sn/fragmented_simple | 90,616 -> 75,417 | 2 / 2 -> 1 / 5 | 125,603 -> 10,080 | +22.4% |
| sn/fragmented_three | 180,393 -> 170,772 | 4 / 2 -> 3 / 16 | 147,433 -> 40,480 | +8.2% |
| sn/empty_detailed | 5,002 -> 5,013 | 0 / 0 -> 0 / 0 | 0 -> 0 | -0.1% |
| sn/empty_partial | 5,013 -> 5,087 | 0 / 0 -> 0 / 0 | 0 -> 0 | -2.4% |
| sn/empty_simple | 4,932 -> 5,002 | 0 / 0 -> 0 / 0 | 0 -> 0 | +0.5% |
| sn/empty_three | 4,906 -> 5,023 | 0 / 0 -> 0 / 0 | 0 -> 0 | -2.6% |
| sn/short_detailed | 1,962 -> 1,176 | 2 / 0 -> 1 / 0 | 631 -> 155 | +66.5% |
| sn/short_partial | 1,341 -> 1,116 | 2 / 0 -> 1 / 0 | 631 -> 155 | +20.2% |
| sn/short_simple | 1,319 -> 1,108 | 2 / 0 -> 1 / 0 | 631 -> 155 | +18.9% |
| sn/short_three | 3,013 -> 2,679 | 4 / 0 -> 3 / 0 | 901 -> 465 | +12.6% |

Single outputs use one fewer allocation; three outputs also use one fewer.
Fragmented detailed churn falls 83.8%, fragmented partial/simple churn falls
92.0%, and uniform three-output churn falls 97.1%. Returned SN strings can
retain up to 160 initially reserved bytes each; the combined analysis path's
output reservation is unchanged. No-stream SN behavior performs no allocations.

## Standard total results

The same measure fixtures exercise the public total formatter. Total is the
count of stream measures, excluding gaps; an all-break input preserves the
`No Streams!` output.

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| standard/uniform_total | 7,423 -> 6,230 | 3 / 1 -> 1 / 0 | 24,606 -> 26 | +24.2% |
| standard/fragmented_total | 32,133 -> 6,490 | 3 / 2 -> 1 / 0 | 83,580 -> 26 | +405.1% |
| standard/empty_total | 7,837 -> 6,117 | 2 / 0 -> 1 / 0 | 24,587 -> 11 | +30.5% |
| standard/short_total | 790 -> 235 | 3 / 0 -> 1 / 0 | 500 -> 26 | +232.1% |

Fragmented totals eliminate two allocations and two reallocations, reducing
requested bytes from 83,580 to 26. Their median throughput improves about 5x.

## Composed paths and controls

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| streams/uniform_combined | 23,866 -> 23,936 | 6 / 0 -> 6 / 0 | 33 -> 33 | +0.8% |
| streams/fragmented_combined | 230,481 -> 235,269 | 6 / 0 -> 6 / 0 | 72,039 -> 72,039 | -1.5% |
| streams/empty_combined | 9,179 -> 9,161 | 3 / 0 -> 3 / 0 | 33 -> 33 | -2.1% |
| streams/short_combined | 3,833 -> 3,814 | 6 / 0 -> 6 / 0 | 891 -> 891 | +0.6% |
| analyze/fast_fake_lifts | 641,306 -> 654,366 | 31 / 4 -> 31 / 4 | 59,028 -> 59,028 | -3.6% |
| analyze/camellia | 470,967,106 -> 449,241,285 | 110 / 0 -> 110 / 0 | 5,263,624 -> 5,263,624 | +3.4% |
| analyze/fast_camellia | 60,683,629 -> 57,482,071 | 115 / 0 -> 115 / 0 | 7,051,152 -> 7,051,152 | +5.1% |
| analyze/mixed_small | 52,600 -> 49,702 | 59 / 3 -> 59 / 3 | 7,460 -> 7,460 | +7.3% |

Allocation counts and requested bytes in combined stream outputs and full
analysis are unchanged. Unmodified standard output paths generally remain
within a few percent of their original medians. Small timing changes, including
clean duration timing, empty SN output, and fast fake/lift analysis, do not
establish universal improvements or regressions on this shared machine.
They are included as controls rather than claimed speedups. Targeted row-scan,
nonempty SN, and total-format allocation reductions are reproducible.

## Reproduction and behavior

```powershell
$env:RSSP_HOT_ITERS = '500'
$env:RSSP_HOT_FILTER = 'duration_rows' # Or 'duration/', 'sn/', 'standard/', 'streams/'.
cargo bench -p rssp --bench hotpath_perf

$env:RSSP_HOT_VERIFY = '1'
cargo bench -p rssp --bench hotpath_perf
Remove-Item Env:RSSP_HOT_VERIFY
```

For a controlled baseline, save the final core `stats.rs` and `streams.rs`,
replace their production code with `0aa0bcd` versions, and keep the current
benchmark, fixtures, dependency lockfile, workspace version 0.4.273, checkout,
and target directory. Build and save the baseline executable, restore the final
sources, then build and save the optimized executable. Run the two executables
alternately without other RSSP build/test workloads. Baseline code under
`cfg(test)` has no effect on the bench executable. Use 30 iterations for the
analysis filter to reproduce its smaller batch size.

Verification emits sorted file paths, explicit density vectors, SN and standard
breakdown strings, total strings, all duration/peak/snapshot fields, and errors.
Numeric fields use exact bits. Original and final outputs are byte-identical
across 30,489 valid simfiles and 56,125 charts, with the same 354 invalid-input
errors for each analysis API. The native UTF-8 verification SHA-256 is
`10766ede60ad22c0827cb425977e638af8cefbf883cd640b71ace80624ea8735`.

An extended production-function test compares duration beats with the full
minimizer across all supported lane counts, odd/reducible/dense measures,
96/97-row spill boundaries, nested holds, blocking notes, ignored types,
comments, CRLF, empty measures, EOF, and termination. A new integration test
checks identical/different run categories around gaps of 0, 1, 2, 4, 5, 31, 32,
33, and 128 measures, plus explicit expected strings and total counts.

All 198 workspace library unit tests and all nine integration tests pass in
release mode. Strict Clippy, formatting, and diff checks pass. After confirming
the final benchmarks, the required command passes all 30,489 cases with zero
failures before committing:

```powershell
cargo test --release --test all_parity -- --test-threads=22
```

```text
test result: ok. 30489 passed; 0 failed
```


# 0.4.274: retain normalization work, scan spacing, and stack small NPS medians

## Changes

1. Dirty timing pair maps retain their already-normalized prefix. On the first
   dirty entry, the prefix is cleaned once and the remaining entries are
   cleaned and normalized in one pass. This removes the discarded string,
   repeated number parsing and formatting, the speculative pair-map helper, and
   analysis forwarding helpers. Clean maps retain their borrowed raw text.
   Pair-map control-character rules remain unchanged. Speed-map cleanup keeps
   its original implementation: sharing the cleanup loop caused a measurable
   regression on large clean speed maps, so that candidate was discarded.
2. Measure spacing scans raw rows directly, eliminating the measure vector,
   its growth, row copies, reduction, compaction, and second scan. Reduction
   retains every nonzero row. The surviving row count follows from the
   power-of-two alignment of the measure length and nonzero row positions;
   comparing it with the number of note rows gives the original spacing flag.
   Once a retained row proves spacing false, remaining row inspection stops
   until the next measure boundary. At stride one, alignment bookkeeping
   stops and only note checks remain. Empty measures, odd row counts, comments,
   terminators, unsupported lane counts, mines, tails, and unknown bytes retain
   their original behavior.
3. The allocating NPS statistics API selects small medians in a fixed array
   of 64 doubles instead of creating a vector. One/two-value special cases
   and existing scan shortcuts remain. The caller-owned scratch API retains
   its original reuse path. Larger medians use the existing heap storage.

Workspace versions increase exactly once from 0.4.273 to 0.4.274. Public
signatures, output text, and float operations remain unchanged.

## Benchmark method

Baseline production code is `bd55b4c`. Both executables use workspace version
0.4.274, identical measured benchmark code, fixtures, dependencies and compiler,
and the same checkout and target directory. The baseline executable was saved
before production edits. Holding the version constant avoids crate-metadata
changes in this comparison.

Measurements use rustc 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc, an Intel
Xeon E5-2696 v4 (22 cores / 44 logical processors), fat LTO and one codegen unit.
The measuring thread is pinned to logical CPU 2. Each process takes the median
of seven batches after warmup; tables report medians from three alternating
baseline/optimized process pairs. Component cases use 500 calls per batch;
NPS statistics use 5,000. Analysis uses 30, with three calls per batch for
Camellia. Setup, fixture generation and I/O occur outside timed loops. Timing
and allocation counting run separately. Cycles come from Windows
QueryThreadCycleTime; requested bytes sum allocation and full reallocation
requests, measuring churn rather than peak memory. No rssp builds or tests
run during final measurements. Other shared-machine activity can affect CPU
and elapsed-time results; allocation reductions are deterministic.

## Timing-map normalization

Clean, early-dirty and late-dirty inputs exercise pair maps and unchanged speed
maps at three sizes. Prefix reuse targets late-dirty pairs, while all dirty
pair maps avoid the discarded initial allocation. Clean input and speed-map
allocation counts remain unchanged.

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| cleanup/pair_1_clean | 825 -> 696 | 1 / 1 -> 1 / 1 | 27 -> 27 | +18.5% |
| cleanup/speed_1_clean | 1,073 -> 912 | 1 / 1 -> 1 / 1 | 36 -> 36 | +19.7% |
| cleanup/pair_1_early | 1,010 -> 792 | 3 / 0 -> 2 / 0 | 39 -> 26 | +27.3% |
| cleanup/speed_1_early | 1,391 -> 1,483 | 3 / 1 -> 3 / 1 | 80 -> 80 | -6.2% |
| cleanup/pair_1_late | 986 -> 790 | 3 / 0 -> 2 / 0 | 39 -> 26 | +24.9% |
| cleanup/speed_1_late | 1,316 -> 1,341 | 3 / 1 -> 3 / 1 | 80 -> 80 | -2.0% |
| cleanup/pair_128_clean | 48,435 -> 45,897 | 1 / 1 -> 1 / 1 | 4,521 -> 4,521 | +5.5% |
| cleanup/speed_128_clean | 75,466 -> 72,314 | 1 / 1 -> 1 / 1 | 5,673 -> 5,673 | +4.2% |
| cleanup/pair_128_early | 55,948 -> 52,783 | 3 / 1 -> 2 / 1 | 7,555 -> 6,044 | +5.9% |
| cleanup/speed_128_early | 83,551 -> 83,682 | 3 / 1 -> 3 / 1 | 9,475 -> 9,475 | -0.1% |
| cleanup/pair_128_late | 107,701 -> 61,363 | 3 / 2 -> 2 / 1 | 10,577 -> 6,044 | +75.5% |
| cleanup/speed_128_late | 154,160 -> 152,209 | 3 / 2 -> 3 / 2 | 13,265 -> 13,265 | +1.3% |
| cleanup/pair_4096_clean | 1,657,404 -> 1,577,723 | 1 / 1 -> 1 / 1 | 163,695 -> 163,695 | +5.1% |
| cleanup/speed_4096_clean | 2,337,740 -> 2,400,354 | 1 / 1 -> 1 / 1 | 200,559 -> 200,559 | -2.7% |
| cleanup/pair_4096_early | 1,778,771 -> 1,869,876 | 3 / 1 -> 2 / 1 | 272,845 -> 218,276 | -4.9% |
| cleanup/speed_4096_early | 2,607,276 -> 2,652,944 | 3 / 1 -> 3 / 1 | 334,285 -> 334,285 | -1.8% |
| cleanup/pair_4096_late | 3,440,356 -> 2,078,007 | 3 / 2 -> 2 / 1 | 381,983 -> 218,276 | +65.5% |
| cleanup/speed_4096_late | 5,042,807 -> 5,294,836 | 3 / 2 -> 3 / 2 | 467,999 -> 467,999 | -5.2% |

## Measure spacing

Each input has 4,096 rows. Sparse measures have 256 rows with one note every
16 rows; dense measures have 16 rows with a note on every row; odd measures
have 129 rows and a note every seven rows. Only the output flag vector remains.

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| spacing/4_sparse | 80,428 -> 73,697 | 2 / 2 -> 1 / 0 | 1,809 -> 17 | +9.1% |
| spacing/4_dense | 111,836 -> 90,983 | 2 / 0 -> 1 / 0 | 513 -> 257 | +22.8% |
| spacing/4_odd | 70,226 -> 71,180 | 2 / 2 -> 1 / 0 | 1,824 -> 32 | -1.3% |
| spacing/5_sparse | 85,897 -> 84,529 | 2 / 2 -> 1 / 0 | 2,257 -> 17 | +1.6% |
| spacing/5_dense | 113,530 -> 99,354 | 2 / 0 -> 1 / 0 | 577 -> 257 | +14.3% |
| spacing/5_odd | 74,285 -> 70,896 | 2 / 2 -> 1 / 0 | 2,272 -> 32 | +4.9% |
| spacing/8_sparse | 87,826 -> 93,702 | 2 / 2 -> 1 / 0 | 3,601 -> 17 | -6.3% |
| spacing/8_dense | 136,681 -> 126,429 | 2 / 0 -> 1 / 0 | 769 -> 257 | +8.1% |
| spacing/8_odd | 82,197 -> 82,969 | 2 / 2 -> 1 / 0 | 3,616 -> 32 | -0.9% |
| spacing/10_sparse | 96,634 -> 101,122 | 2 / 2 -> 1 / 0 | 4,497 -> 17 | -4.4% |
| spacing/10_dense | 143,444 -> 141,819 | 2 / 0 -> 1 / 0 | 897 -> 257 | +1.2% |
| spacing/10_odd | 86,587 -> 87,221 | 2 / 2 -> 1 / 0 | 4,512 -> 32 | -0.8% |

Spacing CPU results depend on the input. The ten-lane sparse case uses 4.6%
more cycles while requested allocation bytes fall from 4,497 to 17 (99.6%).
The table includes this tradeoff; the change does not claim a universal CPU
speedup. The measure buffer and its reallocations are eliminated in every case.


## NPS statistics

Cold cases call get_nps_stats; warm cases call get_nps_stats_with_scratch after
warmup. The latter is an allocation-free control and preserves its original
storage strategy. Mixed deterministic values avoid the constant-value median
shortcut. Sizes around 64 verify the boundary and larger controls.

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| nps_stats/1_cold | 30 -> 16 | 0 / 0 -> 0 / 0 | 0 -> 0 | +75.0% |
| nps_stats/1_warm | 22 -> 19 | 0 / 0 -> 0 / 0 | 0 -> 0 | +11.1% |
| nps_stats/2_cold | 41 -> 21 | 0 / 0 -> 0 / 0 | 0 -> 0 | +90.0% |
| nps_stats/2_warm | 31 -> 30 | 0 / 0 -> 0 / 0 | 0 -> 0 | +0.0% |
| nps_stats/3_cold | 243 -> 63 | 1 / 0 -> 0 / 0 | 32 -> 0 | +282.8% |
| nps_stats/3_warm | 42 -> 50 | 0 / 0 -> 0 / 0 | 0 -> 0 | -17.4% |
| nps_stats/8_cold | 273 -> 123 | 1 / 0 -> 0 / 0 | 64 -> 0 | +119.3% |
| nps_stats/8_warm | 108 -> 110 | 0 / 0 -> 0 / 0 | 0 -> 0 | -2.0% |
| nps_stats/16_cold | 435 -> 233 | 1 / 0 -> 0 / 0 | 128 -> 0 | +86.0% |
| nps_stats/16_warm | 211 -> 203 | 0 / 0 -> 0 / 0 | 0 -> 0 | +3.2% |
| nps_stats/32_cold | 1,162 -> 444 | 1 / 0 -> 0 / 0 | 256 -> 0 | +165.5% |
| nps_stats/32_warm | 452 -> 431 | 0 / 0 -> 0 / 0 | 0 -> 0 | +3.5% |
| nps_stats/63_cold | 992 -> 770 | 1 / 0 -> 0 / 0 | 504 -> 0 | +29.1% |
| nps_stats/63_warm | 734 -> 730 | 0 / 0 -> 0 / 0 | 0 -> 0 | +0.6% |
| nps_stats/64_cold | 1,652 -> 819 | 1 / 0 -> 0 / 0 | 512 -> 0 | +101.9% |
| nps_stats/64_warm | 860 -> 824 | 0 / 0 -> 0 / 0 | 0 -> 0 | +4.5% |
| nps_stats/65_cold | 1,019 -> 1,028 | 1 / 0 -> 1 / 0 | 520 -> 520 | -0.9% |
| nps_stats/65_warm | 797 -> 768 | 0 / 0 -> 0 / 0 | 0 -> 0 | +4.0% |
| nps_stats/256_cold | 2,983 -> 2,954 | 1 / 0 -> 1 / 0 | 2,048 -> 2,048 | +0.9% |
| nps_stats/256_warm | 2,665 -> 2,542 | 0 / 0 -> 0 / 0 | 0 -> 0 | +4.5% |
| nps_stats/4096_cold | 48,848 -> 44,455 | 1 / 0 -> 1 / 0 | 32,768 -> 32,768 | +10.2% |
| nps_stats/4096_warm | 44,156 -> 44,266 | 0 / 0 -> 0 / 0 | 0 -> 0 | -0.3% |

The unchanged three-value warm control measures 4 ns slower in this build;
other warm controls vary in both directions. These expose layout and machine
variation alongside the changed cold path. Heap allocation counts are zero
for every cold case through 64 values, including the original 0/1/2 shortcuts.


## Composed analysis controls

Complete analysis exercises timing normalization and NPS statistics alongside
the chart's other work. Raw spacing is a separate public API; the existing
reporting visitor already consumes minimized data and remains unchanged.

| Case | CPU cycles/call, old -> new | Allocations / reallocations, old -> new | Requested bytes/call, old -> new | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| analyze/fast_fake_lifts | 712,519 -> 700,117 | 31 / 4 -> 31 / 4 | 59,028 -> 59,028 | +2.2% |
| analyze/camellia | 468,367,636 -> 462,549,130 | 110 / 0 -> 110 / 0 | 5,263,624 -> 5,263,624 | +1.4% |
| analyze/fast_camellia | 59,901,913 -> 59,170,687 | 115 / 0 -> 115 / 0 | 7,051,152 -> 7,051,152 | +1.3% |
| analyze/mixed_small | 56,631 -> 54,187 | 59 / 3 -> 59 / 3 | 7,460 -> 7,460 | +4.5% |

## Behavioral validation

- 210 release tests passed: 60 rssp library, 138 core library and 12 integration
  tests. New tests compare cleanup with the existing owned APIs, spacing with
  minimization plus the production spacing visitor, and small NPS medians with
  in-place selection. They cover control characters, nonfinite values, signed
  zero, lane widths, row-count boundaries and empty measures.
- Original and final executables produced byte-identical component output on
  all 30,843 corpus files (30,489 supported inputs and 354 matching errors),
  covering 56,125 charts. Added comparisons include raw measure spacing and
  global timing-map normalization, alongside densities, breakdowns, duration,
  peak NPS and BPM snapshots. UTF-8 SHA-256:
  `9248db7d2bd754546c0755ebfee578231e0350c77b677bdbcbb8fcc2db3fe2ae`.
- After final benchmarks confirmed the changes, the exact required command
  passed: `cargo test --release --test all_parity -- --test-threads=22`.
  Result: 30,489 passed, zero failures, before commit.
- Release Clippy with `-D warnings`, formatting and git diff checks passed.

## Reproduction

Build the identical harness against baseline production and final production
with both manifests at 0.4.274:

```powershell
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_HOT_FILTER = 'cleanup/' # also spacing/, nps_stats/, analyze/
$env:RSSP_HOT_ITERS = '500'       # 30 for analysis
& target/release/deps/hotpath_perf-<hash>.exe
$env:RSSP_HOT_VERIFY = '1'
& target/release/deps/hotpath_perf-<hash>.exe
```

Local executables, three-pair raw logs, measurement JSON and corpus output are
under the ignored target/perf-274 directory. rust-performance.md, optimize.sh
and optimize.ps1 are excluded from this commit.


# 0.4.275: move pattern strings, count in results, and skip single-BPM storage

## Changes

1. One-shot custom-pattern matching moves the temporary matcher's owned pattern
   strings into its result records. It eliminates a clone and heap allocation
   per unique nonempty pattern. Compiled matchers retain their reusable ownership.
2. Both custom-pattern matching APIs accumulate hits directly in result records.
   The temporary count vector, its zero-fill, and the subsequent count copy are
   removed. The row-analysis API retains its existing caller-owned count storage.
   The compilation forwarding function is also removed.
3. The allocating BPM statistics APIs handle empty and single finite values
   directly, using a one-element array for the existing numeric calculation.
   They avoid filtering, heap storage, and the general summary handoff. Non-finite
   values retain their original path, preserving signaling-NaN quieting as well
   as ordinary NaN payloads, infinities, signed zero, and range sentinels. The
   reusable scratch API retains its original implementation and buffer effects.

The workspace patch version increases exactly once: 0.4.274 → 0.4.275.
Public signatures, pattern ordering, case-insensitive deduplication, overlapping
counts, and numeric outputs remain compatible.

## Method

Baseline production code is `bbc8488`. Baseline, ownership-only pattern search,
and final executables use version 0.4.275, identical measured benchmark code and
fixtures, the same checkout and target directory, and rustc 1.98.1 / LLVM 22.1.8
on x86_64-pc-windows-msvc. The release profile uses fat LTO and one codegen unit.
Hardware is an Intel Xeon E5-2696 v4 (22 cores / 44 logical processors); the
measurement thread is pinned to logical CPU 2.

Each process reports medians of seven warmed batches. The tables aggregate three
alternating baseline/new process pairs. Component cases use 500 calls per batch;
BPM statistics use 5,000; analysis uses 30 (three for Camellia). Compilation,
fixture construction, and I/O stay outside measurement loops. Allocation counting
is a separate pass. Cycles use Windows QueryThreadCycleTime. Churn bytes sum
requested allocation/reallocation sizes, rather than measuring peak RSS. There
are no reallocations in the custom-pattern or BPM-statistics cases below.
No builds or tests run during final measurements. Shared-machine activity and
code layout affect timing; deterministic allocation changes are the strongest
evidence for small differences.

## String ownership in isolation

This intermediate measurement retains the original count vector and search
implementation. Each eight-character pattern loses one eight-byte string
allocation. Inputs contain 128 periodic masks and 4, 32, or 256 unique patterns.

| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `custom/4_128_owned` | 3,337 → 2,877 | 7,254 → 6,302 | 20 → 16 | 3,328 → 3,296 | +16.0% |
| `custom/32_128_owned` | 15,772 → 14,444 | 34,484 → 31,581 | 76 → 44 | 25,184 → 24,928 | +9.2% |
| `custom/256_128_owned` | 122,294 → 99,393 | 267,369 → 216,770 | 524 → 268 | 194,272 → 192,224 | +23.0% |


## Direct result counting and final custom matching

Compiled cases isolate removal of the count vector: one fewer allocation and
9.1% less requested heap storage per call. One-shot cases include both ownership
reuse and direct counting. Compilation cases are controls. Each pattern contains
eight symbols; texts contain 128 or 4,096 periodic masks. Compilation and compiled
search are measured separately from the composed one-shot API.

| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `custom/4_compile` | 3,005 → 2,931 | 6,570 → 6,365 | 14 → 14 | 3,152 → 3,152 | +2.5% |
| `custom/4_128_owned` | 3,189 → 2,863 | 6,969 → 6,258 | 20 → 15 | 3,328 → 3,280 | +11.4% |
| `custom/4_128_compiled` | 767 → 731 | 1,680 → 1,600 | 6 → 5 | 176 → 160 | +4.9% |
| `custom/4_4096_owned` | 14,898 → 14,560 | 32,639 → 31,843 | 20 → 15 | 3,328 → 3,280 | +2.3% |
| `custom/4_4096_compiled` | 12,491 → 12,352 | 27,261 → 26,986 | 6 → 5 | 176 → 160 | +1.1% |
| `custom/32_compile` | 17,172 → 17,152 | 37,505 → 37,482 | 42 → 42 | 23,776 → 23,776 | +0.1% |
| `custom/32_128_owned` | 16,866 → 13,946 | 36,834 → 30,497 | 76 → 43 | 25,184 → 24,800 | +20.9% |
| `custom/32_128_compiled` | 3,219 → 3,028 | 7,046 → 6,633 | 34 → 33 | 1,408 → 1,280 | +6.3% |
| `custom/32_4096_owned` | 28,450 → 26,160 | 62,154 → 57,135 | 76 → 43 | 25,184 → 24,800 | +8.8% |
| `custom/32_4096_compiled` | 15,043 → 14,878 | 32,962 → 32,552 | 34 → 33 | 1,408 → 1,280 | +1.1% |
| `custom/256_compile` | 97,675 → 100,753 | 213,464 → 220,158 | 266 → 266 | 183,008 → 183,008 | -3.1% |
| `custom/256_128_owned` | 118,392 → 99,530 | 258,850 → 217,510 | 524 → 267 | 194,272 → 191,200 | +19.0% |
| `custom/256_128_compiled` | 19,491 → 19,640 | 42,652 → 42,970 | 258 → 257 | 11,264 → 10,240 | -0.8% |
| `custom/256_4096_owned` | 129,439 → 111,182 | 283,072 → 243,047 | 524 → 267 | 194,272 → 191,200 | +16.4% |
| `custom/256_4096_compiled` | 32,713 → 32,379 | 71,518 → 70,793 | 258 → 257 | 11,264 → 10,240 | +1.0% |


## BPM statistics

The finite single-value convenience APIs eliminate their allocation and eight
requested bytes per call. Empty, multi-value, and caller-owned scratch cases
provide controls. Selection and arithmetic for larger inputs retain the existing
implementation. Cold cases include allocation when it is part of the public API;
warm scratch cases reuse preallocated storage.

| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `bpm_stats/0_values` | 6 → 6 | 12 → 13 | 0 → 0 | 0 → 0 | +0.0% |
| `bpm_stats/0_map` | 6 → 6 | 13 → 13 | 0 → 0 | 0 → 0 | +0.0% |
| `bpm_stats/0_summary_cold` | 11 → 6 | 25 → 12 | 0 → 0 | 0 → 0 | +83.3% |
| `bpm_stats/0_summary_warm` | 8 → 8 | 16 → 16 | 0 → 0 | 0 → 0 | +0.0% |
| `bpm_stats/1_values` | 77 → 6 | 165 → 13 | 1 → 0 | 8 → 0 | +1183.3% |
| `bpm_stats/1_map` | 75 → 6 | 163 → 13 | 1 → 0 | 8 → 0 | +1150.0% |
| `bpm_stats/1_summary_cold` | 85 → 13 | 186 → 28 | 1 → 0 | 8 → 0 | +553.8% |
| `bpm_stats/1_summary_warm` | 18 → 18 | 40 → 40 | 0 → 0 | 0 → 0 | +0.0% |
| `bpm_stats/2_values` | 82 → 82 | 180 → 180 | 1 → 1 | 16 → 16 | +0.0% |
| `bpm_stats/2_map` | 82 → 82 | 179 → 178 | 1 → 1 | 16 → 16 | +0.0% |
| `bpm_stats/2_summary_cold` | 101 → 100 | 220 → 220 | 1 → 1 | 16 → 16 | +1.0% |
| `bpm_stats/2_summary_warm` | 28 → 28 | 60 → 60 | 0 → 0 | 0 → 0 | +0.0% |
| `bpm_stats/8_values` | 103 → 108 | 225 → 237 | 1 → 1 | 64 → 64 | -4.6% |
| `bpm_stats/8_map` | 105 → 107 | 230 → 234 | 1 → 1 | 64 → 64 | -1.9% |
| `bpm_stats/8_summary_cold` | 131 → 137 | 287 → 299 | 1 → 1 | 64 → 64 | -4.4% |
| `bpm_stats/8_summary_warm` | 59 → 65 | 128 → 141 | 0 → 0 | 0 → 0 | -9.2% |
| `bpm_stats/64_values` | 451 → 443 | 987 → 971 | 1 → 1 | 512 → 512 | +1.8% |
| `bpm_stats/64_map` | 413 → 406 | 899 → 887 | 1 → 1 | 512 → 512 | +1.7% |
| `bpm_stats/64_summary_cold` | 530 → 524 | 1,153 → 1,149 | 1 → 1 | 512 → 512 | +1.1% |
| `bpm_stats/64_summary_warm` | 445 → 450 | 972 → 978 | 0 → 0 | 0 → 0 | -1.1% |


## Composed paths and controls

Default analysis uses empty custom-pattern configurations and warm BPM storage,
so these changes primarily benefit callers of the targeted convenience APIs.
Serialization is unchanged and serves as another control. Timing variations in
these controls do not establish a whole-analysis improvement.

| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `serialize/small_sm` | 4,476 → 4,498 | 9,804 → 9,848 | 0 → 0 | 0 → 0 | -0.5% |
| `serialize/small_ssc` | 6,284 → 6,179 | 13,753 → 13,547 | 0 → 0 | 0 → 0 | +1.7% |
| `serialize/timing_ssc` | 20,679 → 20,529 | 45,222 → 44,852 | 0 → 0 | 0 → 0 | +0.7% |
| `serialize/large_ssc` | 193,092 → 191,540 | 421,702 → 418,861 | 0 → 0 | 0 → 0 | +0.8% |
| `analyze/fast_fake_lifts` | 307,230 → 308,003 | 670,821 → 672,489 | 31 → 31 | 59,028 → 59,028 | -0.3% |
| `analyze/camellia` | 210,674,033 → 212,967,867 | 460,587,530 → 465,558,916 | 110 → 110 | 5,263,624 → 5,263,624 | -1.1% |
| `analyze/fast_camellia` | 27,944,433 → 27,630,767 | 60,941,029 → 60,341,282 | 115 → 115 | 7,051,152 → 7,051,152 | +1.1% |
| `analyze/mixed_small` | 33,933 → 23,887 | 73,935 → 51,773 | 59 → 59 | 7,460 → 7,460 | +42.1% |


## Behavioral validation

- Release library suites: 60 rssp tests and 138 core tests passed.
- Release optimization regressions: 14 passed. These cover mixed-case duplicates,
  empty/unknown/Unicode patterns, high mask bits, repeated matcher reuse, and 64
  overlapping suffix patterns; BPM checks compare exact output bits and original
  scratch-buffer contents/capacity, including signaling NaNs.
- `cargo test --release --test all_parity -- --test-threads=22`: 30,489 passed,
  zero failed, after final benchmarks and before commit.
- Strict release Clippy for all workspace targets, formatting, and diff checks
  passed.
- Baseline/final corpus output is byte-identical across 30,843 files and 56,125
  supported charts, including 354 matching parse errors. The explicit comparison
  includes custom matching, finite/non-finite single BPMs, serialized fixtures,
  densities, spacing, breakdowns, durations, peak NPS, normalization, and BPM
  snapshots. UTF-8 SHA-256: `9cd4f319db50e03f9861ddf483e447afcf78a32200292972d245f0692b581ab3`.

## Reproduction

Build `bbc8488` with the checked-in benchmark changes and version 0.4.275 to save
the baseline executable, then build the final implementation with the same version:

```powershell
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_HOT_FILTER = 'custom/' # also bpm_stats/, analyze/, serialize/
$env:RSSP_HOT_ITERS = '500' # 30 for analyze/
$bench = Get-ChildItem target/release/deps/hotpath_perf-*.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $bench.FullName
```

Alternate saved baseline/final executables three times and aggregate their seven
batch medians. Set `RSSP_HOT_VERIFY=1` to emit the deterministic corpus comparison.
Local binaries, raw runs, JSON results, and verification logs are in
`target/perf-275/` and remain outside the commit.

# 0.4.276: avoid temporary BPM storage and zero-output work

Three changes remove work whose result is already known or whose temporary
storage can stay on the stack:

1. `compute_bpm_stats` and `compute_bpm_map_stats` use a bounded 32-value stack
   buffer for small inputs. They share filtering and call the original sorting,
   median, and average implementation. The one-finite-value shortcut, larger
   inputs, and caller-owned scratch API keep their existing algorithms.
2. `stream_sequences` finds the first stream before reserving its output vector.
   Charts without streams return an empty vector directly. A shared walker
   starts at that first index, avoiding both a repeated leading-gap scan and
   per-segment reservation checks. The visitor preserves ordering, leading and
   trailing breaks, one-measure gap handling, and error propagation.
3. `compute_chart_durations` returns the existing zero duration when the last
   beat is nonpositive before resolving or building chart timing. Metadata is
   still returned. Skipping these charts also avoids replacing the local timing
   cache with data that cannot affect their result.

The workspace patch increases exactly once: **0.4.275 → 0.4.276**.

## Method

Baseline production code is `87476fc`. Both saved executables use version
0.4.276, identical benchmark code and fixtures, Rust 1.98.1 / LLVM 22.1.8,
Windows x86-64, and the normal fat-LTO bench profile. The benchmark pins its
thread to logical CPU 2. Three process pairs alternate old/new, new/old,
old/new; each result is the median of the process's seven warmed batches,
followed by the median across those three runs. Composed analysis controls were
repeated once and combine all six process pairs. Allocations are counted in a
separate pass with the counter disabled during CPU/timing measurements.

Inputs are prepared outside measurement. Parsing is intentionally measured by
the duration and analysis cases; disk I/O is excluded. Heap churn bytes sum
allocation and reallocation requests, rather than measuring peak resident
memory. The small BPM buffer uses 256 bytes of stack storage. Raw output,
62-case JSON results, and saved binaries remain in `target/perf-276/`.

## Small BPM statistics

All six measured 2/8/32-value convenience cases eliminate their one temporary
heap allocation. Tests compare exact median/average bits against the original
scratch API, including filtering, signed zero, infinities, NaN payloads, and
signaling NaNs. Existing sort rejection for some non-total NaN inputs is also
preserved.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `bpm_stats/2_values` | 80 → 24 | 175 → 51 | 1 → 0 | 16 → 0 | +233.3% |
| `bpm_stats/2_map` | 92 → 25 | 201 → 54 | 1 → 0 | 16 → 0 | +268.0% |
| `bpm_stats/8_values` | 106 → 40 | 231 → 87 | 1 → 0 | 64 → 0 | +165.0% |
| `bpm_stats/8_map` | 101 → 44 | 221 → 96 | 1 → 0 | 64 → 0 | +129.5% |
| `bpm_stats/32_values` | 445 → 322 | 967 → 704 | 1 → 0 | 256 → 0 | +38.2% |
| `bpm_stats/32_map` | 447 → 308 | 978 → 674 | 1 → 0 | 256 → 0 | +45.1% |

## Stream output without temporary segment storage

The 4,096-measure fixture alternates densities 0 and 15, producing no streams.
The segment vector formerly reserved 1,024 entries (24,576 bytes), then was
discarded empty. Output strings still have their required owned storage.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `standard/empty_detailed` | 3,170 → 2,047 | 6,953 → 4,431 | 2 → 1 | 24,587 → 11 | +54.9% |
| `standard/empty_partial` | 3,492 → 2,274 | 7,648 → 4,935 | 2 → 1 | 24,587 → 11 | +53.6% |
| `standard/empty_simple` | 3,471 → 2,163 | 7,607 → 4,710 | 2 → 1 | 24,587 → 11 | +60.5% |
| `standard/empty_three` | 3,468 → 2,357 | 7,599 → 5,111 | 4 → 3 | 24,609 → 33 | +47.1% |

## Zero-duration timing work

Each fixture contains four supported charts. `empty` charts contain only zeros;
`first` charts contain one note at beat zero. Both have always returned zero
duration, including when an offset is present. Local timing fixtures vary the
chart offset, forcing four timing cache misses in the original implementation.
The 128-entry fixtures have 128 stops and 128 delays per timing source.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `zero_duration/1_empty_global` | 4,083 → 3,568 | 8,934 → 7,757 | 19 → 10 | 2,496 → 2,368 | +14.4% |
| `zero_duration/1_first_global` | 4,619 → 3,544 | 10,089 → 7,771 | 19 → 10 | 2,496 → 2,368 | +30.3% |
| `zero_duration/1_empty_local` | 9,024 → 4,158 | 19,771 → 9,119 | 46 → 10 | 2,880 → 2,368 | +117.0% |
| `zero_duration/1_first_local` | 8,686 → 4,193 | 19,032 → 9,182 | 46 → 10 | 2,880 → 2,368 | +107.2% |
| `zero_duration/128_empty_global` | 50,029 → 13,104 | 108,755 → 28,720 | 19 → 10 | 13,672 → 2,368 | +281.8% |
| `zero_duration/128_first_global` | 51,596 → 13,340 | 112,937 → 29,104 | 19 → 10 | 13,672 → 2,368 | +286.8% |
| `zero_duration/128_empty_local` | 194,175 → 14,686 | 424,859 → 32,057 | 46 → 10 | 47,584 → 2,368 | +1222.2% |
| `zero_duration/128_first_local` | 194,762 → 14,520 | 426,008 → 31,450 | 46 → 10 | 47,584 → 2,368 | +1241.3% |

## Other paths and controls

These measurements retain nonempty streams, positive-duration charts, larger
BPM maps, and composed analysis. Their allocation/reallocation counts and
requested bytes are unchanged. CPU timings on this shared machine vary across
unchanged controls too; the targeted results do not establish a general
whole-analysis speedup. Full raw runs record every measured case.
The default Camellia analysis control is about 3.5% slower across the combined
six pairs, so this pass makes no claim of improved default-analysis throughput.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `standard/uniform_three` | 3,274 → 2,876 | 7,168 → 6,298 | 4 → 4 | 24,594 → 24,594 | +13.8% |
| `standard/fragmented_three` | 35,801 → 35,588 | 78,379 → 77,871 | 4 → 4 | 103,212 → 103,212 | +0.6% |
| `standard/short_detailed` | 658 → 575 | 1,440 → 1,260 | 2 → 2 | 492 → 492 | +14.4% |
| `standard/short_three` | 635 → 511 | 1,389 → 1,119 | 4 → 4 | 660 → 660 | +24.3% |
| `bpm_stats/64_values` | 450 → 437 | 985 → 954 | 1 → 1 | 512 → 512 | +3.0% |
| `bpm_stats/8_summary_warm` | 67 → 53 | 145 → 116 | 0 → 0 | 0 → 0 | +26.4% |
| `zero_duration/128_nonzero_local` | 196,057 → 202,304 | 428,696 → 442,261 | 46 → 46 | 47,584 → 47,584 | -3.1% |
| `duration/clean_hit` | 95,606 → 96,484 | 209,188 → 210,834 | 21 → 21 | 25,815 → 25,815 | -0.9% |
| `duration/dirty_hit` | 110,818 → 108,207 | 242,122 → 236,419 | 25 → 25 | 32,227 → 32,227 | +2.4% |
| `duration/dirty_miss` | 404,903 → 409,710 | 885,820 → 895,992 | 70 → 70 | 120,223 → 120,223 | -1.2% |
| `duration/camellia` | 6,209,132 → 5,851,662 | 13,583,362 → 12,809,050 | 18 → 18 | 3,635 → 3,635 | +6.1% |
| `analyze/camellia` | 195,965,500 → 202,958,016 | 428,402,831 → 443,739,297 | 110 → 110 | 5,263,624 → 5,263,624 | -3.4% |
| `analyze/fast_camellia` | 25,167,700 → 25,316,883 | 55,030,333 → 55,335,364 | 115 → 115 | 7,051,152 → 7,051,152 | -0.6% |
| `analyze/mixed_small` | 23,218 → 22,548 | 50,668 → 49,135 | 59 → 59 | 7,460 → 7,460 | +3.0% |

## Behavioral validation

- Release library suites: 60 rssp and 138 core tests passed.
- Release optimization regressions: 18 passed. New cases cover small BPM exact
  bits and original sort behavior; zero durations, offsets, metadata and cache
  transitions; stream gaps, long leading breaks and visitor error propagation.
- `cargo test --release --test all_parity -- --test-threads=22`: 30,489 passed,
  zero failed, after the final optimizations were confirmed and before commit.
- Strict release Clippy for all workspace targets, formatting and diff checks
  passed.
- Original/final output is byte-identical across 30,843 corpus files and 56,125
  supported charts, including 354 matching parse errors. The comparison covers
  durations, densities, spacing, stream breakdowns, normalization, peak NPS,
  BPM snapshots, custom matching and serialized fixtures. UTF-8 SHA-256:
  `9cd4f319db50e03f9861ddf483e447afcf78a32200292972d245f0692b581ab3`.

## Reproduction

Build `87476fc` with only the version bump to 0.4.276 and the final benchmark
additions to save the baseline executable. Then build the final production code
with the same version and harness:

```powershell
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_HOT_FILTER = 'bpm_stats/' # also standard/, zero_duration/, duration/, analyze/
$env:RSSP_HOT_ITERS = '500' # 200 for zero_duration/, 30 for analyze/
$bench = Get-ChildItem target/release/deps/hotpath_perf-*.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $bench.FullName
```

Alternate baseline/final executables three times. The filter matches substrings,
so the driver records only cases starting with the selected namespace to avoid
counting `zero_duration/` again under `duration/`. Set `RSSP_HOT_VERIFY=1` to emit
the deterministic component and corpus comparison.

# 0.4.277: skip empty peak timing and repeated stream work


Three changes remove work from peak NPS and combined stream analysis:

1. `compute_chart_peak_nps` counts density before constructing timing. When
   every measure has zero density, it returns the existing zero peak with its
   metadata. Mines, fakes, lifts, keysounds and hold tails do not contribute to
   density; a scored note at beat zero still follows the normal timing path.
2. Peak NPS retains the last local elapsed-time data for this call. Equal raw
   offset/BPM/stop/delay/warp tags reuse it before cleanup and construction.
   Cleanup now occurs only on local timing misses. The cache stores the source
   chart's index and compares its raw fields directly, avoiding a copied timing
   key or any new key type/helper. Parsed entries remain immutable and stable
   throughout the call. `duration.rs` is unchanged.
3. Combined stream counting finds the first stream before reserving token
   storage. No-stream charts need no token allocation. Leading breaks use the
   first index directly, skipping their counting/tokenization loop work.
   Reservation excludes the leading gap. The active-range loop keeps its
   existing structure, which measured better for short cold inputs.

The patch version increases exactly once: **0.4.276 → 0.4.277**.

## Method

The baseline is `e469cfc` production code built with version 0.4.277 and the
same final benchmark harness. Both executables use Rust 1.98.1 / LLVM 22.1.8,
Windows x86-64 and fat LTO. The benchmark pins its thread to logical CPU 2.
Three process pairs alternate old/new, new/old and old/new. Each process uses
seven warmed batches; tables report the median across the three process
medians. The counting allocator runs separately from CPU/timing measurement.

Fixtures are prepared outside measurement. Peak and duration cases intentionally
include parsing and timing construction, with no disk I/O. Heap churn sums
allocation/reallocation requests, not resident memory. All 32 measured cases,
raw runs, comparison JSON and saved executables remain in `target/perf-277/`.

## Zero-density peak NPS

Each fixture has four charts, with either 1 or 128 stops and delays per source.
`objects_local` contains only non-scoring objects. The original builds timing
for all four charts; the new implementation skips each local build. Global
normalization remains part of the API and is included in the measurements.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `peak_work/1_empty_global` | 5,942 → 3,838 | 13,006 → 8,405 | 22 → 13 | 2,526 → 2,398 | +54.8% |
| `peak_work/1_objects_local` | 12,468 → 4,353 | 27,334 → 9,545 | 61 → 13 | 3,034 → 2,398 | +186.4% |
| `peak_work/128_empty_global` | 54,191 → 15,395 | 118,679 → 33,760 | 22 → 13 | 17,206 → 5,902 | +252.0% |
| `peak_work/128_objects_local` | 224,046 → 16,764 | 490,786 → 36,698 | 61 → 13 | 65,258 → 5,902 | +1236.5% |

## Repeated local timing

`first_local` has one scored note at beat zero, which has nonzero peak NPS.
`repeat_local` has four notes per measure. Equal dirty timing tags across four
charts eliminate three cleanup/build cycles. The inherited/global and varying
local timing controls are shown below as well.

The cache is owned by the caller during this parsing operation, single-threaded
and local to the call. It holds at most one local timing entry, warming on the
first nonempty local chart. Misses perform the original CPU-only build;
replacement and return destroy the old data on the caller thread. There are no
scans, pruning, locks or I/O. Lookups compare at most five tags, bounded by
their input byte lengths; misses retain the original input-dependent build
cost. Empty and inherited charts preserve the last local entry. This API runs
at chart load and must not run during gameplay. Allocation counters in the
benchmark expose the skipped builds; no persistent instrumentation is added.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `peak_work/1_first_local` | 12,318 → 6,958 | 26,989 → 15,240 | 61 → 25 | 3,034 → 2,557 | +77.0% |
| `peak_work/1_repeat_local` | 12,450 → 7,048 | 27,295 → 15,426 | 61 → 25 | 3,042 → 2,565 | +76.6% |
| `peak_work/128_first_local` | 226,674 → 66,158 | 496,178 → 144,920 | 61 → 25 | 65,258 → 20,741 | +242.6% |
| `peak_work/128_repeat_local` | 222,447 → 66,374 | 487,119 → 145,178 | 61 → 25 | 65,266 → 20,749 | +235.1% |

## Combined stream counts and breakdowns

The empty fixture alternates densities 0 and 15 across 4,096 measures. The
leading fixture has 4,000 leading breaks and a 96-measure stream. Both APIs
return identical counts and all six breakdown strings. Warm cases reuse token
storage; cold cases include its creation and destruction. A token is 16 bytes:
skipping the empty reservation saves 16,384 requested bytes, and reserving only
the leading fixture's active suffix saves 14,848 bytes.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `streams/empty_combined` | 4,506 → 2,473 | 9,858 → 5,417 | 3 → 3 | 33 → 33 | +82.2% |
| `streams/empty_cold` | 4,600 → 2,534 | 10,088 → 5,479 | 4 → 3 | 16,417 → 33 | +81.5% |
| `streams/leading_combined` | 4,776 → 2,967 | 10,469 → 6,503 | 6 → 6 | 33 → 33 | +61.0% |
| `streams/leading_cold` | 4,898 → 3,364 | 10,721 → 7,333 | 7 → 7 | 16,417 → 1,569 | +45.6% |
| `streams/uniform_combined` | 10,970 → 11,570 | 24,012 → 25,324 | 6 → 6 | 33 → 33 | -5.2% |
| `streams/uniform_cold` | 10,969 → 11,466 | 24,026 → 25,127 | 7 → 7 | 16,417 → 16,417 | -4.3% |
| `streams/fragmented_combined` | 123,764 → 124,716 | 270,886 → 271,089 | 6 → 6 | 72,039 → 72,039 | -0.8% |
| `streams/fragmented_cold` | 112,568 → 111,280 | 246,520 → 243,655 | 7 → 7 | 186,727 → 186,727 | +1.2% |
| `streams/short_combined` | 1,887 → 1,858 | 4,132 → 4,060 | 6 → 6 | 891 → 891 | +1.6% |
| `streams/short_cold` | 2,152 → 2,209 | 4,712 → 4,833 | 7 → 7 | 1,403 → 1,403 | -2.6% |

## Other paths and composed controls

The remaining controls exercise ordinary timing, local cache misses, unchanged
duration processing, and full analysis. Allocation counts and requested
bytes are unchanged except where peak timing construction is avoided. Timings
on this shared machine vary; the component gains do not imply the same gain in
default analysis, where parity processing dominates.

Not every CPU control improved: the unchanged `duration/camellia` path measured
6.7% lower throughput, uniform stream controls were 4.3–5.2% lower, and default
Camellia analysis was 1.2% lower. This pass establishes allocation savings and
the targeted empty/repeated-timing gains, with exact behavioral parity; it does
not establish regression-free throughput for every workload. The cause of the
unchanged-duration slowdown was not established. Whole-program code layout and
shared-machine variation are possible explanations, not confirmed causes.


| Case | ns old → new | cycles old → new | allocations old → new | churn bytes old → new | throughput change |
| --- | ---: | ---: | ---: | ---: | ---: |
| `peak_work/1_vary_local` | 12,414 → 12,442 | 27,213 → 27,266 | 61 → 61 | 3,042 → 3,042 | -0.2% |
| `peak_work/128_vary_local` | 222,639 → 209,322 | 487,643 → 458,348 | 61 → 61 | 65,266 → 65,266 | +6.4% |
| `peak/plain` | 10,935 → 10,598 | 23,952 → 23,229 | 14 → 14 | 3,223 → 3,223 | +3.2% |
| `peak/global_aux` | 11,111 → 10,963 | 24,344 → 24,024 | 14 → 14 | 3,223 → 3,223 | +1.3% |
| `peak/local_aux` | 22,785 → 17,394 | 49,819 → 38,141 | 55 → 22 | 4,031 → 3,395 | +31.0% |
| `peak/camellia` | 4,393,635 → 4,031,770 | 9,619,665 → 8,830,836 | 25 → 19 | 213,011 → 212,867 | +9.0% |
| `duration/clean_hit` | 91,620 → 92,206 | 200,524 → 202,039 | 21 → 21 | 25,815 → 25,815 | -0.6% |
| `duration/dirty_hit` | 111,050 → 111,500 | 243,227 → 244,075 | 25 → 25 | 32,227 → 32,227 | -0.4% |
| `duration/dirty_miss` | 396,426 → 397,688 | 868,153 → 870,558 | 70 → 70 | 120,223 → 120,223 | -0.3% |
| `duration/camellia` | 5,944,230 → 6,374,435 | 12,960,860 → 13,955,821 | 18 → 18 | 3,635 → 3,635 | -6.7% |
| `analyze/fast_fake_lifts` | 289,637 → 301,357 | 633,579 → 660,285 | 31 → 31 | 59,028 → 59,028 | -3.9% |
| `analyze/camellia` | 206,212,533 → 208,797,733 | 451,466,209 → 456,118,074 | 110 → 110 | 5,263,624 → 5,263,624 | -1.2% |
| `analyze/fast_camellia` | 27,286,400 → 26,304,833 | 59,734,877 → 57,586,118 | 115 → 115 | 7,051,152 → 7,051,152 | +3.7% |
| `analyze/mixed_small` | 24,400 → 22,563 | 53,192 → 49,205 | 59 → 59 | 7,460 → 7,460 | +8.1% |

## Validation

- Release library tests: 60 rssp and 138 core passed; 21 optimization regressions
  passed. New cases cover all supported lane counts, non-scoring objects, a
  beat-zero note, legacy SSC versions, all timing key fields, inherited timing,
  auxiliary override tags, skipped unsupported charts, large leading/trailing
  gaps and reused token storage.
- `cargo test --release --test all_parity -- --test-threads=22`: 30,489 passed,
  zero failed, after the final optimizations were confirmed and before commit.
- Strict release Clippy for every workspace target, formatting and diff checks
  passed.
- Original/final output is byte-identical across 30,843 corpus files and 56,125
  supported charts, including matching parse errors. This checks exact peak and
  duration bits, densities, spacing, stream strings, normalized timing, BPM
  snapshots, custom matching and serialized fixtures. SHA-256:
  `9cd4f319db50e03f9861ddf483e447afcf78a32200292972d245f0692b581ab3`.

## Reproduction

Build `e469cfc` with only version 0.4.277 and the final benchmark harness to
save the original executable; build the final production code with the same
version and harness. Alternate the executables three times:

```powershell
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_HOT_FILTER = 'peak_work/' # also peak/, streams/, duration/, analyze/
$env:RSSP_HOT_ITERS = '200' # 30 for analyze/
$bench = Get-ChildItem target/release/deps/hotpath_perf-*.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $bench.FullName
```

`RSSP_HOT_VERIFY=1` prints deterministic component and corpus outputs. The
excluded performance guide and optimization scripts are not included in the
commit. Hash coalescing trials were discarded after measured regressions;
`hash.rs` is unchanged.


# Performance pass 0.4.278

Baseline: `c6fbe49` (0.4.277), compiled with version 0.4.278 and the same
benchmark harness as the optimized code. This pass bumps the patch exactly
once and removes 45 net production lines. `rust-performance.md` was reviewed;
it and the two optimization scripts are excluded from the commit.

## Changes

1. **Delete ineffective BPM normalization.** `normalize_and_tidy_bpms` formatted
   every accepted number with three decimal places and then parsed that string
   as `i64`. No entry could survive: finite values contain a decimal point and
   nonfinite values are not integers. Return the existing `0.000=60.000`
   fallback directly and delete both now-unused formatting wrappers and the
   unreachable sort/dedup/output pipeline. This preserves a legacy behavior;
   it does not repair the API's normalization semantics. Its performance gain
   applies to callers of this convenience API, which analysis does not call.
2. **Reuse normalized BPM strings in batch hashing.** Borrow the global string
   for identical raw local maps and retain one normalized local map across
   charts. Compare raw bytes before normalization, preserving absent, empty,
   invalid UTF-8 and distinct-map behavior. Move the source BPM tag into the
   cache so unrelated owned chart tags are freed as entries are consumed;
   no raw tag is cloned. The cache holds one entry for one caller-thread call;
   misses replace it without I/O or pruning. The hash API belongs at load time.
3. **Move decoded credits into their final field.** Replace
   `decode_bytes -> unescape_tag -> into_owned` with the existing
   `decode_unescape -> into_owned` path. Owned Windows-1252 decoding now reaches
   `step_artist_str` without another allocation/copy. Escapes use the existing
   in-place implementation; UTF-8 borrowing, whitespace and trailing escapes
   retain their behavior. No new decoder, API or abstraction was added.

The proposed matrix aggregation change was discarded after dense-input CPU
regressions. `matrix.rs` is unchanged.

## Measurements

Rust 1.98.1 / LLVM 22.1.8, release fat LTO, Xeon E5-2696 v4, Windows,
44 logical CPUs; benchmark thread pinned to CPU 2. Three alternating
old/new process pairs, seven batches per process, medians of process medians.
Use 200 iterations per batch, except composed analysis uses 30 (Camellia
internally uses three). Setup is outside timing; counted allocations are a
separate single invocation. CPU cycles are from `QueryThreadCycleTime`.
Requested bytes include reallocations and represent allocation churn, not RSS.


- At 4,096 BPM entries, fallback normalization eliminates 8,192 allocations
  and 131,072 requested bytes; CPU cycles fall from 2,945,497 -> 149.
- Eight charts with 128 identical global BPM entries: cycles 467,883 -> 147,350
  (3.2x throughput), eight fewer allocations, 55,720 fewer requested bytes.
  Repeated local maps: cycles 474,359 -> 191,536 (2.5x throughput), seven fewer
  allocations, 48,755 fewer requested bytes.
- Four charts with 4,096-byte Windows-1252 credits: four fewer allocations and
  20,496 fewer requested bytes, a 41.1% churn reduction. Escaped credits save
  19,672 bytes (40.8%). This is an allocation improvement; composed throughput
  changes are +0.5% and -7.6% respectively.
- Unique local-map misses retain the original allocation counts and bytes:
  throughput -1.3% at one entry and -3.2% at 128 entries. The main Camellia
  analysis control is +0.2%, fast Camellia -3.7%, duration Camellia -5.0%.
  Small controls vary in both directions, including short UTF-8 credits -0.5%
  and short warm streams +3.3%. These measurements do not establish a
  universal CPU improvement or the cause of small declines. Behavioral results
  are unchanged; the deterministic allocation reductions are the credit benefit.

| Case | Cycles old -> new | Throughput | Allocs old -> new | Requested bytes old -> new |
|---|---:|---:|---:|---:|
| `tidy_bpm/1` | 754 -> 149 | +383.3% | 3 -> 1 | 44 -> 12 |
| `tidy_bpm/128` | 88,379 -> 149 | +55962.5% | 257 -> 1 | 4,108 -> 12 |
| `tidy_bpm/4096` | 2,945,497 -> 149 | +1867327.8% | 8193 -> 1 | 131,084 -> 12 |
| `hash_batch/1_global` | 31,892 -> 22,798 | +39.3% | 36 -> 28 | 5,563 -> 5,283 |
| `hash_batch/1_repeat` | 30,895 -> 23,373 | +31.7% | 36 -> 29 | 5,563 -> 5,318 |
| `hash_batch/1_vary` | 30,675 -> 23,193 | +32.3% | 36 -> 29 | 5,563 -> 5,318 |
| `hash_batch/1_distinct` | 29,807 -> 30,198 | -1.3% | 36 -> 36 | 5,563 -> 5,563 |
| `hash_batch/128_global` | 467,883 -> 147,350 | +217.5% | 36 -> 28 | 67,933 -> 12,213 |
| `hash_batch/128_repeat` | 474,359 -> 191,536 | +147.7% | 36 -> 29 | 67,933 -> 19,178 |
| `hash_batch/128_vary` | 466,836 -> 190,698 | +144.9% | 36 -> 29 | 67,933 -> 19,178 |
| `hash_batch/128_distinct` | 471,985 -> 487,737 | -3.2% | 36 -> 36 | 67,933 -> 67,933 |
| `credit/16_utf8` | 31,482 -> 31,548 | -0.5% | 62 -> 62 | 8,930 -> 8,930 |
| `credit/16_utf8_escape` | 31,847 -> 31,213 | +2.1% | 62 -> 62 | 8,930 -> 8,930 |
| `credit/16_cp1252` | 32,511 -> 31,628 | +3.1% | 66 -> 62 | 9,042 -> 8,946 |
| `credit/16_cp1252_escape` | 32,790 -> 31,755 | +3.4% | 66 -> 62 | 9,026 -> 8,938 |
| `credit/4096_utf8` | 111,131 -> 117,985 | -5.8% | 62 -> 62 | 25,250 -> 25,250 |
| `credit/4096_utf8_escape` | 181,015 -> 194,455 | -7.3% | 62 -> 62 | 25,250 -> 25,250 |
| `credit/4096_cp1252` | 239,176 -> 238,075 | +0.5% | 66 -> 62 | 49,842 -> 29,346 |
| `credit/4096_cp1252_escape` | 312,143 -> 337,918 | -7.6% | 66 -> 62 | 48,194 -> 28,522 |
| `matrix/long_segments` | 130,938 -> 125,562 | +4.3% | 2 -> 2 | 5,200 -> 5,200 |
| `matrix/short_segments` | 132,439 -> 125,871 | +5.2% | 2 -> 2 | 5,104 -> 5,104 |
| `analyze/fast_fake_lifts` | 671,099 -> 623,270 | +7.6% | 31 -> 31 | 59,028 -> 59,028 |
| `analyze/camellia` | 448,337,895 -> 448,300,364 | +0.2% | 110 -> 110 | 5,263,624 -> 5,263,624 |
| `analyze/fast_camellia` | 56,283,020 -> 58,365,050 | -3.7% | 115 -> 115 | 7,051,152 -> 7,051,152 |
| `analyze/mixed_small` | 48,414 -> 48,488 | +0.3% | 59 -> 59 | 7,460 -> 7,460 |
| `duration/clean_hit` | 199,854 -> 199,449 | +1.0% | 21 -> 21 | 25,815 -> 25,815 |
| `duration/dirty_hit` | 234,957 -> 233,799 | +0.4% | 25 -> 25 | 32,227 -> 32,227 |
| `duration/dirty_miss` | 867,482 -> 866,384 | +0.2% | 70 -> 70 | 120,223 -> 120,223 |
| `duration/camellia` | 13,213,318 -> 13,928,746 | -5.0% | 18 -> 18 | 3,635 -> 3,635 |
| `streams/uniform_combined` | 23,836 -> 24,012 | -1.0% | 6 -> 6 | 33 -> 33 |
| `streams/uniform_cold` | 24,676 -> 24,446 | +0.9% | 7 -> 7 | 16,417 -> 16,417 |
| `streams/fragmented_combined` | 249,771 -> 243,061 | +2.8% | 6 -> 6 | 72,039 -> 72,039 |
| `streams/fragmented_cold` | 243,837 -> 214,967 | +13.4% | 7 -> 7 | 186,727 -> 186,727 |
| `streams/empty_combined` | 5,489 -> 5,331 | +3.5% | 3 -> 3 | 33 -> 33 |
| `streams/empty_cold` | 5,583 -> 5,336 | +4.6% | 3 -> 3 | 33 -> 33 |
| `streams/short_combined` | 3,971 -> 3,843 | +3.3% | 6 -> 6 | 891 -> 891 |
| `streams/short_cold` | 4,724 -> 4,564 | +3.5% | 7 -> 7 | 1,403 -> 1,403 |
| `streams/leading_combined` | 6,482 -> 6,306 | +3.1% | 6 -> 6 | 33 -> 33 |
| `streams/leading_cold` | 7,250 -> 7,152 | +1.1% | 7 -> 7 | 1,569 -> 1,569 |

## Validation

- Release unit tests: 198 passed; focused optimization regressions: 24 passed.
  New cases compare original credit decoding/unescaping semantics directly,
  including invalid UTF-8, escaped Unicode and Windows-1252 text, preserved
  whitespace, trailing escapes and both SSC versions. BPM/hash tests cover
  nonfinite/extreme numeric inputs, empty tags and repeated/inherited/distinct
  timing transitions.
- Strict release Clippy for all workspace targets, formatting and diff checks
  passed.
- Standalone hash parity: 30,489 passed, zero failed, covering the changed
  `compute_all_hashes` API across the native corpus.
- After confirming the optimizations and before committing, the exact command
  `cargo test --release --test all_parity -- --test-threads=22` passed all
  30,489 tests with zero failures.
- Original/final deterministic output is byte-identical across 30,843 corpus
  files, 56,125 supported charts and the new component fixtures, with 354
  matching malformed-file errors. SHA-256 of 163,409,264 output bytes:
  `5f2dc873e903dbbf32ea004c19f9b317c3d878623eb817a6cf1ee443dbcab16f`.
  The corpus trace checks densities, spacing, stream strings, normalized maps,
  exact duration/peak bits and BPM snapshots; component fixtures additionally
  check credits, batch hashes, custom matching and serialized output. The full
  parity suite checks the composed analysis fields against the golden data.

## Reproduction

Build `c6fbe49` with only version 0.4.278 and the final benchmark harness;
save the executable, then build the optimized production code with the same
version/harness. Alternate the two saved executables three times:

```powershell
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_HOT_FILTER = 'hash_batch/' # also tidy_bpm/, credit/, matrix/, duration/, streams/
$env:RSSP_HOT_ITERS = '200' # 30 for analyze/
$bench = Get-ChildItem target/release/deps/hotpath_perf-*.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $bench.FullName
```

`RSSP_HOT_VERIFY=1` emits the deterministic component and corpus transcript.

# Performance pass 0.4.279

Baseline: `983d6a3` (0.4.278), compiled at version 0.4.279 with the same
benchmark harness as the optimized code. The patch version advances exactly
once. No public API or allocation is added. `rust-performance.md` was
reviewed and remains outside the commit,
along with `optimize.sh` and `optimize.ps1`.

## Changes

1. **Batch JSON indentation.** Replace one `write_all` per space with slices
   of a constant 64-byte block. Normal report indentation takes one write;
   larger widths use bounded chunks, retaining arbitrary-width behavior.
   Empty indentation does no work.
2. **Write fixed JSON keys directly.** Schema keys are static ASCII
   identifiers, so omit their escape scan and combine the closing quote
   with `: ` in one write. Private field APIs require static keys. The
   existing separator/indent state transition is shared with custom names,
   which still use the original JSON escaping function. Value escaping and
   all numeric formatting remain unchanged.
3. **Scan CSV bytes directly.** Replace generic character searches with the
   existing dependency's `memchr2`/`memchr`. Retain the first quote/comma
   position, write the known clean prefix once, and search only the suffix
   for quotes to double. Commas, doubled quotes, Unicode and the existing
   newline handling are preserved. No temporary strings are created.

JSON value-scanner trials were discarded after clean-string and small-report
slowdowns. The original `write_json_string` implementation is unchanged.

## Measurements

Rust 1.98.1 / LLVM 22.1.8, Windows, Xeon E5-2696 v4, 44 logical CPUs.
Both builds use release fat LTO and one codegen unit. Each result is the
median of three alternating old/new process pairs, with seven timed batches
per process. Each benchmark process is pinned to CPU 2.

Private production functions and JSON field operations are called directly by the explicitly ignored
`report_hotpath` unit benchmark: a reused 8 KB `BufWriter<Vec<u8>>`, a
pre-sized backing vector, and a flush per invocation. Use 200,000 iterations
for object fields/indentation/16-byte fields and 4,000 for 4 KB fields. These leaf results
measure wall-clock throughput, including flush/clear overhead; they do not
claim CPU-cycle or peak-memory measurements.

The public `write_reports` benchmarks reuse a pre-sized output vector and
exclude fixture parsing from measurement. Use 200 iterations per batch;
unchanged analysis controls use 30 (Camellia internally uses three).
`QueryThreadCycleTime` supplies CPU cycles. Allocations are counted in a
separate invocation. Requested bytes include reallocations and mean churn,
not resident memory.

- Eight-space indentation: 1.44x throughput;
  64 spaces: 6.67x.
- Fixed-key string field `sn_detailed_breakdown`:
  1.39x throughput;
  the corresponding numeric field:
  1.79x.
- A 4 KB CSV field with a late quote: 14.33x;
  the complete CSV report: 4.11x.
- Dense 4 KB escaping: unchanged JSON string control +3.3%,
  complete JSON -2.5%; CSV leaf
  +77.6%, complete CSV
  -2.6%.
- Camellia reports: JSON +12.6%, CSV
  +42.3%. Allocation counts and bytes are unchanged
  in every report and analysis case. This pass improves CPU/throughput,
  not allocation churn.

All cases, including short fields, clean strings and unchanged controls,
are shown below. Small timing differences do not establish a universal
speedup or their cause.

| Direct production function / input | ns old -> new | Throughput |
|---|---:|---:|
| `report_leaf/object/title_false` | 44 -> 35 | +25.7% |
| `report_leaf/object/title_true` | 39 -> 35 | +11.4% |
| `report_leaf/object/sn_detailed_breakdown_false` | 61 -> 44 | +38.6% |
| `report_leaf/object/sn_detailed_breakdown_true` | 52 -> 29 | +79.3% |
| `report_leaf/object/equally_spaced_per_measure_false` | 68 -> 36 | +88.9% |
| `report_leaf/object/equally_spaced_per_measure_true` | 57 -> 31 | +83.9% |
| `report_leaf/indent/0` | 1 -> 1 | +0.0% |
| `report_leaf/indent/2` | 14 -> 16 | -12.5% |
| `report_leaf/indent/8` | 23 -> 16 | +43.8% |
| `report_leaf/indent/16` | 42 -> 14 | +200.0% |
| `report_leaf/indent/64` | 160 -> 24 | +566.7% |
| `report_leaf/indent/129` | 318 -> 34 | +835.3% |
| `report_leaf/json/16_clean` | 31 -> 31 | +0.0% |
| `report_leaf/json/16_early` | 40 -> 39 | +2.6% |
| `report_leaf/json/16_late` | 61 -> 54 | +13.0% |
| `report_leaf/json/16_dense` | 86 -> 81 | +6.2% |
| `report_leaf/json/16_comma` | 33 -> 32 | +3.1% |
| `report_leaf/json/16_comma_quote` | 60 -> 54 | +11.1% |
| `report_leaf/json/4096_clean` | 3,711 -> 3,634 | +2.1% |
| `report_leaf/json/4096_early` | 4,620 -> 4,698 | -1.7% |
| `report_leaf/json/4096_late` | 8,132 -> 8,021 | +1.4% |
| `report_leaf/json/4096_dense` | 18,078 -> 17,495 | +3.3% |
| `report_leaf/json/4096_comma` | 3,640 -> 3,629 | +0.3% |
| `report_leaf/json/4096_comma_quote` | 8,098 -> 8,184 | -1.1% |
| `report_leaf/csv/16_clean` | 39 -> 26 | +50.0% |
| `report_leaf/csv/16_early` | 57 -> 49 | +16.3% |
| `report_leaf/csv/16_late` | 69 -> 43 | +60.5% |
| `report_leaf/csv/16_dense` | 117 -> 87 | +34.5% |
| `report_leaf/csv/16_comma` | 47 -> 34 | +38.2% |
| `report_leaf/csv/16_comma_quote` | 63 -> 46 | +37.0% |
| `report_leaf/csv/4096_clean` | 3,180 -> 241 | +1219.5% |
| `report_leaf/csv/4096_early` | 581 -> 229 | +153.7% |
| `report_leaf/csv/4096_late` | 3,611 -> 252 | +1332.9% |
| `report_leaf/csv/4096_dense` | 21,087 -> 11,873 | +77.6% |
| `report_leaf/csv/4096_comma` | 3,590 -> 243 | +1377.4% |
| `report_leaf/csv/4096_comma_quote` | 2,080 -> 232 | +796.6% |

| Complete path / input | Cycles old -> new | Throughput | Allocs old -> new | Requested bytes old -> new |
|---|---:|---:|---:|---:|
| `report/json/16_clean` | 104,313 -> 85,783 | +21.7% | 18 -> 18 | 318 -> 318 |
| `report/csv/16_clean` | 35,010 -> 33,010 | +6.0% | 0 -> 0 | 0 -> 0 |
| `report/json/16_early` | 113,733 -> 87,804 | +29.6% | 18 -> 18 | 318 -> 318 |
| `report/csv/16_early` | 37,194 -> 33,964 | +9.5% | 0 -> 0 | 0 -> 0 |
| `report/json/16_late` | 119,850 -> 85,045 | +41.4% | 18 -> 18 | 318 -> 318 |
| `report/csv/16_late` | 36,424 -> 34,196 | +6.5% | 0 -> 0 | 0 -> 0 |
| `report/json/16_dense` | 106,776 -> 85,605 | +24.9% | 18 -> 18 | 318 -> 318 |
| `report/csv/16_dense` | 36,356 -> 35,239 | +3.0% | 0 -> 0 | 0 -> 0 |
| `report/json/16_comma` | 106,105 -> 84,718 | +25.2% | 18 -> 18 | 318 -> 318 |
| `report/csv/16_comma` | 39,480 -> 34,044 | +16.4% | 0 -> 0 | 0 -> 0 |
| `report/json/16_comma_quote` | 112,546 -> 86,381 | +30.7% | 18 -> 18 | 318 -> 318 |
| `report/csv/16_comma_quote` | 34,027 -> 34,020 | +0.2% | 0 -> 0 | 0 -> 0 |
| `report/json/16_custom` | 105,663 -> 86,298 | +22.5% | 18 -> 18 | 318 -> 318 |
| `report/csv/16_custom` | 37,315 -> 33,539 | +11.2% | 0 -> 0 | 0 -> 0 |
| `report/json/4096_clean` | 158,146 -> 127,562 | +23.8% | 18 -> 18 | 318 -> 318 |
| `report/csv/4096_clean` | 159,461 -> 42,980 | +271.3% | 0 -> 0 | 0 -> 0 |
| `report/json/4096_early` | 169,308 -> 148,978 | +13.7% | 18 -> 18 | 318 -> 318 |
| `report/csv/4096_early` | 55,977 -> 41,745 | +33.8% | 0 -> 0 | 0 -> 0 |
| `report/json/4096_late` | 213,271 -> 185,575 | +15.1% | 18 -> 18 | 318 -> 318 |
| `report/csv/4096_late` | 178,225 -> 43,315 | +311.0% | 0 -> 0 | 0 -> 0 |
| `report/json/4096_dense` | 324,994 -> 333,031 | -2.5% | 18 -> 18 | 318 -> 318 |
| `report/csv/4096_dense` | 490,770 -> 503,020 | -2.6% | 0 -> 0 | 0 -> 0 |
| `report/json/4096_comma` | 147,497 -> 129,223 | +14.1% | 18 -> 18 | 318 -> 318 |
| `report/csv/4096_comma` | 174,064 -> 43,170 | +303.4% | 0 -> 0 | 0 -> 0 |
| `report/json/4096_comma_quote` | 226,269 -> 188,114 | +20.3% | 18 -> 18 | 318 -> 318 |
| `report/csv/4096_comma_quote` | 125,209 -> 42,713 | +193.5% | 0 -> 0 | 0 -> 0 |
| `report/json/4096_custom` | 152,411 -> 128,829 | +18.4% | 18 -> 18 | 318 -> 318 |
| `report/csv/4096_custom` | 157,836 -> 43,965 | +259.3% | 0 -> 0 | 0 -> 0 |
| `report/json/camellia` | 22,878,245 -> 20,515,615 | +12.6% | 30 -> 30 | 850 -> 850 |
| `report/csv/camellia` | 91,310 -> 64,151 | +42.3% | 0 -> 0 | 0 -> 0 |
| `analyze/fast_fake_lifts` | 633,799 -> 651,286 | -2.7% | 31 -> 31 | 59,028 -> 59,028 |
| `analyze/camellia` | 441,425,257 -> 437,701,070 | +0.9% | 110 -> 110 | 5,263,624 -> 5,263,624 |
| `analyze/fast_camellia` | 60,577,463 -> 56,765,334 | +6.7% | 115 -> 115 | 7,051,152 -> 7,051,152 |
| `analyze/mixed_small` | 51,041 -> 48,700 | +5.2% | 59 -> 59 | 7,460 -> 7,460 |


## Validation

- Release unit tests: 206 passed, with the performance test intentionally
  ignored by default. Focused optimization regression suite: 24 passed.
  Eight new tests call the production writers and cover all JSON ASCII
  controls, Unicode, early/late quotes, commas, quote doubling, existing
  CSV newline behavior, indentation across chunk boundaries, short writes
  and errors at every output byte. The first error and written prefix are
  checked explicitly; CSV stops attempting writes after an error.
  Fixed schema keys and dynamic custom names containing Unicode, quotes,
  backslashes, newlines and other controls are checked separately.
- Strict release Clippy across all workspace targets, formatting and diff
  checks passed.
- After confirming the final optimizations, the exact command
  `cargo test --release --test all_parity -- --test-threads=22` passed
  all 30,489 tests with zero failures before commit.
- Original/final deterministic output is byte-identical across 30,843
  corpus files, 56,125 supported charts and representative component
  fixtures, including 354 matching malformed-file errors. New components
  compare complete JSON and CSV bytes for clean, early/late/dense escaped
  metadata, commas, comma-before-quote cases and dynamic custom keys at
  16 and 4,096 bytes.
  The corpus fields check densities, spacing, stream strings, normalized
  maps, exact duration/peak bits and BPM snapshots; they do not serialize
  reports for every corpus file. Full parity checks JSON analysis fields
  against golden data. SHA-256 of 167,714,804
  transcript bytes: `1c9330ce908bd078713e45d7fe8a36ea13cacdf703e0a311c38713ddfc9be488`.

## Reproduction

Build `983d6a3` with only version 0.4.279 and the final benchmark/test harness,
save both executables, then build the final production code with that same
version/harness. Alternate the saved executables three times.

```powershell
cargo test --release -p rssp --lib --no-run
$env:RSSP_REPORT_FILTER = 'object/' # also indent/, json/16_, json/4096_, csv/16_, csv/4096_
$env:RSSP_REPORT_ITERS = '200000' # 4000 for 4 KB fields
cargo test --release -p rssp --lib report::perf::report_hotpath -- --ignored --nocapture --test-threads=1

cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_HOT_FILTER = 'report/' # or analyze/
$env:RSSP_HOT_ITERS = '200' # 30 for analyze/
$bench = Get-ChildItem target/release/deps/hotpath_perf-*.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $bench.FullName
```

Pin the leaf benchmark process to CPU 2 for the recorded configuration.
`RSSP_HOT_VERIFY=1` emits the deterministic component/corpus transcript.

# Performance pass 0.4.280

Baseline: `a0454cf` (0.4.279), compiled at version 0.4.280 with the same
benchmark harness as the final implementation. Patch version advances once.
Reviewed `rust-performance.md`; it and `optimize.sh` / `optimize.ps1` remain
outside the commit.

## Changes

1. **Strip title tags in the existing string.** The stripping function returns
   a suffix, so drain the prefix rather than compare strings, allocate a copy,
   and drop the original buffer. Cleanup, trimming and marker translation
   retain their original order. Nonempty stripped titles avoid one allocation.
   The title retains its original capacity; requested allocation bytes below
   measure churn, not retained capacity or peak RSS.
2. **Copy the clean escape prefix in bulk.** Reuse the first backslash position
   from `memchr`, copy the UTF-8 prefix once, and run the original character
   unescaping loop only on the remaining suffix. Clean values still borrow;
   escaped values retain the original capacity and trailing-backslash behavior.
   CP1252 decoding and owned-buffer unescaping are unchanged.
3. **Reuse the marker scan position.** Find the first ampersand once and start
   the existing in-place translation scan there. This removes a repeated
   search through the prefix. Compaction, numeric/alias handling, unknown
   markers and the caller's allocation remain unchanged.

No production API, helper, cache, dependency, unsafe block or inline attribute
is added. The three production edits add two net lines while eliminating the
redundant allocation/copy and prefix scans.

## Measurements

Rust 1.98.1 / LLVM 22.1.8, Windows, Xeon E5-2696 v4, 44 logical CPUs.
Both executables use fat LTO and one codegen unit, and pin the measured thread
to CPU 2. Each entry is the median of three alternating original/final process
pairs, each with seven batches. No rssp build, test or corpus comparison ran
during the timed measurements.

Leaf cases use 10,000 invocations per batch. The in-place marker benchmark
prepares independent owned strings before starting the timer and drops them
after stopping it; preparation is excluded from the separate allocation
count. The owned marker API measures its required copy as part of that API.
Unescaping and UTF-8 decode/unescape are measured separately. Fixtures use
16-byte and 4 KB prefixes with clean, early/late/dense, Unicode, unknown,
nested and trailing delimiter variants.

Composed metadata cases call production `analyze_with_scratch` on a small
single-chart fixture, with reusable scratch and 1,000 invocations per batch.
Controls use 30 iterations (Camellia internally uses three).
`QueryThreadCycleTime` supplies CPU cycles; allocations/reallocations and
requested bytes are counted in a separate invocation.

- Tagged 4 KB title: 1.06x throughput,
  with one fewer allocation.
- Late escape after 4 KB: 34.66x throughput;
  complete analysis: 1.63x.
- Late marker after 4 KB: 1.44x in-place throughput;
  owned API: 2.61x;
  complete analysis: 1.47x.

The tables include unchanged paths and all measured cases, including any
small timing variation. Allocation counts are deterministic; timings are
specific to this host and workload.

Limits: several short leaf cases cost 2-6 ns more; the in-place 4 KB early
marker case is 10.9% slower, while its owned API is 8.8% faster and complete
analysis is 1.7% faster. Dense 4 KB owned marker translation is 1.4% slower.
All 36 composed metadata cases and the four analysis controls improve in
the retained measurements. A larger translation-loop rewrite was rejected
after repeated 5.7-9.3% slowdowns on analysis controls; it is absent from the
final source. Leaf timings should not be substituted for composed results.

## Leaf functions

| Case | CPU cycles old -> new | Throughput change | Allocations old -> new | Reallocations old -> new | Requested bytes old -> new |
|---|---:|---:|---:|---:|---:|
| `unescape/16_clean` | 17 -> 21 | -20.0% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `unescape/16_early` | 237 -> 246 | -3.6% | 1 -> 1 | 0 -> 0 | 18 -> 18 |
| `unescape/16_late` | 244 -> 183 | +33.3% | 1 -> 1 | 0 -> 0 | 18 -> 18 |
| `unescape/16_dense` | 191 -> 198 | -3.3% | 1 -> 1 | 0 -> 0 | 16 -> 16 |
| `unescape/16_unicode` | 226 -> 185 | +21.2% | 1 -> 1 | 0 -> 0 | 22 -> 22 |
| `unescape/16_trailing` | 230 -> 184 | +25.0% | 1 -> 1 | 0 -> 0 | 17 -> 17 |
| `unescape/4096_clean` | 841 -> 115 | +638.5% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `unescape/4096_early` | 16,426 -> 16,417 | +0.0% | 1 -> 1 | 0 -> 0 | 4,098 -> 4,098 |
| `unescape/4096_late` | 17,225 -> 498 | +3365.6% | 1 -> 1 | 0 -> 0 | 4,098 -> 4,098 |
| `unescape/4096_dense` | 9,098 -> 9,114 | -0.1% | 1 -> 1 | 0 -> 0 | 4,096 -> 4,096 |
| `unescape/4096_unicode` | 16,235 -> 481 | +3267.3% | 1 -> 1 | 0 -> 0 | 4,102 -> 4,102 |
| `unescape/4096_trailing` | 17,255 -> 493 | +3400.0% | 1 -> 1 | 0 -> 0 | 4,097 -> 4,097 |
| `decode_escape/16_clean` | 54 -> 59 | -7.4% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `decode_escape/16_early` | 282 -> 286 | -1.5% | 1 -> 1 | 0 -> 0 | 18 -> 18 |
| `decode_escape/16_late` | 285 -> 218 | +30.0% | 1 -> 1 | 0 -> 0 | 18 -> 18 |
| `decode_escape/16_dense` | 229 -> 237 | -2.8% | 1 -> 1 | 0 -> 0 | 16 -> 16 |
| `decode_escape/16_unicode` | 298 -> 254 | +17.2% | 1 -> 1 | 0 -> 0 | 22 -> 22 |
| `decode_escape/16_trailing` | 264 -> 222 | +18.8% | 1 -> 1 | 0 -> 0 | 17 -> 17 |
| `decode_escape/4096_clean` | 1,183 -> 451 | +162.6% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `decode_escape/4096_early` | 16,798 -> 16,757 | +0.3% | 1 -> 1 | 0 -> 0 | 4,098 -> 4,098 |
| `decode_escape/4096_late` | 17,601 -> 847 | +1975.7% | 1 -> 1 | 0 -> 0 | 4,098 -> 4,098 |
| `decode_escape/4096_dense` | 9,438 -> 9,441 | -0.0% | 1 -> 1 | 0 -> 0 | 4,096 -> 4,096 |
| `decode_escape/4096_unicode` | 22,839 -> 7,078 | +222.6% | 1 -> 1 | 0 -> 0 | 4,102 -> 4,102 |
| `decode_escape/4096_trailing` | 17,573 -> 840 | +1993.5% | 1 -> 1 | 0 -> 0 | 4,097 -> 4,097 |
| `markers/16_clean` | 17 -> 22 | -20.0% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/16_early` | 94 -> 100 | -6.5% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/16_late` | 86 -> 93 | -9.3% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/16_dense` | 207 -> 220 | -6.0% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/16_unknown` | 108 -> 113 | -5.8% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/16_nested` | 123 -> 131 | -6.7% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/16_unicode` | 104 -> 107 | -2.0% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/16_trailing` | 37 -> 43 | -15.0% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_clean` | 1,418 -> 1,106 | +27.9% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_early` | 1,638 -> 1,845 | -10.9% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_late` | 1,607 -> 1,121 | +43.6% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_dense` | 55,441 -> 55,075 | +0.8% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_unknown` | 1,720 -> 1,270 | +35.5% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_nested` | 1,698 -> 1,401 | +20.9% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_unicode` | 1,736 -> 1,223 | +41.9% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers/4096_trailing` | 1,575 -> 1,213 | +30.1% | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `markers_owned/16_clean` | 168 -> 178 | -4.9% | 1 -> 1 | 0 -> 0 | 16 -> 16 |
| `markers_owned/16_early` | 278 -> 280 | -0.8% | 1 -> 1 | 0 -> 0 | 21 -> 21 |
| `markers_owned/16_late` | 259 -> 253 | +2.6% | 1 -> 1 | 0 -> 0 | 21 -> 21 |
| `markers_owned/16_dense` | 366 -> 375 | -2.3% | 1 -> 1 | 0 -> 0 | 15 -> 15 |
| `markers_owned/16_unknown` | 280 -> 282 | -0.8% | 1 -> 1 | 0 -> 0 | 25 -> 25 |
| `markers_owned/16_nested` | 301 -> 300 | +0.7% | 1 -> 1 | 0 -> 0 | 24 -> 24 |
| `markers_owned/16_unicode` | 271 -> 269 | +0.8% | 1 -> 1 | 0 -> 0 | 27 -> 27 |
| `markers_owned/16_trailing` | 205 -> 205 | +1.1% | 1 -> 1 | 0 -> 0 | 17 -> 17 |
| `markers_owned/4096_clean` | 1,185 -> 425 | +178.9% | 1 -> 1 | 0 -> 0 | 4,096 -> 4,096 |
| `markers_owned/4096_early` | 753 -> 692 | +8.9% | 1 -> 1 | 0 -> 0 | 4,101 -> 4,101 |
| `markers_owned/4096_late` | 1,390 -> 533 | +161.3% | 1 -> 1 | 0 -> 0 | 4,101 -> 4,101 |
| `markers_owned/4096_dense` | 54,185 -> 54,947 | -1.4% | 1 -> 1 | 0 -> 0 | 4,095 -> 4,095 |
| `markers_owned/4096_unknown` | 1,394 -> 602 | +132.1% | 1 -> 1 | 0 -> 0 | 4,105 -> 4,105 |
| `markers_owned/4096_nested` | 1,432 -> 617 | +131.9% | 1 -> 1 | 0 -> 0 | 4,104 -> 4,104 |
| `markers_owned/4096_unicode` | 1,432 -> 579 | +147.7% | 1 -> 1 | 0 -> 0 | 4,107 -> 4,107 |
| `markers_owned/4096_trailing` | 1,322 -> 512 | +157.7% | 1 -> 1 | 0 -> 0 | 4,097 -> 4,097 |

## Composed metadata

| Case | CPU cycles old -> new | Throughput change | Allocations old -> new | Reallocations old -> new | Requested bytes old -> new |
|---|---:|---:|---:|---:|---:|
| `metadata/title_16_clean` | 12,949 -> 12,251 | +5.8% | 21 -> 21 | 2 -> 2 | 2,424 -> 2,424 |
| `metadata/title_16_tagged` | 13,168 -> 12,725 | +3.4% | 22 -> 21 | 2 -> 2 | 2,460 -> 2,443 |
| `metadata/title_16_spaces` | 12,942 -> 12,259 | +5.5% | 22 -> 21 | 2 -> 2 | 2,449 -> 2,430 |
| `metadata/title_16_unicode` | 12,980 -> 12,426 | +4.3% | 22 -> 21 | 2 -> 2 | 2,451 -> 2,435 |
| `metadata/escape_16_clean` | 12,532 -> 11,844 | +5.8% | 21 -> 21 | 2 -> 2 | 2,428 -> 2,428 |
| `metadata/escape_16_early` | 12,479 -> 12,194 | +2.4% | 21 -> 21 | 2 -> 2 | 2,430 -> 2,430 |
| `metadata/escape_16_late` | 12,595 -> 11,891 | +5.9% | 21 -> 21 | 2 -> 2 | 2,430 -> 2,430 |
| `metadata/escape_16_dense` | 12,622 -> 11,897 | +6.1% | 21 -> 21 | 2 -> 2 | 2,428 -> 2,428 |
| `metadata/escape_16_unicode` | 11,302 -> 10,834 | +4.2% | 21 -> 21 | 2 -> 2 | 2,434 -> 2,434 |
| `metadata/escape_16_trailing` | 12,669 -> 11,978 | +5.7% | 21 -> 21 | 2 -> 2 | 2,429 -> 2,429 |
| `metadata/marker_16_clean` | 12,719 -> 11,939 | +6.4% | 21 -> 21 | 2 -> 2 | 2,424 -> 2,424 |
| `metadata/marker_16_early` | 11,744 -> 11,076 | +6.0% | 21 -> 21 | 2 -> 2 | 2,430 -> 2,430 |
| `metadata/marker_16_late` | 11,825 -> 11,036 | +7.0% | 21 -> 21 | 2 -> 2 | 2,430 -> 2,430 |
| `metadata/marker_16_dense` | 11,938 -> 11,236 | +6.3% | 21 -> 21 | 2 -> 2 | 2,426 -> 2,426 |
| `metadata/marker_16_unknown` | 11,880 -> 11,197 | +6.1% | 21 -> 21 | 2 -> 2 | 2,434 -> 2,434 |
| `metadata/marker_16_nested` | 11,905 -> 10,742 | +10.9% | 21 -> 21 | 2 -> 2 | 2,433 -> 2,433 |
| `metadata/marker_16_unicode` | 12,069 -> 10,779 | +12.0% | 21 -> 21 | 2 -> 2 | 2,436 -> 2,436 |
| `metadata/marker_16_trailing` | 11,341 -> 10,785 | +5.2% | 21 -> 21 | 2 -> 2 | 2,425 -> 2,425 |
| `metadata/title_4096_clean` | 33,390 -> 31,702 | +5.3% | 21 -> 21 | 2 -> 2 | 6,504 -> 6,504 |
| `metadata/title_4096_tagged` | 34,350 -> 32,278 | +6.4% | 22 -> 21 | 2 -> 2 | 10,620 -> 6,523 |
| `metadata/title_4096_spaces` | 36,159 -> 34,691 | +4.3% | 22 -> 21 | 2 -> 2 | 10,609 -> 6,510 |
| `metadata/title_4096_unicode` | 38,897 -> 37,266 | +4.3% | 22 -> 21 | 2 -> 2 | 10,611 -> 6,515 |
| `metadata/escape_4096_clean` | 29,283 -> 28,035 | +4.6% | 21 -> 21 | 2 -> 2 | 6,508 -> 6,508 |
| `metadata/escape_4096_early` | 44,358 -> 43,955 | +0.8% | 21 -> 21 | 2 -> 2 | 6,510 -> 6,510 |
| `metadata/escape_4096_late` | 45,974 -> 28,125 | +63.4% | 21 -> 21 | 2 -> 2 | 6,510 -> 6,510 |
| `metadata/escape_4096_dense` | 29,275 -> 28,493 | +2.6% | 21 -> 21 | 2 -> 2 | 6,508 -> 6,508 |
| `metadata/escape_4096_unicode` | 49,457 -> 33,250 | +48.8% | 21 -> 21 | 2 -> 2 | 6,514 -> 6,514 |
| `metadata/escape_4096_trailing` | 45,776 -> 28,108 | +62.8% | 21 -> 21 | 2 -> 2 | 6,509 -> 6,509 |
| `metadata/marker_4096_clean` | 30,455 -> 28,222 | +7.9% | 21 -> 21 | 2 -> 2 | 6,504 -> 6,504 |
| `metadata/marker_4096_early` | 55,467 -> 54,553 | +1.7% | 21 -> 21 | 2 -> 2 | 6,510 -> 6,510 |
| `metadata/marker_4096_late` | 56,933 -> 38,662 | +47.3% | 21 -> 21 | 2 -> 2 | 6,510 -> 6,510 |
| `metadata/marker_4096_dense` | 149,321 -> 148,287 | +0.6% | 21 -> 21 | 2 -> 2 | 7,322 -> 7,322 |
| `metadata/marker_4096_unknown` | 47,523 -> 32,241 | +47.4% | 21 -> 21 | 2 -> 2 | 6,514 -> 6,514 |
| `metadata/marker_4096_nested` | 47,781 -> 31,985 | +49.5% | 21 -> 21 | 2 -> 2 | 6,513 -> 6,513 |
| `metadata/marker_4096_unicode` | 60,996 -> 43,568 | +40.0% | 21 -> 21 | 2 -> 2 | 6,516 -> 6,516 |
| `metadata/marker_4096_trailing` | 31,091 -> 28,623 | +8.6% | 21 -> 21 | 2 -> 2 | 6,505 -> 6,505 |

## Analysis controls

| Case | CPU cycles old -> new | Throughput change | Allocations old -> new | Reallocations old -> new | Requested bytes old -> new |
|---|---:|---:|---:|---:|---:|
| `analyze/fast_fake_lifts` | 667,565 -> 655,551 | +1.9% | 31 -> 31 | 4 -> 4 | 59,028 -> 59,028 |
| `analyze/camellia` | 443,543,648 -> 443,397,609 | +0.1% | 110 -> 110 | 0 -> 0 | 5,263,624 -> 5,263,624 |
| `analyze/fast_camellia` | 57,209,822 -> 56,550,735 | +1.0% | 115 -> 115 | 0 -> 0 | 7,051,152 -> 7,051,152 |
| `analyze/mixed_small` | 50,712 -> 49,234 | +3.6% | 59 -> 59 | 3 -> 3 | 7,460 -> 7,460 |


## Regression checks

- The three new edge tests pass on both the original and final code; 27 edge
  tests and 206 library tests pass in release mode.
- Full original/final component and corpus transcripts match byte-for-byte:
  167,859,340 UTF-8 bytes, SHA-256
  `88893da2c498a787c62ec3f80ea9e53999a87562e3f499ae3c28349f08a53887`.
  The corpus covers 30,843 files, 56,125 supported charts, 30,489 successful
  parses and 354 matching parse failures. Component checks include complete
  JSON/CSV/SM/SSC output, all metadata option combinations, escape outputs,
  and marker buffer pointer/capacity preservation.
- After confirming the optimizations:
  `cargo test --release --test all_parity -- --test-threads=22`:
  **30,489 passed, zero failures**.
- Strict release workspace/all-target Clippy, formatting and diff checks pass.

## Reproduction

Before editing production code, bump only the workspace version to 0.4.280,
apply the final benchmark harness/tests, and save the original executable:

```powershell
cargo bench -p rssp --bench hotpath_perf --no-run
```

After applying the three production edits, build and save the final executable
with the same command/profile. Run each filter three times per executable,
alternating old/new then new/old then old/new:

```powershell
$env:RSSP_HOT_ITERS='10000'
$env:RSSP_HOT_FILTER='unescape/' # also decode_escape/, markers/, markers_owned/
& $exe
$env:RSSP_HOT_ITERS='1000'
$env:RSSP_HOT_FILTER='metadata/'
& $exe
$env:RSSP_HOT_ITERS='30'
$env:RSSP_HOT_FILTER='analyze/'
& $exe
Remove-Item Env:RSSP_HOT_FILTER
$env:RSSP_HOT_VERIFY='1'
& $exe # redirect UTF-8 bytes via Python subprocess for an exact comparison
cargo test --release --workspace --lib
cargo test --release -p rssp --test optimization_edges
cargo clippy --release --workspace --all-targets -- -D warnings
cargo test --release --test all_parity -- --test-threads=22
cargo fmt --all -- --check
git diff --check
```


# Performance pass 0.4.281

Baseline: `b6a8126` (0.4.280), compiled at version 0.4.281 with the
same benchmark harness as the final implementation. The patch advances once.
Reviewed `rust-performance.md`; it, `optimize.ps1` and `optimize.sh` are
excluded from the commit.

## Changes

1. **Skip descriptions that legacy SSC discards.** Hash, duration and peak-NPS
   utility APIs previously decoded, unescaped and trimmed a description before
   replacing it with an empty string for SSC versions below 0.74.
   `decode_chart_desc` checks the version first and returns a borrowed empty
   string. Modern SSC, missing/NaN versions and SM retain the existing decoder.
   Full analysis still decodes legacy descriptions used as chart names.
2. **Own only labels that survive merging.** `parse_labels` inserts borrowed
   `Cow<str>` values into the existing ordering and neighbor-merge algorithm.
   It converts surviving labels to owned strings at the return boundary.
   Equal-sized tuples permit the iterator collection to reuse the vector's
   allocation, confirmed by counts for distinct-label controls. The public
   snapshot type, retained vector capacity and label semantics stay unchanged.
   Whole-segment trimming and its empty check are removed: trimming the two
   fields already covers whitespace, and segments without '=' are skipped.
3. **Skip the guaranteed BPM-buffer overflow pass.** Quantized beats and
   round-tripped BPMs are finite, each requiring at least eight characters in
   the existing fixed-six-decimal formatter. Including '=' and commas, a map
   needs at least `18 * entries - 1` bytes. Above `16384 / 18` (910 entries),
   bypass the first formatting attempt and use the existing streaming path.
   Smaller maps keep their original buffering and overflow fallback.

The edits remove discarded decoding, transient label copies and a failed
formatting pass. No timing math, cache, dependency or production unsafe code
is added. One shared decoding function has three runtime callers.

## Measurements

Rust 1.98.1 / LLVM 22.1.8, Windows, Xeon E5-2696 v4, 44 logical CPUs.
Fat LTO and one codegen unit; measured thread/process pinned to CPU 2.
Each value is the median of three alternating original/final process pairs,
with seven batches per process. No rssp build, regression test or corpus
comparison ran during the timed measurements.

Fixtures are prepared outside measurement. Description cases use four small
charts, 16-byte/4 KB descriptions and plain, escaped or CP1252 bytes. They
measure all three changed utility APIs directly, including unchanged SM and
modern-SSC controls (1,000 calls per batch).

Label leaf cases call the private production parser; composed cases call
`build_timing_snapshot` (10,000 and 1,000 calls respectively). They include
distinct, repeated, same-row replacement, grouped, 1 KB and invalid labels.
BPM leaf cases call the production JSON formatter using both a reused Vec
and an 8 KB BufWriter; composed cases write the complete JSON report.
Leaf iterations are 200,000 for 0/1 entries, 10,000 for 32, 1,000 for
512/1,024 and 500 for 2,048; complete reports use 200.
Analysis controls use 30 iterations (Camellia internally uses three).
Seven short/control cases are also measured in fresh processes with 10,000
iterations; both the initial and focused results are retained.

`QueryThreadCycleTime` measures CPU cycles for the composed cases.
Allocation/reallocation counts and requested bytes come from a separate
invocation. Requested bytes represent allocation churn, not peak RSS.

- Legacy 4 KB CP1252 descriptions, batch hashes:
  3.63x throughput;
  allocations 20 -> 16,
  requested bytes 35,571 -> 2,787.
- 256 repeated labels: parser
  2.33x, complete snapshot
  2.01x;
  allocations 264 -> 9.
  With 1 KB repeated labels, requested bytes
  393,496 -> 132,376.
- 1,024 BPM entries: buffered leaf
  1.84x, complete report
  1.43x.
  2,048 entries: buffered leaf
  1.40x, complete report
  1.18x.

All measured cases follow, including unchanged paths and slower samples.

### Description utility APIs

| Case | ns old -> new | cycles old -> new | allocs old -> new | reallocs old -> new | bytes old -> new | throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `description/hash/0.6_16_plain` | 5,962 -> 5,647 | 13,021 -> 12,344 | 16 -> 16 | 2 -> 2 | 2,787 -> 2,787 | 1.06x |
| `description/duration/0.6_16_plain` | 4,680 -> 4,613 | 10,207 -> 10,069 | 13 -> 13 | 0 -> 0 | 2,408 -> 2,408 | 1.01x |
| `description/peak/0.6_16_plain` | 4,797 -> 4,677 | 10,497 -> 10,208 | 14 -> 14 | 0 -> 0 | 2,424 -> 2,424 | 1.03x |
| `description/hash/0.6_16_escaped` | 6,174 -> 5,548 | 13,510 -> 12,114 | 20 -> 16 | 2 -> 2 | 2,867 -> 2,787 | 1.11x |
| `description/duration/0.6_16_escaped` | 5,204 -> 4,683 | 11,344 -> 10,258 | 17 -> 13 | 0 -> 0 | 2,488 -> 2,408 | 1.11x |
| `description/peak/0.6_16_escaped` | 5,391 -> 4,665 | 11,790 -> 10,188 | 18 -> 14 | 0 -> 0 | 2,504 -> 2,424 | 1.16x |
| `description/hash/0.6_16_cp1252` | 6,566 -> 5,645 | 14,344 -> 12,367 | 20 -> 16 | 2 -> 2 | 2,931 -> 2,787 | 1.16x |
| `description/duration/0.6_16_cp1252` | 5,334 -> 4,522 | 11,652 -> 9,869 | 17 -> 13 | 0 -> 0 | 2,552 -> 2,408 | 1.18x |
| `description/peak/0.6_16_cp1252` | 5,592 -> 4,668 | 12,207 -> 10,224 | 18 -> 14 | 0 -> 0 | 2,568 -> 2,424 | 1.20x |
| `description/hash/0.74_16_plain` | 5,858 -> 5,950 | 12,794 -> 12,995 | 16 -> 16 | 2 -> 2 | 2,787 -> 2,787 | 0.98x |
| `description/duration/0.74_16_plain` | 4,798 -> 4,746 | 10,504 -> 10,363 | 13 -> 13 | 0 -> 0 | 2,408 -> 2,408 | 1.01x |
| `description/peak/0.74_16_plain` | 4,946 -> 4,881 | 10,780 -> 10,658 | 14 -> 14 | 0 -> 0 | 2,424 -> 2,424 | 1.01x |
| `description/hash/0.74_16_escaped` | 6,222 -> 6,311 | 13,605 -> 13,808 | 20 -> 20 | 2 -> 2 | 2,867 -> 2,867 | 0.99x |
| `description/duration/0.74_16_escaped` | 5,197 -> 5,358 | 11,365 -> 11,749 | 17 -> 17 | 0 -> 0 | 2,488 -> 2,488 | 0.97x |
| `description/peak/0.74_16_escaped` | 5,604 -> 5,363 | 12,248 -> 11,723 | 18 -> 18 | 0 -> 0 | 2,504 -> 2,504 | 1.04x |
| `description/hash/0.74_16_cp1252` | 6,443 -> 6,437 | 14,089 -> 14,076 | 20 -> 20 | 2 -> 2 | 2,931 -> 2,931 | 1.00x |
| `description/duration/0.74_16_cp1252` | 5,463 -> 5,508 | 11,943 -> 12,037 | 17 -> 17 | 0 -> 0 | 2,552 -> 2,552 | 0.99x |
| `description/peak/0.74_16_cp1252` | 5,508 -> 5,557 | 12,038 -> 12,141 | 18 -> 18 | 0 -> 0 | 2,568 -> 2,568 | 0.99x |
| `description/hash/NaN_16_plain` | 5,925 -> 5,872 | 12,940 -> 12,838 | 16 -> 16 | 2 -> 2 | 2,787 -> 2,787 | 1.01x |
| `description/duration/NaN_16_plain` | 4,870 -> 4,901 | 10,635 -> 10,702 | 13 -> 13 | 0 -> 0 | 2,408 -> 2,408 | 0.99x |
| `description/peak/NaN_16_plain` | 4,968 -> 5,116 | 10,846 -> 11,177 | 14 -> 14 | 0 -> 0 | 2,424 -> 2,424 | 0.97x |
| `description/hash/NaN_16_escaped` | 6,250 -> 6,647 | 13,677 -> 14,535 | 20 -> 20 | 2 -> 2 | 2,867 -> 2,867 | 0.94x |
| `description/duration/NaN_16_escaped` | 5,152 -> 5,283 | 11,262 -> 11,542 | 17 -> 17 | 0 -> 0 | 2,488 -> 2,488 | 0.98x |
| `description/peak/NaN_16_escaped` | 5,338 -> 5,344 | 11,667 -> 11,710 | 18 -> 18 | 0 -> 0 | 2,504 -> 2,504 | 1.00x |
| `description/hash/NaN_16_cp1252` | 6,612 -> 6,422 | 14,460 -> 14,037 | 20 -> 20 | 2 -> 2 | 2,931 -> 2,931 | 1.03x |
| `description/duration/NaN_16_cp1252` | 5,500 -> 5,384 | 12,008 -> 11,715 | 17 -> 17 | 0 -> 0 | 2,552 -> 2,552 | 1.02x |
| `description/peak/NaN_16_cp1252` | 5,368 -> 5,556 | 11,755 -> 12,166 | 18 -> 18 | 0 -> 0 | 2,568 -> 2,568 | 0.97x |
| `description/hash/sm_16_plain` | 4,794 -> 4,759 | 10,488 -> 10,390 | 16 -> 16 | 2 -> 2 | 3,307 -> 3,307 | 1.01x |
| `description/duration/sm_16_plain` | 3,715 -> 3,755 | 8,097 -> 8,193 | 15 -> 15 | 0 -> 0 | 2,952 -> 2,952 | 0.99x |
| `description/peak/sm_16_plain` | 3,765 -> 4,063 | 8,254 -> 8,899 | 16 -> 16 | 0 -> 0 | 2,968 -> 2,968 | 0.93x |
| `description/hash/sm_16_escaped` | 5,094 -> 5,140 | 11,119 -> 11,225 | 20 -> 20 | 2 -> 2 | 3,387 -> 3,387 | 0.99x |
| `description/duration/sm_16_escaped` | 4,341 -> 4,316 | 9,452 -> 9,431 | 19 -> 19 | 0 -> 0 | 3,032 -> 3,032 | 1.01x |
| `description/peak/sm_16_escaped` | 4,315 -> 4,356 | 9,461 -> 9,547 | 20 -> 20 | 0 -> 0 | 3,048 -> 3,048 | 0.99x |
| `description/hash/sm_16_cp1252` | 5,546 -> 5,460 | 12,090 -> 11,919 | 20 -> 20 | 2 -> 2 | 3,451 -> 3,451 | 1.02x |
| `description/duration/sm_16_cp1252` | 4,411 -> 4,648 | 9,671 -> 10,138 | 19 -> 19 | 0 -> 0 | 3,096 -> 3,096 | 0.95x |
| `description/peak/sm_16_cp1252` | 4,610 -> 4,474 | 10,100 -> 9,806 | 20 -> 20 | 0 -> 0 | 3,112 -> 3,112 | 1.03x |
| `description/hash/0.6_4096_plain` | 25,974 -> 24,102 | 56,796 -> 52,734 | 16 -> 16 | 2 -> 2 | 2,787 -> 2,787 | 1.08x |
| `description/duration/0.6_4096_plain` | 24,201 -> 22,844 | 52,899 -> 49,933 | 13 -> 13 | 0 -> 0 | 2,408 -> 2,408 | 1.06x |
| `description/peak/0.6_4096_plain` | 23,803 -> 23,274 | 52,038 -> 50,891 | 14 -> 14 | 0 -> 0 | 2,424 -> 2,424 | 1.02x |
| `description/hash/0.6_4096_escaped` | 54,780 -> 36,944 | 119,853 -> 80,788 | 20 -> 16 | 2 -> 2 | 19,187 -> 2,787 | 1.48x |
| `description/duration/0.6_4096_escaped` | 51,397 -> 35,883 | 112,482 -> 78,505 | 17 -> 13 | 0 -> 0 | 18,808 -> 2,408 | 1.43x |
| `description/peak/0.6_4096_escaped` | 51,590 -> 35,477 | 112,707 -> 77,581 | 18 -> 14 | 0 -> 0 | 18,824 -> 2,424 | 1.45x |
| `description/hash/0.6_4096_cp1252` | 87,400 -> 24,101 | 190,990 -> 52,235 | 20 -> 16 | 2 -> 2 | 35,571 -> 2,787 | 3.63x |
| `description/duration/0.6_4096_cp1252` | 82,488 -> 22,897 | 180,442 -> 50,098 | 17 -> 13 | 0 -> 0 | 35,192 -> 2,408 | 3.60x |
| `description/peak/0.6_4096_cp1252` | 83,610 -> 22,767 | 182,903 -> 49,747 | 18 -> 14 | 0 -> 0 | 35,208 -> 2,424 | 3.67x |
| `description/hash/0.74_4096_plain` | 25,616 -> 25,060 | 55,973 -> 54,817 | 16 -> 16 | 2 -> 2 | 2,787 -> 2,787 | 1.02x |
| `description/duration/0.74_4096_plain` | 24,728 -> 24,186 | 54,042 -> 52,907 | 13 -> 13 | 0 -> 0 | 2,408 -> 2,408 | 1.02x |
| `description/peak/0.74_4096_plain` | 23,872 -> 23,996 | 52,230 -> 52,488 | 14 -> 14 | 0 -> 0 | 2,424 -> 2,424 | 0.99x |
| `description/hash/0.74_4096_escaped` | 55,332 -> 56,067 | 120,997 -> 122,667 | 20 -> 20 | 2 -> 2 | 19,187 -> 19,187 | 0.99x |
| `description/duration/0.74_4096_escaped` | 52,335 -> 55,114 | 114,426 -> 120,535 | 17 -> 17 | 0 -> 0 | 18,808 -> 18,808 | 0.95x |
| `description/peak/0.74_4096_escaped` | 51,575 -> 54,804 | 112,686 -> 119,820 | 18 -> 18 | 0 -> 0 | 18,824 -> 18,824 | 0.94x |
| `description/hash/0.74_4096_cp1252` | 85,309 -> 84,728 | 186,563 -> 185,329 | 20 -> 20 | 2 -> 2 | 35,571 -> 35,571 | 1.01x |
| `description/duration/0.74_4096_cp1252` | 83,356 -> 84,153 | 182,315 -> 183,930 | 17 -> 17 | 0 -> 0 | 35,192 -> 35,192 | 0.99x |
| `description/peak/0.74_4096_cp1252` | 85,262 -> 83,225 | 186,546 -> 182,101 | 18 -> 18 | 0 -> 0 | 35,208 -> 35,208 | 1.02x |
| `description/hash/NaN_4096_plain` | 25,889 -> 25,412 | 56,627 -> 55,541 | 16 -> 16 | 2 -> 2 | 2,787 -> 2,787 | 1.02x |
| `description/duration/NaN_4096_plain` | 24,515 -> 24,379 | 53,536 -> 53,352 | 13 -> 13 | 0 -> 0 | 2,408 -> 2,408 | 1.01x |
| `description/peak/NaN_4096_plain` | 24,311 -> 24,501 | 53,184 -> 53,561 | 14 -> 14 | 0 -> 0 | 2,424 -> 2,424 | 0.99x |
| `description/hash/NaN_4096_escaped` | 54,951 -> 54,697 | 120,142 -> 119,592 | 20 -> 20 | 2 -> 2 | 19,187 -> 19,187 | 1.00x |
| `description/duration/NaN_4096_escaped` | 52,885 -> 54,558 | 115,663 -> 119,394 | 17 -> 17 | 0 -> 0 | 18,808 -> 18,808 | 0.97x |
| `description/peak/NaN_4096_escaped` | 52,622 -> 52,459 | 115,064 -> 114,766 | 18 -> 18 | 0 -> 0 | 18,824 -> 18,824 | 1.00x |
| `description/hash/NaN_4096_cp1252` | 84,132 -> 84,858 | 183,997 -> 185,476 | 20 -> 20 | 2 -> 2 | 35,571 -> 35,571 | 0.99x |
| `description/duration/NaN_4096_cp1252` | 86,162 -> 83,554 | 188,389 -> 182,798 | 17 -> 17 | 0 -> 0 | 35,192 -> 35,192 | 1.03x |
| `description/peak/NaN_4096_cp1252` | 84,538 -> 84,138 | 184,885 -> 184,102 | 18 -> 18 | 0 -> 0 | 35,208 -> 35,208 | 1.00x |
| `description/hash/sm_4096_plain` | 26,521 -> 24,887 | 57,998 -> 54,431 | 16 -> 16 | 2 -> 2 | 3,307 -> 3,307 | 1.07x |
| `description/duration/sm_4096_plain` | 24,445 -> 23,726 | 53,458 -> 51,917 | 15 -> 15 | 0 -> 0 | 2,952 -> 2,952 | 1.03x |
| `description/peak/sm_4096_plain` | 24,649 -> 24,381 | 53,860 -> 53,356 | 16 -> 16 | 0 -> 0 | 2,968 -> 2,968 | 1.01x |
| `description/hash/sm_4096_escaped` | 40,384 -> 38,909 | 88,337 -> 85,087 | 20 -> 20 | 2 -> 2 | 19,707 -> 19,707 | 1.04x |
| `description/duration/sm_4096_escaped` | 38,109 -> 37,726 | 83,324 -> 82,491 | 19 -> 19 | 0 -> 0 | 19,352 -> 19,352 | 1.01x |
| `description/peak/sm_4096_escaped` | 37,999 -> 37,586 | 83,081 -> 82,165 | 20 -> 20 | 0 -> 0 | 19,368 -> 19,368 | 1.01x |
| `description/hash/sm_4096_cp1252` | 87,325 -> 87,295 | 190,924 -> 190,926 | 20 -> 20 | 2 -> 2 | 36,091 -> 36,091 | 1.00x |
| `description/duration/sm_4096_cp1252` | 85,679 -> 83,586 | 187,283 -> 182,782 | 19 -> 19 | 0 -> 0 | 35,736 -> 35,736 | 1.03x |
| `description/peak/sm_4096_cp1252` | 86,451 -> 83,584 | 187,942 -> 182,796 | 20 -> 20 | 0 -> 0 | 35,752 -> 35,752 | 1.03x |

### Complete timing snapshots

| Case | ns old -> new | cycles old -> new | allocs old -> new | reallocs old -> new | bytes old -> new | throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `label_snapshot/0_unique` | 1,494 -> 1,641 | 3,271 -> 3,550 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 0.91x |
| `label_snapshot/0_repeat` | 1,492 -> 1,657 | 3,266 -> 3,629 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 0.90x |
| `label_snapshot/0_replace` | 1,553 -> 1,682 | 3,376 -> 3,687 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 0.92x |
| `label_snapshot/0_mixed` | 1,493 -> 1,687 | 3,271 -> 3,637 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 0.89x |
| `label_snapshot/0_long` | 1,503 -> 1,514 | 3,275 -> 3,318 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 0.99x |
| `label_snapshot/0_invalid` | 1,578 -> 1,663 | 3,463 -> 3,631 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 0.95x |
| `label_snapshot/1_unique` | 1,632 -> 1,692 | 3,546 -> 3,704 | 9 -> 9 | 0 -> 0 | 415 -> 415 | 0.96x |
| `label_snapshot/1_repeat` | 1,713 -> 1,783 | 3,751 -> 3,901 | 9 -> 9 | 0 -> 0 | 413 -> 413 | 0.96x |
| `label_snapshot/1_replace` | 1,885 -> 1,754 | 4,125 -> 3,840 | 9 -> 9 | 0 -> 0 | 415 -> 415 | 1.07x |
| `label_snapshot/1_mixed` | 1,784 -> 1,761 | 3,877 -> 3,861 | 9 -> 9 | 0 -> 0 | 414 -> 414 | 1.01x |
| `label_snapshot/1_long` | 1,784 -> 2,068 | 3,905 -> 4,526 | 9 -> 9 | 0 -> 0 | 3,384 -> 3,384 | 0.86x |
| `label_snapshot/1_invalid` | 1,560 -> 1,724 | 3,415 -> 3,715 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 0.90x |
| `label_snapshot/32_unique` | 7,769 -> 7,378 | 16,966 -> 16,141 | 40 -> 40 | 1 -> 1 | 2,830 -> 2,830 | 1.05x |
| `label_snapshot/32_repeat` | 6,686 -> 4,345 | 14,644 -> 9,512 | 40 -> 9 | 0 -> 0 | 1,048 -> 893 | 1.54x |
| `label_snapshot/32_replace` | 7,499 -> 5,002 | 16,408 -> 10,964 | 40 -> 9 | 0 -> 0 | 1,230 -> 992 | 1.50x |
| `label_snapshot/32_mixed` | 7,582 -> 4,758 | 16,562 -> 10,407 | 40 -> 12 | 0 -> 0 | 1,144 -> 976 | 1.59x |
| `label_snapshot/32_long` | 13,960 -> 10,042 | 30,502 -> 21,955 | 40 -> 9 | 0 -> 0 | 98,872 -> 67,128 | 1.39x |
| `label_snapshot/32_invalid` | 3,550 -> 3,326 | 7,783 -> 7,286 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 1.07x |
| `label_snapshot/256_unique` | 48,792 -> 46,657 | 106,673 -> 102,009 | 264 -> 264 | 1 -> 1 | 23,210 -> 23,210 | 1.05x |
| `label_snapshot/256_repeat` | 43,857 -> 21,840 | 95,883 -> 47,755 | 264 -> 9 | 0 -> 0 | 6,648 -> 5,373 | 2.01x |
| `label_snapshot/256_replace` | 44,952 -> 26,043 | 98,289 -> 56,971 | 264 -> 9 | 0 -> 0 | 8,426 -> 6,241 | 1.73x |
| `label_snapshot/256_mixed` | 46,918 -> 23,681 | 102,668 -> 51,758 | 264 -> 40 | 0 -> 0 | 7,944 -> 6,446 | 1.98x |
| `label_snapshot/256_long` | 93,978 -> 71,354 | 205,575 -> 155,923 | 264 -> 9 | 0 -> 0 | 393,496 -> 132,376 | 1.32x |
| `label_snapshot/256_invalid` | 17,003 -> 15,574 | 37,203 -> 34,014 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 1.09x |

### Complete BPM JSON reports

| Case | ns old -> new | cycles old -> new | allocs old -> new | reallocs old -> new | bytes old -> new | throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `bpm_report/0` | 27,665 -> 26,448 | 60,499 -> 57,957 | 6 -> 6 | 0 -> 0 | 106 -> 106 | 1.05x |
| `bpm_report/1` | 28,242 -> 29,522 | 61,680 -> 64,658 | 6 -> 6 | 0 -> 0 | 106 -> 106 | 0.96x |
| `bpm_report/32` | 52,870 -> 53,256 | 115,606 -> 116,460 | 6 -> 6 | 0 -> 0 | 106 -> 106 | 0.99x |
| `bpm_report/512` | 427,781 -> 444,361 | 934,994 -> 971,601 | 6 -> 6 | 0 -> 0 | 106 -> 106 | 0.96x |
| `bpm_report/1024` | 1,215,664 -> 847,960 | 2,658,846 -> 1,852,419 | 6 -> 6 | 0 -> 0 | 106 -> 106 | 1.43x |
| `bpm_report/2048` | 2,041,664 -> 1,725,230 | 4,462,363 -> 3,759,376 | 6 -> 6 | 0 -> 0 | 106 -> 106 | 1.18x |

### Analysis controls

| Case | ns old -> new | cycles old -> new | allocs old -> new | reallocs old -> new | bytes old -> new | throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `analyze/fast_fake_lifts` | 320,177 -> 313,427 | 700,183 -> 684,811 | 31 -> 31 | 4 -> 4 | 59,028 -> 59,028 | 1.02x |
| `analyze/camellia` | 208,876,867 -> 207,029,567 | 456,659,652 -> 452,483,300 | 110 -> 110 | 0 -> 0 | 5,263,624 -> 5,263,624 | 1.01x |
| `analyze/fast_camellia` | 26,569,467 -> 26,459,433 | 58,033,971 -> 57,861,078 | 115 -> 115 | 0 -> 0 | 7,051,152 -> 7,051,152 | 1.00x |
| `analyze/mixed_small` | 24,500 -> 22,213 | 53,287 -> 48,692 | 59 -> 59 | 3 -> 3 | 7,460 -> 7,460 | 1.10x |

### JSON control

| Case | ns old -> new | cycles old -> new | allocs old -> new | reallocs old -> new | bytes old -> new | throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `report/json/16_clean` | 40,611 -> 44,889 | 88,848 -> 98,166 | 18 -> 18 | 0 -> 0 | 318 -> 318 | 0.90x |

### Focused controls, 10,000 calls per batch

| Case | ns old -> new | cycles old -> new | allocs old -> new | reallocs old -> new | bytes old -> new | throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `analyze/mixed_small` | 23,297 -> 23,810 | 50,921 -> 52,004 | 59 -> 59 | 3 -> 3 | 7,460 -> 7,460 | 0.98x |
| `label_snapshot/256_invalid` | 16,686 -> 15,624 | 36,501 -> 34,158 | 9 -> 9 | 0 -> 0 | 322 -> 322 | 1.07x |

### Isolated short inputs, 10,000 calls per batch

| Case | ns old -> new | cycles old -> new | allocs old -> new | reallocs old -> new | bytes old -> new | throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `description/duration/0.74_16_plain` | 4,893 -> 4,875 | 10,702 -> 10,618 | 13 -> 13 | 0 -> 0 | 2,408 -> 2,408 | 1.00x |
| `description/peak/sm_16_plain` | 3,999 -> 4,078 | 8,749 -> 8,925 | 16 -> 16 | 0 -> 0 | 2,968 -> 2,968 | 0.98x |
| `description/hash/NaN_16_escaped` | 6,460 -> 6,459 | 14,126 -> 14,113 | 20 -> 20 | 2 -> 2 | 2,867 -> 2,867 | 1.00x |
| `label_snapshot/1_replace` | 1,811 -> 1,788 | 3,960 -> 3,913 | 9 -> 9 | 0 -> 0 | 415 -> 415 | 1.01x |
| `bpm_report/0` | 27,919 -> 26,660 | 61,052 -> 58,294 | 6 -> 6 | 0 -> 0 | 106 -> 106 | 1.05x |

### Private production leaf functions

| Case | ns old -> new | throughput |
| --- | ---: | ---: |
| `report_leaf/labels/0_unique` | 154 -> 159 | 0.97x |
| `report_leaf/labels/0_repeat` | 152 -> 174 | 0.87x |
| `report_leaf/labels/0_replace` | 161 -> 164 | 0.98x |
| `report_leaf/labels/0_mixed` | 160 -> 177 | 0.90x |
| `report_leaf/labels/0_long` | 159 -> 162 | 0.98x |
| `report_leaf/labels/0_invalid` | 158 -> 154 | 1.03x |
| `report_leaf/labels/1_unique` | 251 -> 234 | 1.07x |
| `report_leaf/labels/1_repeat` | 245 -> 225 | 1.09x |
| `report_leaf/labels/1_replace` | 255 -> 229 | 1.11x |
| `report_leaf/labels/1_mixed` | 241 -> 227 | 1.06x |
| `report_leaf/labels/1_long` | 418 -> 384 | 1.09x |
| `report_leaf/labels/1_invalid` | 208 -> 198 | 1.05x |
| `report_leaf/labels/32_unique` | 5,825 -> 5,141 | 1.13x |
| `report_leaf/labels/32_repeat` | 5,830 -> 2,544 | 2.29x |
| `report_leaf/labels/32_replace` | 6,166 -> 3,111 | 1.98x |
| `report_leaf/labels/32_mixed` | 5,892 -> 2,851 | 2.07x |
| `report_leaf/labels/32_long` | 12,330 -> 9,413 | 1.31x |
| `report_leaf/labels/32_invalid` | 2,083 -> 1,858 | 1.12x |
| `report_leaf/labels/256_unique` | 48,807 -> 42,774 | 1.14x |
| `report_leaf/labels/256_repeat` | 45,347 -> 19,434 | 2.33x |
| `report_leaf/labels/256_replace` | 50,016 -> 24,264 | 2.06x |
| `report_leaf/labels/256_mixed` | 48,689 -> 21,897 | 2.22x |
| `report_leaf/labels/256_long` | 98,159 -> 68,525 | 1.43x |
| `report_leaf/labels/256_invalid` | 15,973 -> 13,782 | 1.16x |
| `report_leaf/native_bpms/0_false` | 230 -> 229 | 1.00x |
| `report_leaf/native_bpms/0_true` | 239 -> 244 | 0.98x |
| `report_leaf/native_bpms/1_false` | 499 -> 502 | 0.99x |
| `report_leaf/native_bpms/1_true` | 515 -> 504 | 1.02x |
| `report_leaf/native_bpms/32_false` | 16,756 -> 16,720 | 1.00x |
| `report_leaf/native_bpms/32_true` | 16,885 -> 16,664 | 1.01x |
| `report_leaf/native_bpms/512_false` | 272,727 -> 274,204 | 0.99x |
| `report_leaf/native_bpms/512_true` | 275,407 -> 278,378 | 0.99x |
| `report_leaf/native_bpms/1024_false` | 1,028,159 -> 556,069 | 1.85x |
| `report_leaf/native_bpms/1024_true` | 1,024,572 -> 556,680 | 1.84x |
| `report_leaf/native_bpms/2048_false` | 1,562,016 -> 1,168,871 | 1.34x |
| `report_leaf/native_bpms/2048_true` | 1,665,520 -> 1,192,476 | 1.40x |


## Validation and limits

- 238 release unit/edge tests pass, including the original implementation's
  29 edge fixtures. New checks cover SSC version boundaries, SM difficulty
  promotion, retained legacy chart names, Unicode labels, merging, empty maps,
  extreme floats, the 910/911-entry lower bound and partial-write failures.
- Original/final UTF-8 component and corpus transcripts are byte-identical:
  168,735,139 bytes, SHA-256 `34c70dab6f97cc907f1ee626f3456ac4958fd9898802b62311e1c0fda0972710`.
  The corpus includes 30,843 files / 56,125 supported charts, with matching
  parse errors. Added component cases compare utility outputs, snapshots and
  complete reports at all fixture sizes.
- After the optimizations were confirmed, the required command
  `cargo test --release --test all_parity -- --test-threads=22`
  passes all 30,489 cases. Strict release Clippy for the workspace/all targets,
  formatting and whitespace checks pass.

Description savings apply to legacy SSC utility APIs. Distinct labels retain
the same allocations, and label vectors retain their original capacity.
The BPM change retains the 16 KB stack buffer; it saves the abandoned
formatting work for large maps, without claiming a stack-memory reduction.
The focused controls and isolated short-input measurements above use longer
batches to distinguish stable differences from short-run variation. Invalid
label defaults and all unchanged controls retain their allocation counts.
No universal CPU-speedup claim is made; all slower cases are retained.

A broader direct-streaming candidate was rejected: its three-pair medians
were slower by 2.1% for buffered 32-entry maps and 1.6% for buffered 512-entry
maps (2.8% for 512 entries into Vec), despite large gains on empty and
oversized maps. The final change preserves the existing smaller-map path.

Reproduce the composed groups with `cargo bench -p rssp --bench hotpath_perf`,
`RSSP_HOT_FILTER` set to `description/`, `label_snapshot/`, `bpm_report/`,
`analyze/` or `report/json/16_clean`, and `RSSP_HOT_ITERS` as above.
Run private leaves with
`cargo test --release -p rssp --lib report::perf::report_hotpath -- --ignored --nocapture --test-threads=1`,
`RSSP_REPORT_FILTER=labels/` or `native_bpms/<count>_` and
`RSSP_REPORT_ITERS` as above; pin this process to CPU 2.
`RSSP_HOT_VERIFY=1` emits the deterministic comparison transcript.

## 0.4.282 — background decoding, course scanning, and report allocations

Compared with `cb9d94e` (0.4.281). Both builds use version 0.4.282 and the same benchmark/test harness. The version advances exactly one patch.

Three retained changes:

1. Background changes use the existing `decode_unescape` function, which keeps an owned CP1252 decode buffer when unescaping. This removes the second allocation and copy for escaped CP1252 tags. UTF-8 still follows the borrowed path.
2. Course parsing searches for `#` and `:` with `memchr`. Its terminator scanner skips ordinary prefixes with `memchr2`, while tags shorter than 32 bytes retain scalar scanning. Backslash parity and unterminated-tag behavior are preserved. The forwarding `parse_crs_impl`/`parse_crs` layer is removed.
3. Course report placeholders keep song metadata empty and share the chart timing allocation. Chart writers consume the course chart, timing-text defaults, and flags; they never read the placeholder title or global timing allocation. This removes a title clone and a fresh empty `TimingSegments` allocation from Full, Pretty, and JSON reports. CSV remains a control.

Representative results:

- Escaped CP1252 background tag with 32 changes: allocations 3 → 2 (33.3% less); requested allocation bytes 19,686 → 11,043 (43.9% less).
- Course report placeholder with a 16-byte title: allocations 2 → 0 (100.0% less); requested allocation bytes 208 → 0 (100.0% less).
- Course parsing with 32 entries and 4 KB tag values: CPU cycles 428,030 → 55,503 (87.0% less).

Measurement protocol: Windows x86-64 MSVC, Rust 1.98.1 / LLVM 22.1.8, release fat LTO and one codegen unit. Benchmark threads are pinned to CPU 2. Each process takes the median of seven batches after four warmups; tables take the median of three alternating old/new process pairs. No other rssp builds, tests, or corpus comparisons run during timing. Fixture creation and file I/O are outside timing. Parsing, output writes, and result destruction are included when those operations are the target. Allocation counting runs separately from timing and counts successful allocations, reallocations, and requested bytes; requested bytes are churn, not peak RSS.

Private production functions are called from explicit ignored tests. The hotpath benchmark measures composed course parsing and retains numeric snapshots and analysis controls. Raw pair logs and measurements are in ignored `target/perf-282/`. Reproduce the benchmark binaries with `cargo test --release -p rssp --lib --no-run` and `cargo bench -p rssp --bench hotpath_perf --no-run`. Run private cases with `RSSP_PASS_FILTER` / `RSSP_PASS_ITERS` and `pass_edges --skip course_trace --ignored --nocapture --test-threads=1`; use `RSSP_HOT_FILTER` / `RSSP_HOT_ITERS` for the hotpath binary.

Rejected candidates: owned-buffer reuse in title matching cut allocations but increased cycles for 4 KB titles with every CP1252 character escaped; that caller retains its original implementation. Removing whole-segment trims in numeric report parsers produced small mixed gains and slower malformed triples, so those parsers retain their original implementation. An unconditional vector-prefix search slowed short dense escapes; the retained scanner uses its scalar path below 32 bytes.

All measured cases are listed below, including slower controls. Small-call timings and unchanged control paths can vary with code layout and the shared machine; these results do not establish a universal CPU improvement. No measured allocation count, reallocation count, or requested-byte total increases.

### `background/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `1_plain` | 1,417 → 1,460 | 647 → 666 | -2.9% | 1 → 1 | 0 → 0 | 160 → 160 |
| `1_escaped` | 2,212 → 2,237 | 1,009 → 1,022 | -1.3% | 2 → 2 | 0 → 0 | 300 → 300 |
| `1_cp1252` | 2,760 → 2,835 | 1,260 → 1,293 | -2.6% | 2 → 2 | 0 → 0 | 427 → 427 |
| `1_cp_escape` | 4,027 → 3,782 | 1,841 → 1,726 | +6.7% | 3 → 2 | 0 → 0 | 696 → 428 |
| `32_plain` | 11,843 → 11,983 | 5,408 → 5,473 | -1.2% | 1 → 1 | 3 → 3 | 2,400 → 2,400 |
| `32_escaped` | 30,932 → 31,243 | 14,124 → 14,261 | -1.0% | 2 → 2 | 3 → 3 | 6,947 → 6,947 |
| `32_cp1252` | 46,806 → 47,186 | 21,370 → 21,536 | -0.8% | 2 → 2 | 3 → 3 | 11,011 → 11,011 |
| `32_cp_escape` | 83,884 → 79,319 | 38,301 → 36,221 | +5.7% | 3 → 2 | 3 → 3 | 19,686 → 11,043 |

### `course_dummy/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `0` | 362 → 123 | 165 → 56 | +194.6% | 1 → 0 | 0 → 0 | 192 → 0 |
| `16` | 519 → 123 | 237 → 56 | +323.2% | 2 → 0 | 0 → 0 | 208 → 0 |
| `4096` | 804 → 123 | 368 → 56 | +557.1% | 2 → 0 | 0 → 0 | 4,288 → 0 |

### `course_report/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `0_Full` | 12,554 → 12,761 | 5,733 → 5,827 | -1.6% | 1 → 0 | 0 → 0 | 192 → 0 |
| `0_Pretty` | 5,982 → 5,691 | 2,736 → 2,597 | +5.4% | 1 → 0 | 0 → 0 | 192 → 0 |
| `0_JSON` | 26,977 → 25,861 | 12,320 → 11,803 | +4.4% | 7 → 6 | 0 → 0 | 298 → 106 |
| `0_CSV` | 1,847 → 1,899 | 847 → 867 | -2.3% | 1 → 1 | 0 → 0 | 16 → 16 |
| `16_Full` | 12,737 → 12,648 | 5,813 → 5,777 | +0.6% | 2 → 0 | 0 → 0 | 208 → 0 |
| `16_Pretty` | 6,186 → 5,744 | 2,822 → 2,625 | +7.5% | 2 → 0 | 0 → 0 | 208 → 0 |
| `16_JSON` | 27,130 → 26,016 | 12,392 → 11,875 | +4.4% | 8 → 6 | 0 → 0 | 314 → 106 |
| `16_CSV` | 1,870 → 1,889 | 853 → 862 | -1.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `4096_Full` | 14,009 → 13,033 | 6,400 → 5,954 | +7.5% | 2 → 0 | 0 → 0 | 4,288 → 0 |
| `4096_Pretty` | 6,839 → 5,819 | 3,123 → 2,653 | +17.7% | 2 → 0 | 0 → 0 | 4,288 → 0 |
| `4096_JSON` | 33,587 → 32,896 | 15,347 → 15,025 | +2.1% | 8 → 6 | 0 → 0 | 4,394 → 106 |
| `4096_CSV` | 2,084 → 2,026 | 955 → 925 | +3.2% | 1 → 1 | 0 → 0 | 16 → 16 |

### `course_scan/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `16_plain` | 48 → 50 | 22 → 23 | -4.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `16_escaped` | 58 → 57 | 26 → 26 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `16_unterminated` | 45 → 46 | 21 → 21 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `16_dense` | 47 → 46 | 22 → 21 | +4.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `4096_plain` | 10,450 → 233 | 4,770 → 106 | +4400.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `4096_escaped` | 10,446 → 241 | 4,769 → 110 | +4235.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `4096_unterminated` | 10,404 → 231 | 4,748 → 105 | +4421.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `4096_dense` | 10,443 → 10,457 | 4,766 → 4,772 | -0.1% | 0 → 0 | 0 → 0 | 0 → 0 |

### `course_parse/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `1_16_false` | 2,549 → 2,297 | 1,164 → 1,050 | +10.9% | 6 → 6 | 0 → 0 | 519 → 519 |
| `1_16_true` | 2,381 → 2,417 | 1,087 → 1,104 | -1.5% | 6 → 6 | 0 → 0 | 525 → 525 |
| `1_4096_false` | 31,069 → 3,519 | 14,183 → 1,608 | +782.0% | 6 → 6 | 0 → 0 | 4,599 → 4,599 |
| `1_4096_true` | 30,949 → 3,599 | 14,127 → 1,641 | +760.9% | 6 → 6 | 0 → 0 | 4,605 → 4,605 |
| `32_16_false` | 45,644 → 43,566 | 20,863 → 19,893 | +4.9% | 99 → 99 | 0 → 0 | 15,530 → 15,530 |
| `32_16_true` | 46,207 → 44,583 | 21,088 → 20,365 | +3.6% | 99 → 99 | 0 → 0 | 16,016 → 16,016 |
| `32_4096_false` | 428,030 → 55,503 | 195,393 → 25,348 | +670.8% | 99 → 99 | 0 → 0 | 20,090 → 20,090 |
| `32_4096_true` | 428,273 → 54,636 | 195,533 → 24,955 | +683.5% | 99 → 99 | 0 → 0 | 20,096 → 20,096 |

### `title_match/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `16_plain` | 1,111 → 1,115 | 508 → 512 | -0.8% | 1 → 1 | 0 → 0 | 520 → 520 |
| `16_escaped` | 1,618 → 1,598 | 739 → 729 | +1.4% | 3 → 3 | 0 → 0 | 584 → 584 |
| `16_cp1252` | 1,777 → 1,943 | 811 → 887 | -8.6% | 3 → 3 | 0 → 0 | 584 → 584 |
| `16_cp_escape` | 2,481 → 2,797 | 1,132 → 1,278 | -11.4% | 5 → 5 | 0 → 0 | 712 → 712 |
| `4096_plain` | 16,628 → 17,871 | 7,594 → 8,160 | -6.9% | 1 → 1 | 0 → 0 | 520 → 520 |
| `4096_escaped` | 58,322 → 56,597 | 26,626 → 25,845 | +3.0% | 3 → 3 | 0 → 0 | 16,904 → 16,904 |
| `4096_cp1252` | 90,630 → 87,006 | 41,374 → 39,721 | +4.2% | 3 → 3 | 0 → 0 | 16,904 → 16,904 |
| `4096_cp_escape` | 203,704 → 192,331 | 93,026 → 87,804 | +5.9% | 5 → 5 | 0 → 0 | 49,672 → 49,672 |

### `numeric/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `ticks/0_plain` | 201 → 193 | 92 → 88 | +4.5% | 1 → 1 | 0 → 0 | 16 → 16 |
| `signatures/0_plain` | 188 → 183 | 86 → 83 | +3.6% | 1 → 1 | 0 → 0 | 16 → 16 |
| `combos/0_plain` | 177 → 177 | 81 → 81 | +0.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `ticks/0_padded` | 193 → 193 | 88 → 88 | +0.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `signatures/0_padded` | 189 → 183 | 87 → 83 | +4.8% | 1 → 1 | 0 → 0 | 16 → 16 |
| `combos/0_padded` | 177 → 178 | 83 → 81 | +2.5% | 1 → 1 | 0 → 0 | 16 → 16 |
| `ticks/0_invalid` | 206 → 193 | 94 → 88 | +6.8% | 1 → 1 | 0 → 0 | 16 → 16 |
| `signatures/0_invalid` | 188 → 183 | 86 → 83 | +3.6% | 1 → 1 | 0 → 0 | 16 → 16 |
| `combos/0_invalid` | 205 → 178 | 94 → 81 | +16.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `ticks/1_plain` | 379 → 367 | 175 → 168 | +4.2% | 1 → 1 | 0 → 0 | 64 → 64 |
| `signatures/1_plain` | 452 → 421 | 207 → 193 | +7.3% | 1 → 1 | 0 → 0 | 80 → 80 |
| `combos/1_plain` | 417 → 407 | 190 → 186 | +2.2% | 1 → 1 | 0 → 0 | 64 → 64 |
| `ticks/1_padded` | 433 → 411 | 197 → 188 | +4.8% | 1 → 1 | 0 → 0 | 64 → 64 |
| `signatures/1_padded` | 477 → 466 | 217 → 213 | +1.9% | 1 → 1 | 0 → 0 | 80 → 80 |
| `combos/1_padded` | 469 → 457 | 215 → 209 | +2.9% | 1 → 1 | 0 → 0 | 64 → 64 |
| `ticks/1_invalid` | 319 → 309 | 146 → 141 | +3.5% | 1 → 1 | 0 → 0 | 16 → 16 |
| `signatures/1_invalid` | 258 → 253 | 118 → 118 | +0.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `combos/1_invalid` | 257 → 248 | 117 → 113 | +3.5% | 1 → 1 | 0 → 0 | 16 → 16 |
| `ticks/32_plain` | 6,128 → 6,046 | 2,798 → 2,759 | +1.4% | 1 → 1 | 1 → 1 | 1,344 → 1,344 |
| `signatures/32_plain` | 7,369 → 7,324 | 3,364 → 3,347 | +0.5% | 1 → 1 | 0 → 0 | 544 → 544 |
| `combos/32_plain` | 7,503 → 7,434 | 3,425 → 3,391 | +1.0% | 1 → 1 | 1 → 1 | 1,392 → 1,392 |
| `ticks/32_padded` | 7,335 → 7,632 | 3,346 → 3,484 | -4.0% | 1 → 1 | 0 → 0 | 1,216 → 1,216 |
| `signatures/32_padded` | 8,801 → 8,712 | 4,017 → 3,978 | +1.0% | 1 → 1 | 0 → 0 | 1,200 → 1,200 |
| `combos/32_padded` | 8,588 → 8,680 | 3,920 → 3,960 | -1.0% | 1 → 1 | 0 → 0 | 1,040 → 1,040 |
| `ticks/32_invalid` | 4,593 → 4,582 | 2,095 → 2,092 | +0.1% | 1 → 1 | 0 → 0 | 16 → 16 |
| `signatures/32_invalid` | 2,783 → 2,790 | 1,274 → 1,273 | +0.1% | 1 → 1 | 0 → 0 | 16 → 16 |
| `combos/32_invalid` | 2,778 → 2,786 | 1,269 → 1,272 | -0.2% | 1 → 1 | 0 → 0 | 16 → 16 |
| `ticks/256_plain` | 45,900 → 44,984 | 20,950 → 20,534 | +2.0% | 1 → 1 | 1 → 1 | 12,144 → 12,144 |
| `signatures/256_plain` | 55,261 → 54,968 | 25,222 → 25,087 | +0.5% | 1 → 1 | 0 → 0 | 4,656 → 4,656 |
| `combos/256_plain` | 55,649 → 55,339 | 25,398 → 25,259 | +0.6% | 1 → 1 | 1 → 1 | 12,192 → 12,192 |
| `ticks/256_padded` | 59,889 → 60,473 | 27,333 → 27,603 | -1.0% | 1 → 1 | 0 → 0 | 10,192 → 10,192 |
| `signatures/256_padded` | 74,929 → 70,367 | 34,213 → 32,111 | +6.5% | 1 → 1 | 0 → 0 | 9,920 → 9,920 |
| `combos/256_padded` | 72,859 → 70,792 | 33,252 → 32,310 | +2.9% | 1 → 1 | 0 → 0 | 8,672 → 8,672 |
| `ticks/256_invalid` | 36,065 → 35,643 | 16,458 → 16,269 | +1.2% | 1 → 1 | 0 → 0 | 16 → 16 |
| `signatures/256_invalid` | 21,852 → 21,462 | 9,977 → 9,797 | +1.8% | 1 → 1 | 0 → 0 | 16 → 16 |
| `combos/256_invalid` | 21,671 → 22,005 | 9,895 → 10,041 | -1.5% | 1 → 1 | 0 → 0 | 16 → 16 |

### `numeric_snapshot/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `0_plain` | 3,568 → 5,968 | 1,628 → 2,732 | -40.4% | 9 → 9 | 0 → 0 | 322 → 322 |
| `0_padded` | 3,573 → 4,246 | 1,633 → 2,211 | -26.1% | 9 → 9 | 0 → 0 | 322 → 322 |
| `0_invalid` | 3,633 → 7,580 | 1,670 → 3,591 | -53.5% | 9 → 9 | 0 → 0 | 322 → 322 |
| `1_plain` | 3,572 → 3,501 | 1,635 → 1,601 | +2.1% | 9 → 9 | 0 → 0 | 482 → 482 |
| `1_padded` | 4,726 → 5,416 | 2,157 → 2,473 | -12.8% | 9 → 9 | 0 → 0 | 482 → 482 |
| `1_invalid` | 4,107 → 4,660 | 1,876 → 2,130 | -11.9% | 9 → 9 | 0 → 0 | 322 → 322 |
| `32_plain` | 25,502 → 26,006 | 11,651 → 11,883 | -2.0% | 9 → 9 | 2 → 2 | 3,554 → 3,554 |
| `32_padded` | 31,000 → 33,637 | 14,159 → 15,440 | -8.3% | 9 → 9 | 0 → 0 | 3,730 → 3,730 |
| `32_invalid` | 15,912 → 15,284 | 7,272 → 6,981 | +4.2% | 9 → 9 | 0 → 0 | 322 → 322 |
| `256_plain` | 192,000 → 217,363 | 87,710 → 106,531 | -17.7% | 9 → 9 | 2 → 2 | 29,266 → 29,266 |
| `256_padded` | 257,053 → 236,066 | 123,090 → 122,202 | +0.7% | 9 → 9 | 0 → 0 | 29,058 → 29,058 |
| `256_invalid` | 98,380 → 96,682 | 44,934 → 44,132 | +1.8% | 9 → 9 | 0 → 0 | 322 → 322 |

### `analyze/`

| Case | CPU cycles old → new | ns old → new | Throughput change | Allocs old → new | Reallocs old → new | Requested bytes old → new |
|---|---:|---:|---:|---:|---:|---:|
| `fast_fake_lifts` | 627,550 → 657,483 | 286,793 → 300,320 | -4.5% | 31 → 31 | 4 → 4 | 59,028 → 59,028 |
| `camellia` | 448,763,872 → 446,966,826 | 204,885,200 → 204,085,667 | +0.4% | 110 → 110 | 0 → 0 | 5,263,624 → 5,263,624 |
| `fast_camellia` | 57,849,369 → 57,309,256 | 26,415,700 → 26,173,400 | +0.9% | 115 → 115 | 0 → 0 | 7,051,152 → 7,051,152 |
| `mixed_small` | 48,670 → 50,734 | 22,223 → 23,280 | -4.5% | 59 → 59 | 3 → 3 | 7,460 → 7,460 |

### Repeated unchanged controls

Title matching and numeric snapshots were repeated with 10,000 iterations per batch. Their production implementations are unchanged. Empty snapshots have identical input and allocation counts across the three fixture kinds, yet process medians range from 1,487 to 3,578 ns. This variation limits attribution of short-control CPU differences. Allocation savings and the large course-scanning improvement are independently measurable.

| Case | CPU cycles old → new | ns old → new | Throughput change |
|---|---:|---:|---:|
| `title_match/16_plain` | 1,135 → 1,151 | 518 → 526 | -1.5% |
| `title_match/16_escaped` | 1,635 → 1,657 | 746 → 763 | -2.2% |
| `title_match/16_cp1252` | 1,881 → 1,806 | 858 → 827 | +3.7% |
| `title_match/16_cp_escape` | 2,569 → 2,694 | 1,173 → 1,230 | -4.6% |
| `title_match/4096_plain` | 16,744 → 16,807 | 7,643 → 7,677 | -0.4% |
| `title_match/4096_escaped` | 54,923 → 55,040 | 25,079 → 25,128 | -0.2% |
| `title_match/4096_cp1252` | 89,161 → 87,205 | 40,755 → 39,814 | +2.4% |
| `title_match/4096_cp_escape` | 188,631 → 191,207 | 86,163 → 87,292 | -1.3% |
| `numeric_snapshot/0_plain` | 3,484 → 3,351 | 1,591 → 1,530 | +4.0% |
| `numeric_snapshot/0_padded` | 3,557 → 3,419 | 1,624 → 1,562 | +4.0% |
| `numeric_snapshot/0_invalid` | 3,440 → 3,351 | 1,570 → 1,531 | +2.5% |
| `numeric_snapshot/1_plain` | 3,239 → 3,510 | 1,479 → 1,603 | -7.7% |
| `numeric_snapshot/1_padded` | 4,278 → 4,728 | 1,952 → 2,158 | -9.5% |
| `numeric_snapshot/1_invalid` | 3,877 → 4,210 | 1,769 → 1,921 | -7.9% |
| `numeric_snapshot/32_plain` | 23,475 → 24,016 | 10,716 → 10,961 | -2.2% |
| `numeric_snapshot/32_padded` | 27,066 → 27,679 | 12,355 → 12,637 | -2.2% |
| `numeric_snapshot/32_invalid` | 13,561 → 13,925 | 6,190 → 6,356 | -2.6% |
| `numeric_snapshot/256_plain` | 165,421 → 163,542 | 75,533 → 74,860 | +0.9% |
| `numeric_snapshot/256_padded` | 206,678 → 207,967 | 94,495 → 95,123 | -0.7% |
| `numeric_snapshot/256_invalid` | 83,689 → 82,447 | 38,204 → 37,634 | +1.5% |

Validation:

- Original and optimized library builds: 75 release tests each pass, with six explicit benchmark/trace tests ignored. Core: 139 release tests. Optimization edges: 29 release tests.
- `cargo test --release --test all_parity -- --test-threads=22`: **30,489 passed, 0 failed**, run after final benchmark confirmation and before committing.
- Component plus corpus comparison: **168,778,790 identical UTF-8 bytes**, SHA-256 `a955dcdd39b28604d1b05fc26f27202ab2605a21c6b1afe0557fff1e5aa62af8`. It covers 30,843 files, 56,125 supported charts, 30,489 successes and 354 matching errors, plus existing component fixtures and course parser fixtures.
- Separate course report comparison: all 12 complete outputs (three title lengths × four output modes) are byte-identical. JSON fields, flags, native timing, partial-write prefixes, first I/O errors, and post-call reference counts are tested directly.
- `cargo clippy --release --workspace --all-targets -- -D warnings`, formatting, and diff checks pass.
- No cache, dependency, timing arithmetic, production unsafe code, or new production API is added. `rust-performance.md`, `optimize.sh`, and `optimize.ps1` are excluded from the commit.

# 0.4.283: stream course hashes and reuse sorting and normalization work

## Changes

1. Course CSV writes hash strings and separators directly to the output writer.
   This deletes the length prepasses, temporary String, buffer reset, and extra
   copies. Empty lists, empty hashes, embedded separators, Unicode, and partial
   writer errors retain their output bytes and error propagation. The CLI uses
   its existing stdout lock; callers writing files should continue buffering
   report output because reports issue many writes. No disk I/O is timed here.
2. Pack sorting stores permutation destinations in key offsets after sorting,
   when those offsets are no longer needed. This deletes the separate zeroed
   destination Vec. ASCII-insensitive ordering, stable ties, record ownership,
   the small-list path, and oversized-input fallbacks are preserved. The sort
   still performs a bounded number of swaps; no cache or new lifetime is added.
3. Speed-map cleanup retains its already-formatted prefix on the first dirty
   entry. It cleans the raw prefix and then cleans/formats the remainder once,
   deleting the speculative normalizer helper, discarded allocation, and
   repeated prefix parsing/formatting. Clean raw maps remain borrowed. The
   original control-character, whitespace, numeric, and unit rules remain.

The three production edits together remove nine lines. There are no new
dependencies, public APIs, production unsafe blocks, or caches. The workspace
patch version changes exactly once, from 0.4.282 to 0.4.283.

## Method

Original production code is commit 1ad0434. Both baseline and optimized builds
use version 0.4.283 and identical fixtures/harnesses. Original executables were
saved before applying the production edits. Rust 1.98.1 / LLVM 22.1.8 targets
x86_64-pc-windows-msvc on the Xeon E5-2696 v4 host with 44 logical CPUs. Release
and bench retain fat LTO and one codegen unit; bench retains debug information.

Benchmarks call production functions directly. Four warmups precede seven
batches, with the thread pinned to logical CPU 2. Each table reports medians of
three alternating process pairs: old/new, new/old, old/new. Windows
QueryThreadCycleTime supplies thread cycles; elapsed time supplies throughput.
Allocator calls/requested bytes are counted separately for one invocation.
Requested bytes measure allocation churn, including realloc requests, rather
than process RSS. Fixtures, parsing/setup for output benchmarks, buffer reserve,
and fresh sorting-input clones are outside measurement. Sorting scratch and
target normalization allocations are inside measurement. Sort input/output
destruction occurs after the timer; target scratch destruction is included.

Pack sorting uses 500 iterations/batch for the generic sorter and 200 for the
PackScan caller, across ordered, reversed, rotated, mixed, and equal names.
Every iteration receives fresh unsorted input. Sizes 1/4 exercise the unchanged
small path; 5/32/256/4096 exercise compact keys. The PackScan caller stops at 256
to avoid excessive setup memory. CSV uses 2,000 iterations with preallocated Vec
and 8 KiB BufWriter outputs, ordinary 16-byte hashes, arbitrary hash bytes, and
1 KiB hashes. Cleanup uses 300 iterations on 1/128/4096-entry pair/speed maps;
unchanged pair maps are controls. Four-chart speed loads use 80 iterations;
ordinary analysis uses 40 (Camellia uses four); course report controls use 2,000.
The focused repeat uses 3,000 iterations on the six 128-entry cleanup cases.

Scripts, saved executables, raw runs, and output traces remain in ignored
target/perf-283. run.py accepts exact case prefixes; measurements.json contains
all 114 main medians and raw process values; focused.json contains six repeats.

## Results


Course CSV removes its temporary allocation for every nonempty hash field.
For 32 ordinary hashes, thread cycles fall from 2,643 to 2,182 with Vec output
and from 2,691 to 2,385 with buffered output; throughput rises 21.7% and 13.1%.
The 4,096-hash case removes 69,631 requested bytes and gains 47.0%/31.7% in
throughput. The 32 long-hash case removes 32,799 bytes and gains 87.3%/61.3%.

Compact pack sorting changes three scratch allocations to two and requested
bytes from 40N to 36N for these fixtures: a 10% reduction, saving 16,384 bytes
at 4,096 entries. Five-entry sort throughput gains 19.4-33.1%; 32-entry sort
gains 5.1-9.4%. The 32-entry PackScan caller gains 0.7-16.1%. Large generic
sorts are mostly near flat, so the durable large-list gain is memory churn.

Late-dirty speed normalization at 4,096 entries falls from 4,946,542 to
2,879,363 cycles (-41.8%), with allocations/reallocations 3/2 -> 2/1 and
requested bytes 467,999 -> 267,428 (-42.9%). Throughput improves 71.6% in the
leaf and 45.8% in four-chart analysis. The focused 128-entry repeat retains a
70.7% throughput improvement for late dirtiness and 42.9% fewer requested
bytes. Early-dirty maps save one allocation and 20% of requested bytes.

### Pack sorter

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| pack_sort/1_sorted | 17 -> 17 | 10 -> 10 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_sort/1_reverse | 17 -> 17 | 10 -> 9 | 0/0 -> 0/0 | 0 -> 0 | +11.1% |
| pack_sort/1_cycle | 17 -> 17 | 9 -> 9 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_sort/1_mixed | 17 -> 17 | 10 -> 9 | 0/0 -> 0/0 | 0 -> 0 | +11.1% |
| pack_sort/1_equal | 16 -> 17 | 9 -> 10 | 0/0 -> 0/0 | 0 -> 0 | -10.0% |
| pack_sort/4_sorted | 138 -> 139 | 65 -> 66 | 0/0 -> 0/0 | 0 -> 0 | -1.5% |
| pack_sort/4_reverse | 259 -> 262 | 120 -> 122 | 0/0 -> 0/0 | 0 -> 0 | -1.6% |
| pack_sort/4_cycle | 216 -> 216 | 101 -> 101 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_sort/4_mixed | 139 -> 140 | 66 -> 66 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_sort/4_equal | 142 -> 141 | 67 -> 67 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_sort/5_sorted | 740 -> 567 | 346 -> 260 | 3/0 -> 2/0 | 200 -> 180 | +33.1% |
| pack_sort/5_reverse | 903 -> 753 | 413 -> 345 | 3/0 -> 2/0 | 200 -> 180 | +19.7% |
| pack_sort/5_cycle | 842 -> 697 | 388 -> 320 | 3/0 -> 2/0 | 200 -> 180 | +21.2% |
| pack_sort/5_mixed | 821 -> 687 | 376 -> 315 | 3/0 -> 2/0 | 200 -> 180 | +19.4% |
| pack_sort/5_equal | 730 -> 570 | 335 -> 261 | 3/0 -> 2/0 | 200 -> 180 | +28.4% |
| pack_sort/32_sorted | 2,597 -> 2,407 | 1,186 -> 1,099 | 3/0 -> 2/0 | 1,280 -> 1,152 | +7.9% |
| pack_sort/32_reverse | 2,504 -> 2,333 | 1,157 -> 1,066 | 3/0 -> 2/0 | 1,280 -> 1,152 | +8.5% |
| pack_sort/32_cycle | 5,778 -> 5,447 | 2,636 -> 2,508 | 3/0 -> 2/0 | 1,280 -> 1,152 | +5.1% |
| pack_sort/32_mixed | 5,183 -> 4,692 | 2,369 -> 2,171 | 3/0 -> 2/0 | 1,280 -> 1,152 | +9.1% |
| pack_sort/32_equal | 2,274 -> 2,049 | 1,039 -> 950 | 3/0 -> 2/0 | 1,280 -> 1,152 | +9.4% |
| pack_sort/256_sorted | 16,730 -> 15,378 | 7,669 -> 7,022 | 3/0 -> 2/0 | 10,240 -> 9,216 | +9.2% |
| pack_sort/256_reverse | 16,559 -> 16,569 | 7,610 -> 7,595 | 3/0 -> 2/0 | 10,240 -> 9,216 | +0.2% |
| pack_sort/256_cycle | 68,408 -> 62,823 | 31,292 -> 28,709 | 3/0 -> 2/0 | 10,240 -> 9,216 | +9.0% |
| pack_sort/256_mixed | 75,697 -> 74,689 | 34,645 -> 34,226 | 3/0 -> 2/0 | 10,240 -> 9,216 | +1.2% |
| pack_sort/256_equal | 14,140 -> 14,122 | 6,488 -> 6,464 | 3/0 -> 2/0 | 10,240 -> 9,216 | +0.4% |
| pack_sort/4096_sorted | 262,616 -> 262,229 | 120,103 -> 119,914 | 3/0 -> 2/0 | 163,840 -> 147,456 | +0.2% |
| pack_sort/4096_reverse | 292,030 -> 290,377 | 133,608 -> 132,780 | 3/0 -> 2/0 | 163,840 -> 147,456 | +0.6% |
| pack_sort/4096_cycle | 1,762,947 -> 1,778,906 | 806,180 -> 813,434 | 3/0 -> 2/0 | 163,840 -> 147,456 | -0.9% |
| pack_sort/4096_mixed | 2,221,709 -> 2,240,505 | 1,016,038 -> 1,024,734 | 3/0 -> 2/0 | 163,840 -> 147,456 | -0.8% |
| pack_sort/4096_equal | 242,346 -> 251,404 | 110,816 -> 115,025 | 3/0 -> 2/0 | 163,840 -> 147,456 | -3.7% |

### PackScan caller

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| pack_list/1_sorted | 21 -> 21 | 15 -> 14 | 0/0 -> 0/0 | 0 -> 0 | +7.1% |
| pack_list/1_reverse | 21 -> 22 | 15 -> 15 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_list/1_cycle | 21 -> 22 | 15 -> 15 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_list/1_mixed | 21 -> 21 | 14 -> 14 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_list/1_equal | 21 -> 22 | 14 -> 15 | 0/0 -> 0/0 | 0 -> 0 | -6.7% |
| pack_list/4_sorted | 200 -> 205 | 98 -> 100 | 0/0 -> 0/0 | 0 -> 0 | -2.0% |
| pack_list/4_reverse | 522 -> 530 | 245 -> 249 | 0/0 -> 0/0 | 0 -> 0 | -1.6% |
| pack_list/4_cycle | 390 -> 387 | 185 -> 184 | 0/0 -> 0/0 | 0 -> 0 | +0.5% |
| pack_list/4_mixed | 202 -> 203 | 98 -> 100 | 0/0 -> 0/0 | 0 -> 0 | -2.0% |
| pack_list/4_equal | 196 -> 196 | 96 -> 96 | 0/0 -> 0/0 | 0 -> 0 | +0.0% |
| pack_list/5_sorted | 1,070 -> 1,034 | 494 -> 478 | 3/0 -> 2/0 | 200 -> 180 | +3.3% |
| pack_list/5_reverse | 1,119 -> 988 | 518 -> 457 | 3/0 -> 2/0 | 200 -> 180 | +13.3% |
| pack_list/5_cycle | 1,122 -> 994 | 519 -> 461 | 3/0 -> 2/0 | 200 -> 180 | +12.6% |
| pack_list/5_mixed | 1,091 -> 950 | 504 -> 441 | 3/0 -> 2/0 | 200 -> 180 | +14.3% |
| pack_list/5_equal | 846 -> 676 | 399 -> 314 | 3/0 -> 2/0 | 200 -> 180 | +27.1% |
| pack_list/32_sorted | 3,841 -> 3,310 | 1,763 -> 1,518 | 3/0 -> 2/0 | 1,280 -> 1,152 | +16.1% |
| pack_list/32_reverse | 4,244 -> 3,682 | 1,943 -> 1,685 | 3/0 -> 2/0 | 1,280 -> 1,152 | +15.3% |
| pack_list/32_cycle | 7,428 -> 7,306 | 3,394 -> 3,372 | 3/0 -> 2/0 | 1,280 -> 1,152 | +0.7% |
| pack_list/32_mixed | 5,924 -> 5,558 | 2,725 -> 2,540 | 3/0 -> 2/0 | 1,280 -> 1,152 | +7.3% |
| pack_list/32_equal | 2,760 -> 2,470 | 1,269 -> 1,136 | 3/0 -> 2/0 | 1,280 -> 1,152 | +11.7% |
| pack_list/256_sorted | 31,643 -> 23,563 | 14,449 -> 10,840 | 3/0 -> 2/0 | 10,240 -> 9,216 | +33.3% |
| pack_list/256_reverse | 39,143 -> 30,619 | 17,864 -> 14,020 | 3/0 -> 2/0 | 10,240 -> 9,216 | +27.4% |
| pack_list/256_cycle | 112,777 -> 81,054 | 51,601 -> 37,158 | 3/0 -> 2/0 | 10,240 -> 9,216 | +38.9% |
| pack_list/256_mixed | 117,409 -> 95,928 | 53,714 -> 43,832 | 3/0 -> 2/0 | 10,240 -> 9,216 | +22.5% |
| pack_list/256_equal | 23,321 -> 23,668 | 10,652 -> 10,948 | 3/0 -> 2/0 | 10,240 -> 9,216 | -2.7% |

### Course CSV output

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| csv_hash/0_plain_false | 1,826 -> 1,730 | 842 -> 789 | 0/0 -> 0/0 | 0 -> 0 | +6.7% |
| csv_hash/0_plain_true | 1,709 -> 1,823 | 787 -> 833 | 0/0 -> 0/0 | 0 -> 0 | -5.5% |
| csv_hash/1_plain_false | 1,975 -> 1,680 | 902 -> 774 | 1/0 -> 0/0 | 16 -> 0 | +16.5% |
| csv_hash/1_plain_true | 2,089 -> 1,742 | 952 -> 797 | 1/0 -> 0/0 | 16 -> 0 | +19.4% |
| csv_hash/32_plain_false | 2,643 -> 2,182 | 1,212 -> 996 | 1/0 -> 0/0 | 543 -> 0 | +21.7% |
| csv_hash/32_plain_true | 2,691 -> 2,385 | 1,233 -> 1,090 | 1/0 -> 0/0 | 543 -> 0 | +13.1% |
| csv_hash/256_plain_false | 7,092 -> 5,094 | 3,244 -> 2,332 | 1/0 -> 0/0 | 4,351 -> 0 | +39.1% |
| csv_hash/256_plain_true | 8,044 -> 5,798 | 3,679 -> 2,647 | 1/0 -> 0/0 | 4,351 -> 0 | +39.0% |
| csv_hash/4096_plain_false | 113,122 -> 77,007 | 51,761 -> 35,207 | 1/0 -> 0/0 | 69,631 -> 0 | +47.0% |
| csv_hash/4096_plain_true | 114,329 -> 86,851 | 52,313 -> 39,719 | 1/0 -> 0/0 | 69,631 -> 0 | +31.7% |
| csv_hash/1_special_false | 1,820 -> 1,788 | 831 -> 819 | 0/0 -> 0/0 | 0 -> 0 | +1.5% |
| csv_hash/1_special_true | 1,924 -> 1,831 | 879 -> 836 | 0/0 -> 0/0 | 0 -> 0 | +5.1% |
| csv_hash/32_special_false | 2,633 -> 2,180 | 1,209 -> 995 | 1/0 -> 0/0 | 207 -> 0 | +21.5% |
| csv_hash/32_special_true | 2,728 -> 2,359 | 1,250 -> 1,081 | 1/0 -> 0/0 | 207 -> 0 | +15.6% |
| csv_hash/32_long_false | 13,970 -> 7,448 | 6,383 -> 3,408 | 1/0 -> 0/0 | 32,799 -> 0 | +87.3% |
| csv_hash/32_long_true | 14,380 -> 8,914 | 6,577 -> 4,077 | 1/0 -> 0/0 | 32,799 -> 0 | +61.3% |

### Timing cleanup and unchanged pair controls

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| cleanup/pair_1_clean | 697 -> 699 | 320 -> 321 | 1/1 -> 1/1 | 27 -> 27 | -0.3% |
| cleanup/speed_1_clean | 898 -> 852 | 412 -> 391 | 1/1 -> 1/1 | 36 -> 36 | +5.4% |
| cleanup/pair_1_early | 800 -> 798 | 368 -> 366 | 2/0 -> 2/0 | 26 -> 26 | +0.5% |
| cleanup/speed_1_early | 1,357 -> 1,267 | 621 -> 580 | 3/1 -> 2/1 | 80 -> 64 | +7.1% |
| cleanup/pair_1_late | 801 -> 797 | 368 -> 366 | 2/0 -> 2/0 | 26 -> 26 | +0.5% |
| cleanup/speed_1_late | 1,378 -> 1,253 | 638 -> 575 | 3/1 -> 2/1 | 80 -> 64 | +11.0% |
| cleanup/pair_128_clean | 41,126 -> 43,495 | 18,808 -> 19,915 | 1/1 -> 1/1 | 4,521 -> 4,521 | -5.6% |
| cleanup/speed_128_clean | 69,766 -> 74,003 | 31,885 -> 33,829 | 1/1 -> 1/1 | 5,673 -> 5,673 | -5.7% |
| cleanup/pair_128_early | 46,015 -> 49,232 | 20,988 -> 22,506 | 2/1 -> 2/1 | 6,044 -> 6,044 | -6.7% |
| cleanup/speed_128_early | 72,210 -> 78,389 | 33,021 -> 35,828 | 3/1 -> 2/1 | 9,475 -> 7,580 | -7.8% |
| cleanup/pair_128_late | 54,622 -> 54,016 | 24,977 -> 24,648 | 2/1 -> 2/1 | 6,044 -> 6,044 | +1.3% |
| cleanup/speed_128_late | 148,411 -> 83,136 | 67,855 -> 37,990 | 3/2 -> 2/1 | 13,265 -> 7,580 | +78.6% |
| cleanup/pair_4096_clean | 1,501,571 -> 1,492,602 | 686,998 -> 682,498 | 1/1 -> 1/1 | 163,695 -> 163,695 | +0.7% |
| cleanup/speed_4096_clean | 2,326,453 -> 2,405,689 | 1,064,483 -> 1,100,143 | 1/1 -> 1/1 | 200,559 -> 200,559 | -3.2% |
| cleanup/pair_4096_early | 1,667,397 -> 1,798,899 | 762,581 -> 822,871 | 2/1 -> 2/1 | 218,276 -> 218,276 | -7.3% |
| cleanup/speed_4096_early | 2,424,180 -> 2,526,210 | 1,108,445 -> 1,155,477 | 3/1 -> 2/1 | 334,285 -> 267,428 | -4.1% |
| cleanup/pair_4096_late | 1,955,313 -> 1,962,358 | 894,658 -> 897,732 | 2/1 -> 2/1 | 218,276 -> 218,276 | -0.3% |
| cleanup/speed_4096_late | 4,946,542 -> 2,879,363 | 2,262,363 -> 1,318,346 | 3/2 -> 2/1 | 467,999 -> 267,428 | +71.6% |

### Four-chart speed-map analysis

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| speed_load/1_clean | 34,730 -> 36,453 | 15,838 -> 16,709 | 63/11 -> 63/11 | 9,214 -> 9,214 | -5.2% |
| speed_load/1_early | 60,796 -> 39,112 | 27,906 -> 17,981 | 65/11 -> 64/11 | 9,258 -> 9,242 | +55.2% |
| speed_load/1_late | 43,049 -> 36,484 | 19,668 -> 16,671 | 65/11 -> 64/11 | 9,258 -> 9,242 | +18.0% |
| speed_load/128_clean | 169,344 -> 173,978 | 77,489 -> 79,734 | 63/11 -> 63/11 | 20,867 -> 20,867 | -2.8% |
| speed_load/128_early | 164,932 -> 161,986 | 75,282 -> 74,176 | 65/11 -> 64/11 | 24,669 -> 22,774 | +1.5% |
| speed_load/128_late | 237,178 -> 180,681 | 108,346 -> 82,576 | 65/12 -> 64/11 | 28,459 -> 22,774 | +31.2% |
| speed_load/4096_clean | 3,962,622 -> 4,122,740 | 1,811,944 -> 1,885,900 | 63/11 -> 63/11 | 423,625 -> 423,625 | -3.9% |
| speed_load/4096_early | 4,014,715 -> 4,157,058 | 1,836,269 -> 1,901,745 | 65/11 -> 64/11 | 557,351 -> 490,494 | -3.4% |
| speed_load/4096_late | 6,320,722 -> 4,336,626 | 2,890,445 -> 1,982,692 | 65/12 -> 64/11 | 691,065 -> 490,494 | +45.8% |

### Unchanged analysis controls

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| analyze/fast_fake_lifts | 676,730 -> 666,303 | 309,628 -> 304,612 | 31/4 -> 31/4 | 59,028 -> 59,028 | +1.6% |
| analyze/camellia | 455,704,331 -> 460,807,384 | 208,393,350 -> 210,791,925 | 110/0 -> 110/0 | 5,263,624 -> 5,263,624 | -1.1% |
| analyze/fast_camellia | 58,132,928 -> 58,032,781 | 26,595,650 -> 26,542,800 | 115/0 -> 115/0 | 7,051,152 -> 7,051,152 | +0.2% |
| analyze/mixed_small | 48,444 -> 48,114 | 22,160 -> 22,240 | 59/3 -> 59/3 | 7,460 -> 7,460 | -0.4% |

### Course report composition and controls

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| course_report/0_Full | 12,567 -> 13,264 | 5,746 -> 6,058 | 0/0 -> 0/0 | 0 -> 0 | -5.2% |
| course_report/0_Pretty | 5,675 -> 5,886 | 2,590 -> 2,689 | 0/0 -> 0/0 | 0 -> 0 | -3.7% |
| course_report/0_JSON | 27,340 -> 27,604 | 12,495 -> 12,627 | 6/0 -> 6/0 | 106 -> 106 | -1.0% |
| course_report/0_CSV | 2,006 -> 1,721 | 917 -> 787 | 1/0 -> 0/0 | 16 -> 0 | +16.5% |
| course_report/16_Full | 12,500 -> 12,883 | 5,711 -> 5,888 | 0/0 -> 0/0 | 0 -> 0 | -3.0% |
| course_report/16_Pretty | 5,767 -> 5,645 | 2,630 -> 2,579 | 0/0 -> 0/0 | 0 -> 0 | +2.0% |
| course_report/16_JSON | 28,065 -> 27,429 | 12,833 -> 12,534 | 6/0 -> 6/0 | 106 -> 106 | +2.4% |
| course_report/16_CSV | 2,008 -> 1,755 | 921 -> 801 | 1/0 -> 0/0 | 16 -> 0 | +15.0% |
| course_report/4096_Full | 12,919 -> 13,138 | 5,901 -> 6,004 | 0/0 -> 0/0 | 0 -> 0 | -1.7% |
| course_report/4096_Pretty | 5,838 -> 5,946 | 2,667 -> 2,723 | 0/0 -> 0/0 | 0 -> 0 | -2.1% |
| course_report/4096_JSON | 36,015 -> 34,501 | 16,473 -> 15,778 | 6/0 -> 6/0 | 106 -> 106 | +4.4% |
| course_report/4096_CSV | 2,013 -> 1,800 | 919 -> 822 | 1/0 -> 0/0 | 16 -> 0 | +11.8% |

### Focused 128-entry cleanup repeat

| Case | Thread cycles, old -> new | ns, old -> new | Alloc/realloc, old -> new | Requested bytes, old -> new | Throughput |
|---|---:|---:|---:|---:|---:|
| cleanup/pair_128_clean | 43,489 -> 43,301 | 19,877 -> 19,798 | 1/1 -> 1/1 | 4,521 -> 4,521 | +0.4% |
| cleanup/pair_128_early | 50,023 -> 48,499 | 22,876 -> 22,162 | 2/1 -> 2/1 | 6,044 -> 6,044 | +3.2% |
| cleanup/pair_128_late | 57,309 -> 53,178 | 26,202 -> 24,302 | 2/1 -> 2/1 | 6,044 -> 6,044 | +7.8% |
| cleanup/speed_128_clean | 72,787 -> 73,734 | 33,293 -> 33,746 | 1/1 -> 1/1 | 5,673 -> 5,673 | -1.3% |
| cleanup/speed_128_early | 77,287 -> 76,147 | 35,351 -> 34,808 | 3/1 -> 2/1 | 9,475 -> 7,580 | +1.6% |
| cleanup/speed_128_late | 147,839 -> 86,656 | 67,668 -> 39,635 | 3/2 -> 2/1 | 13,265 -> 7,580 | +70.7% |


## Interpretation and limits

No semantic/output regression was observed, and no measured allocation,
reallocation, or requested-byte count increases. CPU throughput is not a
universal win. The main run's 128-entry early-dirty speed case was 7.8% slower;
the focused repeat was 1.6% faster. Clean 128-entry speeds moved from -5.7% to
-1.3%. Unchanged 128-entry pair controls also changed substantially: early
dirtiness moved from -6.7% to +3.2% and late dirtiness from +1.3% to +7.8%.
These repeats support the allocation and large late-dirty CPU gains while
limiting conclusions about small timing differences on this shared host.

Main-run large clean/early speed cases remain 3.2-4.1% slower in the leaf and
3.4-3.9% slower in composition. The unchanged large early-dirty pair control is
7.3% slower. Large generic pack sorting ranges from +0.6% to -3.7%; the
unchanged 1/4-entry controls include percentages amplified by one nanosecond.
Empty buffered CSV is 5.5% slower with unchanged allocation counts; course
Full/Pretty/JSON controls range from -5.2% to +4.4%. All slower cases are
reported above. No disk/console throughput or process-RSS improvement is
claimed, and filesystem scanning is outside the pack sort benchmark.


## Regression checks

- 77 rssp release library tests, 140 rssp-core release library tests, and 29
  optimization edge integration tests pass: 246 total, zero failures.
- New tests check stable pack permutations at sizes 0/1/4/5/32/257, equal and
  mixed-case names, Unicode, and exact record identity. Speed cleanup is compared
  with the owned production implementation for empty/malformed fields, early and
  late dirty entries, whitespace, controls, nonfinite values, signed zero, and
  extra fields. CSV tests check explicit hash bytes and failure prefixes at every
  byte boundary inside both hash fields, including Unicode; no writes follow an
  error. Existing all-mode course error tests also pass.
- Component and full-corpus comparisons produce 169,095,313 byte-identical
  UTF-8 bytes, including explicit timing snapshots, hashes, statistics, and
  reports. The corpus contains 30,843 files and 56,125 supported charts, with
  30,489 successful analyses and 354 matching errors. SHA-256:
  2f783d1a4404df0de9d27fa0d3b2b9b728b4c9e9cdf6849ab59a2b54e56a714b.
- 57 additional course-report, course-CSV, and pack-order cases produce
  29,124,186 byte-identical UTF-8 bytes. SHA-256:
  aac4e521dea9affd1f0e1baa95991e7dd0fa7f4bb10d109b82bed5ff84d290d7.
- cargo fmt --all -- --check, git diff --check, and strict release workspace
  Clippy for all targets pass.
- After benchmark confirmation and before committing, the required command ran:

```powershell
cargo test --release --test all_parity -- --test-threads=22
```

```text
test result: ok. 30489 passed; 0 failed
```

## Pass 0.4.284: canonical timing comparisons, bounded radar parsing, course meters

Baseline: `283e844` production code, built at the same `0.4.284` version with the new tests and benchmark fixtures. The three production changes remove roughly 20 lines. All original parsing, quantization, stable duplicate handling, error paths, and report fields remain covered.

- Timing cleanup compares already-canonical beats instead of converting both operands back to note rows. The original input quantization and unordered insertion paths remain intact. Canonical row values are finite and injective across the `i32` domain; boundary and sampled round-trip tests verify the comparison invariant.
- Radar parsing fills the first 14 successfully parsed values and, for SSC, requires another 14 successfully parsed values. It then stops. Invalid fields still do not count; the first five categories still permit non-finite values; later categories still require finite nonnegative values. The original whole-field cleanup remains intact.
- Course averaging reads ratings from the already-owned entry summaries. This deletes the separate integer vector and its per-entry writes. Explicit course meters now bypass rating parsing entirely. Integer summation, empty-input behavior, and signed rounding remain unchanged.

Measurement: Rust 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc, Intel Xeon E5-2696 v4 (44 logical CPUs), thread pinned to CPU 2. Fat LTO and one codegen unit. Each process performs four warmups and seven batches; the tables use the median of three alternating original/optimized process pairs. Thread CPU cycles and wall time are measured together; allocation/reallocation counts and requested bytes are measured in a separate invocation. Fixture creation and owned timing input clones are outside measurement. Timing outputs stay alive until after the measured batch. Full course loading intentionally measures the real worker boundary, including warm filesystem I/O; no disk-throughput claim is made. My builds, tests, and corpus comparisons were complete before timed runs.

Allocation bytes are cumulative requested bytes, including reallocations, rather than resident memory or a sampled peak. Microsecond-scale and empty controls can show large percentage changes from a few nanoseconds; see every case below.

Confirmed gains:

- Ordered 4,096-segment cleanup: rows 120,169 → 53,199 CPU cycles (−55.7%); scrolls 121,786 → 57,633 (−52.7%); speeds 135,790 → 72,669 (−46.5%). All three reuse the input allocation. The composed 32-segment timing path improves call throughput by 7.6%; full parsing limits the large composed gains to 1.0–3.9%.
- A 4,096-value plain radar list: 536,842 → 228,337 CPU cycles (−57.5%), call throughput +134.5%. Complete SM/SSC chart analysis improves throughput by 72.3–128.6% across the four long radar fixtures, without extra allocations.
- Every course case eliminates exactly one allocation and 4 bytes per entry in every individual run, with unchanged reallocation counts. For 256 entries, allocation calls drop 2,414 → 2,413 and requested bytes drop 210,559 → 209,535 (implicit meter), or 210,576 → 209,552 (explicit meter). This is a scratch-allocation improvement; course CPU throughput is not claimed to improve.

Limits and slower samples: the initial 28-value dirty SSC radar leaf was 12.1% slower in wall throughput. A longer-batch recheck of that exact case is +10.1%, with the neighboring dirty cases +12.7–35.8% and the plain SSC case +10.3%; both sets are retained below. The unchanged empty compaction control varies by 1 ns (5 → 6 ns). Reversed composed timing cases are −3.1–3.5% in the main run; their unordered algorithm is unchanged. The 28-value plain SSC analysis is −2.4%, and full course loading ranges from −4.6% to +2.3% amid filesystem and allocation timing variance. Existing full-analysis controls are +0.6–4.6% in wall throughput. These measurements establish the listed CPU and allocation wins, rather than a universal throughput or RSS improvement. All 117 main/recheck cases have non-increasing allocation calls, reallocations, and requested bytes.

Iterations per batch: timing leaves 5,000 (4,096 segments: 30); compaction 10,000 (4,096: 500); conversion control 20,000; radar leaves 5,000 (4,096 values: 100); composed timing 1,000 (4,096: 20); radar chart analysis 300; course loading 100; existing analysis controls 100 (Camellia callers use 10). The focused recheck uses 20,000 iterations. Slow reversed fixtures deliberately use smaller batches; fixture contents and iteration counts match within every original/optimized pair.

### Timing cleanup leaves

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `tidy/rows/0_ordered` | 37 → 29 | 17 → 14 | +21.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/0_duplicates` | 37 → 29 | 17 → 14 | +21.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/0_reverse` | 37 → 29 | 17 → 14 | +21.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/1_ordered` | 71 → 51 | 33 → 24 | +37.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/1_duplicates` | 71 → 51 | 33 → 24 | +37.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/1_reverse` | 72 → 53 | 33 → 26 | +26.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/32_ordered` | 934 → 448 | 430 → 205 | +109.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/32_duplicates` | 963 → 481 | 440 → 220 | +100.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/32_reverse` | 1,187 → 1,157 | 544 → 530 | +2.6% | 2 → 2 | 0 → 0 | 768 → 768 |
| `tidy/rows/4096_ordered` | 120,169 → 53,199 | 54,897 → 24,330 | +125.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/4096_duplicates` | 119,942 → 50,829 | 54,877 → 23,350 | +135.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/rows/4096_reverse` | 112,962 → 116,159 | 51,690 → 53,703 | -3.7% | 2 → 2 | 0 → 0 | 98,304 → 98,304 |
| `tidy/scrolls/0_ordered` | 24 → 22 | 11 → 10 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/0_duplicates` | 24 → 22 | 11 → 10 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/0_reverse` | 24 → 22 | 11 → 10 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/1_ordered` | 49 → 46 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/1_duplicates` | 49 → 46 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/1_reverse` | 50 → 47 | 23 → 22 | +4.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/32_ordered` | 985 → 554 | 450 → 254 | +77.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/32_duplicates` | 975 → 518 | 448 → 238 | +88.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/32_reverse` | 9,010 → 8,583 | 4,119 → 3,924 | +5.0% | 1 → 1 | 0 → 0 | 512 → 512 |
| `tidy/scrolls/4096_ordered` | 121,786 → 57,633 | 56,127 → 26,663 | +110.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/4096_duplicates` | 122,774 → 62,689 | 56,310 → 28,873 | +95.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/4096_reverse` | 12,766,537 → 12,672,913 | 5,845,670 → 5,801,183 | +0.8% | 1 → 1 | 0 → 0 | 65,536 → 65,536 |
| `tidy/speeds/0_ordered` | 25 → 21 | 12 → 10 | +20.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/0_duplicates` | 25 → 21 | 11 → 10 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/0_reverse` | 25 → 21 | 11 → 10 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/1_ordered` | 53 → 49 | 25 → 23 | +8.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/1_duplicates` | 53 → 49 | 25 → 23 | +8.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/1_reverse` | 54 → 51 | 25 → 23 | +8.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/32_ordered` | 1,127 → 653 | 516 → 301 | +71.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/32_duplicates` | 1,266 → 701 | 581 → 321 | +81.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/32_reverse` | 9,681 → 9,544 | 4,432 → 4,365 | +1.5% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `tidy/speeds/4096_ordered` | 135,790 → 72,669 | 62,657 → 33,210 | +88.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/4096_duplicates` | 132,871 → 75,859 | 61,153 → 34,670 | +76.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/4096_reverse` | 24,195,515 → 24,256,945 | 11,086,393 → 11,103,293 | -0.2% | 1 → 1 | 0 → 0 | 131,072 → 131,072 |

### Canonical compaction leaf

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `compact/0_ordered` | 10 → 10 | 5 → 5 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `compact/0_duplicates` | 10 → 11 | 5 → 6 | -16.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `compact/1_ordered` | 19 → 11 | 9 → 5 | +80.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `compact/1_duplicates` | 19 → 11 | 9 → 5 | +80.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `compact/32_ordered` | 642 → 155 | 295 → 71 | +315.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `compact/32_duplicates` | 662 → 194 | 303 → 89 | +240.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `compact/4096_ordered` | 82,801 → 20,532 | 37,887 → 9,414 | +302.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `compact/4096_duplicates` | 86,434 → 24,164 | 39,545 → 11,039 | +258.2% | 0 → 0 | 0 → 0 | 0 → 0 |

### Unchanged conversion control

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `row_convert/control` | 9 → 8 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |

### Radar parser leaf

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `radar/0_plain_false` | 11 → 11 | 5 → 5 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/0_plain_true` | 11 → 11 | 5 → 5 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/0_dirty_false` | 11 → 11 | 5 → 5 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/0_dirty_true` | 11 → 11 | 5 → 5 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/0_sparse_false` | 11 → 11 | 5 → 5 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/0_sparse_true` | 11 → 11 | 5 → 5 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/14_plain_false` | 1,744 → 1,471 | 797 → 672 | +18.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/14_plain_true` | 1,691 → 1,398 | 774 → 638 | +21.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/14_dirty_false` | 3,055 → 2,533 | 1,398 → 1,156 | +20.9% | 1 → 1 | 0 → 0 | 115 → 115 |
| `radar/14_dirty_true` | 2,988 → 2,440 | 1,371 → 1,114 | +23.1% | 1 → 1 | 0 → 0 | 115 → 115 |
| `radar/14_sparse_false` | 5,177 → 4,460 | 2,373 → 2,040 | +16.3% | 1 → 1 | 0 → 0 | 101 → 101 |
| `radar/14_sparse_true` | 5,131 → 4,242 | 2,347 → 1,942 | +20.9% | 1 → 1 | 0 → 0 | 101 → 101 |
| `radar/28_plain_false` | 3,722 → 2,309 | 1,703 → 1,054 | +61.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/28_plain_true` | 3,841 → 3,673 | 1,759 → 1,685 | +4.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/28_dirty_false` | 5,718 → 4,206 | 2,615 → 1,924 | +35.9% | 1 → 1 | 0 → 0 | 241 → 241 |
| `radar/28_dirty_true` | 5,580 → 6,340 | 2,551 → 2,901 | -12.1% | 1 → 1 | 0 → 0 | 241 → 241 |
| `radar/28_sparse_false` | 10,173 → 7,176 | 4,657 → 3,283 | +41.9% | 1 → 1 | 0 → 0 | 213 → 213 |
| `radar/28_sparse_true` | 9,516 → 9,378 | 4,353 → 4,288 | +1.5% | 1 → 1 | 0 → 0 | 213 → 213 |
| `radar/128_plain_false` | 16,572 → 7,755 | 7,579 → 3,543 | +113.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/128_plain_true` | 16,775 → 8,817 | 7,671 → 4,028 | +90.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/128_dirty_false` | 23,718 → 14,380 | 10,849 → 6,574 | +65.0% | 1 → 1 | 0 → 0 | 1,131 → 1,131 |
| `radar/128_dirty_true` | 23,605 → 15,531 | 10,812 → 7,094 | +52.4% | 1 → 1 | 0 → 0 | 1,131 → 1,131 |
| `radar/128_sparse_false` | 41,044 → 24,011 | 18,797 → 10,968 | +71.4% | 1 → 1 | 0 → 0 | 1,003 → 1,003 |
| `radar/128_sparse_true` | 40,998 → 26,303 | 18,736 → 12,018 | +55.9% | 1 → 1 | 0 → 0 | 1,003 → 1,003 |
| `radar/4096_plain_false` | 536,842 → 228,337 | 245,402 → 104,643 | +134.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/4096_plain_true` | 555,833 → 224,287 | 254,142 → 102,518 | +147.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `radar/4096_dirty_false` | 771,343 → 466,951 | 352,644 → 213,532 | +65.1% | 1 → 1 | 0 → 0 | 36,453 → 36,453 |
| `radar/4096_dirty_true` | 766,336 → 452,965 | 350,167 → 207,187 | +69.0% | 1 → 1 | 0 → 0 | 36,453 → 36,453 |
| `radar/4096_sparse_false` | 1,352,190 → 749,981 | 618,549 → 342,820 | +80.4% | 1 → 1 | 0 → 0 | 32,357 → 32,357 |
| `radar/4096_sparse_true` | 1,348,529 → 749,634 | 617,004 → 344,245 | +79.2% | 1 → 1 | 0 → 0 | 32,357 → 32,357 |

### Composed timing parsing and cleanup

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `timing_cleanup/0_ordered` | 934 → 928 | 430 → 429 | +0.2% | 2 → 2 | 0 → 0 | 24 → 24 |
| `timing_cleanup/0_duplicates` | 911 → 918 | 416 → 419 | -0.7% | 2 → 2 | 0 → 0 | 24 → 24 |
| `timing_cleanup/0_reverse` | 920 → 923 | 420 → 429 | -2.1% | 2 → 2 | 0 → 0 | 24 → 24 |
| `timing_cleanup/1_ordered` | 4,376 → 4,384 | 1,997 → 2,004 | -0.3% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/1_duplicates` | 4,633 → 4,371 | 2,130 → 2,008 | +6.1% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/1_reverse` | 4,241 → 4,318 | 1,935 → 1,972 | -1.9% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/32_ordered` | 53,362 → 49,587 | 24,378 → 22,661 | +7.6% | 14 → 14 | 0 → 0 | 5,528 → 5,528 |
| `timing_cleanup/32_duplicates` | 51,623 → 49,826 | 23,580 → 22,795 | +3.4% | 14 → 14 | 0 → 0 | 4,600 → 4,600 |
| `timing_cleanup/32_reverse` | 69,363 → 71,734 | 31,760 → 32,787 | -3.1% | 24 → 24 | 0 → 0 | 10,136 → 10,136 |
| `timing_cleanup/4096_ordered` | 7,366,409 → 7,285,501 | 3,368,330 → 3,333,955 | +1.0% | 14 → 14 | 0 → 0 | 731,704 → 731,704 |
| `timing_cleanup/4096_duplicates` | 7,033,494 → 6,762,575 | 3,216,120 → 3,094,630 | +3.9% | 14 → 14 | 0 → 0 | 611,032 → 611,032 |
| `timing_cleanup/4096_reverse` | 43,823,120 → 45,256,916 | 20,063,180 → 20,781,785 | -3.5% | 24 → 24 | 0 → 0 | 1,321,528 → 1,321,528 |

### Composed radar chart analysis

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `radar_load/14_plain_sm` | 14,604 → 14,237 | 6,683 → 6,502 | +2.8% | 24 → 24 | 4 → 4 | 2,517 → 2,517 |
| `radar_load/14_plain_ssc` | 14,460 → 13,291 | 6,603 → 6,074 | +8.7% | 22 → 22 | 4 → 4 | 2,493 → 2,493 |
| `radar_load/14_dirty_sm` | 14,589 → 14,150 | 6,658 → 6,462 | +3.0% | 25 → 25 | 4 → 4 | 2,604 → 2,604 |
| `radar_load/14_dirty_ssc` | 15,402 → 14,896 | 7,036 → 6,840 | +2.9% | 23 → 23 | 4 → 4 | 2,580 → 2,580 |
| `radar_load/28_plain_sm` | 16,136 → 14,993 | 7,367 → 6,867 | +7.3% | 24 → 24 | 4 → 4 | 2,517 → 2,517 |
| `radar_load/28_plain_ssc` | 16,195 → 16,544 | 7,403 → 7,583 | -2.4% | 22 → 22 | 4 → 4 | 2,493 → 2,493 |
| `radar_load/28_dirty_sm` | 17,370 → 15,426 | 7,946 → 7,112 | +11.7% | 25 → 25 | 4 → 4 | 2,702 → 2,702 |
| `radar_load/28_dirty_ssc` | 18,195 → 17,627 | 8,305 → 8,054 | +3.1% | 23 → 23 | 4 → 4 | 2,678 → 2,678 |
| `radar_load/4096_plain_sm` | 643,428 → 286,855 | 294,317 → 131,235 | +124.3% | 24 → 24 | 4 → 4 | 2,517 → 2,517 |
| `radar_load/4096_plain_ssc` | 580,402 → 253,961 | 265,419 → 116,129 | +128.6% | 22 → 22 | 4 → 4 | 2,493 → 2,493 |
| `radar_load/4096_dirty_sm` | 806,942 → 468,389 | 369,346 → 214,421 | +72.3% | 25 → 25 | 4 → 4 | 30,778 → 30,778 |
| `radar_load/4096_dirty_ssc` | 733,867 → 394,943 | 335,823 → 180,663 | +85.9% | 23 → 23 | 4 → 4 | 30,754 → 30,754 |

### Full course loading

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `course_load/1_false` | 1,434,959 → 1,457,702 | 657,902 → 667,923 | -1.5% | 82 → 81 | 11 → 11 | 8,698 → 8,694 |
| `course_load/1_true` | 1,432,931 → 1,501,742 | 656,498 → 687,853 | -4.6% | 82 → 81 | 11 → 11 | 8,715 → 8,711 |
| `course_load/8_false` | 2,026,341 → 2,006,809 | 928,637 → 919,388 | +1.0% | 180 → 179 | 18 → 18 | 20,847 → 20,815 |
| `course_load/8_true` | 2,031,264 → 1,985,860 | 930,277 → 909,746 | +2.3% | 180 → 179 | 18 → 18 | 20,864 → 20,832 |
| `course_load/32_false` | 2,094,647 → 2,112,870 | 958,915 → 967,729 | -0.9% | 396 → 395 | 18 → 18 | 34,743 → 34,615 |
| `course_load/32_true` | 2,131,303 → 2,112,154 | 976,188 → 966,719 | +1.0% | 396 → 395 | 18 → 18 | 34,760 → 34,632 |
| `course_load/256_false` | 3,191,495 → 3,222,003 | 1,461,543 → 1,475,101 | -0.9% | 2,414 → 2,413 | 20 → 20 | 210,559 → 209,535 |
| `course_load/256_true` | 3,006,639 → 3,020,763 | 1,376,455 → 1,389,592 | -0.9% | 2,414 → 2,413 | 20 → 20 | 210,576 → 209,552 |

### Existing analysis controls

| Case | CPU cycles, old → new | ns, old → new | Throughput change | Allocs, old → new | Reallocs, old → new | Requested bytes, old → new |
|---|---:|---:|---:|---:|---:|---:|
| `analyze/fast_fake_lifts` | 694,728 → 665,649 | 318,414 → 304,517 | +4.6% | 31 → 31 | 4 → 4 | 59,028 → 59,028 |
| `analyze/camellia` | 461,662,037 → 459,672,116 | 212,325,200 → 210,266,240 | +1.0% | 110 → 110 | 0 → 0 | 5,263,624 → 5,263,624 |
| `analyze/fast_camellia` | 60,089,442 → 59,357,520 | 27,498,030 → 27,157,040 | +1.3% | 115 → 115 | 0 → 0 | 7,051,152 → 7,051,152 |
| `analyze/mixed_small` | 50,911 → 51,005 | 23,426 → 23,289 | +0.6% | 59 → 59 | 3 → 3 | 7,460 → 7,460 |

### Longer-batch radar recheck and control

| Case | CPU cycles, old → new | ns, old → new | Throughput change |
|---|---:|---:|---:|
| `row_convert/control` | 9 → 8 | 4 → 4 | +0.0% |
| `radar/14_dirty_false` | 2,888 → 2,511 | 1,321 → 1,148 | +15.1% |
| `radar/14_dirty_true` | 2,768 → 2,453 | 1,264 → 1,122 | +12.7% |
| `radar/28_dirty_false` | 5,664 → 4,175 | 2,589 → 1,907 | +35.8% |
| `radar/28_dirty_true` | 5,725 → 5,202 | 2,618 → 2,378 | +10.1% |
| `radar/28_plain_true` | 3,818 → 3,461 | 1,746 → 1,583 | +10.3% |

### Validation and reproduction

- 142 core + 79 library + 29 integration release tests pass (250 total). New regression coverage includes canonical row limits, quantization ties, non-finite input beats and values, ordered/reversed duplicates, sparse radar fields, missing second-player radar data, UTF-8 failures, signed zero, invalid ratings, signed averages, Unicode trimming, and course meter overrides.
- Strict release workspace/all-target Clippy, `cargo fmt --all -- --check`, and `git diff --check` pass.
- After confirming the optimizations, `cargo test --release --test all_parity -- --test-threads=22`: 30,489 passed, zero failed.
- Original and optimized component + full-corpus output are byte-identical: 170,466,481 UTF-8 bytes, SHA-256 `ca0aaa6f2fae2b443d6c4e6138c88d4c93b2e416ffd66427506db19581595892`. This covers 30,843 files, 56,125 supported charts, 30,489 successful files and 354 matching errors, plus the added composed timing, radar and course fixtures.
- Additional canonical core cleanup traces: 24 rows, 1,493,981 UTF-8 bytes, SHA-256 `0ab60debbb3a62439c0a443f1af9735abaadcc552572e03d1abdd3b34bd84f9f`. Existing CSV/pack/course traces: 57 rows, 29,124,186 bytes, SHA-256 `aac4e521dea9affd1f0e1baa95991e7dd0fa7f4bb10d109b82bed5ff84d290d7`. Both are byte-identical.

Build and save the unchanged production implementation at the same patch version before applying the three edits. Build the optimized binaries with the same harness and profile. Run ignored unit benchmarks alone with `--test-threads=1`:

```powershell
cargo test --release -p rssp-core --lib
cargo test --release -p rssp --lib
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_PASS_FILTER="tidy/rows/32_"; $env:RSSP_PASS_ITERS="5000"
.\saved-core.exe pass_edges --skip _trace --ignored --nocapture --test-threads=1
$env:RSSP_PASS_FILTER="radar/28_"; $env:RSSP_PASS_ITERS="5000"
.\saved-lib.exe pass_edges --skip _trace --ignored --nocapture --test-threads=1
$env:RSSP_HOT_FILTER="course_load/"; $env:RSSP_HOT_ITERS="100"
.\saved-hotpath.exe
$env:RSSP_HOT_VERIFY="1"; .\saved-hotpath.exe
```

Local raw runs, saved binaries, fixture outputs, and pairing scripts are under ignored `target/perf-284/`. `rust-performance.md`, `optimize.sh`, and `optimize.ps1` are excluded from the commit.

## Pass 0.4.285: unordered timing searches, blank timing tags, final BPM cache

Baseline: `b6a6af7` production code, built at `0.4.285` with the same fixtures and measurement harness. Helper-call argument adapters reflect the removed row argument and the new final-chart argument. The patch version moves exactly once from `0.4.284` to `0.4.285`.

- Unordered scroll/speed insertion compares already-canonical beats and reuses the first partition position. Every mutating merge branch returns before insertion, so that position remains valid. The original input quantization, duplicate handling, merge behavior, and insertion order remain intact. The unused speed-row conversion helper is removed.
- Optional chart timing tags containing only space or bytes `0x09..=0x0D` return `None` before UTF-8 validation and cleanup. This removes temporary strings whose contents would immediately be discarded. End-byte checks avoid a scan for ordinary nonblank tags; short inputs use a short-circuit check and long inputs use a vectorizable boolean reduction. The guard retains the original input for nonblank tags; boundary control markers must still influence cleanup of internal controls. Both owned and borrowed tag APIs share this decoding boundary.
- The final physical chart takes the call-local global BPM cache when it inherits global timing. A single inherited chart computes its result directly. Earlier charts keep their copies, and a final chart with its own timing uses the original chart-timing path. The original native `f32` precision conversion remains intact.

Measurement: Rust 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc, Intel Xeon E5-2696 v4 (44 logical CPUs), thread pinned to CPU 2. Fat LTO, one codegen unit. Four warmups and seven batches per process; tables report the median of three alternating original/optimized process pairs. Thread CPU cycles and wall time are measured together; allocation/reallocation calls and requested bytes are measured separately. My builds, tests, and corpus verification finish before timed runs. Other user build activity can affect wall time and cache residency.

Fixture generation, normalization used as input, parser setup for the cached-chart leaf, and owned input clones are outside measurement. Prepared leaf outputs remain alive until after the timer on both implementations. Complete caller benchmarks include their real parsing, allocation, and destruction work. Requested bytes are cumulative allocation/reallocation requests, not RSS or a sampled live-memory peak. Taking the formatter buffer also retains its original spare capacity in the final result.

Confirmed results:

- A 32-segment append to unordered scroll storage: 336 → 108 CPU cycles (-67.9%); call throughput +206.0%. Complete reversed 4,096-segment cleanup: scrolls 11,508,882 → 9,267,356 CPU cycles (-19.5%), speeds 23,323,411 → 21,851,591 CPU cycles (-6.3%). The composed 32-segment reversed timing caller is +24.6% in throughput.
- A 24,576-byte blank owned timing tag: 79,621 → 2,535 CPU cycles (-96.8%), one allocation / 24,576 requested bytes → zero. Blank two-chart analysis removes 12 cleanup allocations; duration and peak callers remove four each. The 4,096-repeat analysis caller improves throughput by +227.0%.
- Taking a cached 4,096-segment final SSC BPM result: 227,600 → 99,031 CPU cycles (-56.5%), five → four allocations and 166,506 → 70,981 requested bytes. Each eligible final chart eliminates one formatted-string clone. The complete global SSC snapshot caller is -1.2% in throughput.

All 210 final cases have non-increasing allocation calls, reallocation calls, and requested bytes. Cases with unchanged code and tiny controls remain in the tables; gains are specific to the measured paths, rather than a universal throughput or resident-memory claim.

Limits and slower samples: the main run contains sizable slowdowns in several unchanged raw-cleanup/native-timing controls and tiny callers. A 42-case longer-batch recheck reduces the four 4,096-entry raw-cleanup controls to −2.2%…+2.7%, and the one-entry composed timing controls improve by 9.5–21.6%. Remaining focused slower samples include one-entry raw cleanup (−7.1…−14.2%), 128-entry clean raw cleanup (−6.6%), native SSC timing with stops (−7.0%), a nonfinal cached SSC chart (−7.9%), nonblank owned-tag first/last dirty leaves (−9.3%/−9.1%), and a tiny local SSC snapshot caller (−6.2%). Their outputs and allocation metrics are unchanged. These results establish the target CPU/allocation gains and behavioral parity; they do not establish a CPU win for every input. Both main and recheck samples remain below.

Discarded experiments: rebuilding borrowed timing maps from a verified prefix improved dirty suffixes but repeatedly slowed clean and early-dirty maps by roughly 16–29%; the original implementation is retained. Reusing the `f64` BPM vector after native rounding saved an allocation but slowed large SSC snapshot leaves by about 8%, and retained a larger working buffer; that change is absent. The first cache leaf benchmark destroyed the transferred output inside the timer only on the optimized implementation; both versions now retain outputs until after measurement. An initial blank guard using Rust ASCII trim missed vertical tabs. A scalar full scan eliminated the allocation but slowed long owned tags by about 19% and analysis by about 29%; the final explicit whitespace range, vectorizable reduction, and short-input guard replace it.

Iterations per batch: direct insertion 10,000 (4,096 entries: 100); tidiers 5,000 (4,096: 30); unchanged raw cleanup 5,000 (4,096: 500); BPM timing and cached-chart leaves 1,000 (4,096: 30); timing-tag leaves 5,000 (4,096: 100); blank callers 100; other composed callers 300 (4,096: 20); timing cleanup 300 (4,096: 20); analysis controls 100, with Camellia callers using 10.

### Unordered insertion leaves

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `insert/scrolls/1_append` | 67 → 27 | 31 → 13 | +138.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/1_middle` | 58 → 39 | 26 → 18 | +44.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/1_replace` | 56 → 27 | 26 → 13 | +100.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/32_append` | 336 → 108 | 153 → 50 | +206.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/32_middle` | 386 → 164 | 176 → 75 | +134.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/32_replace` | 237 → 125 | 109 → 57 | +91.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/4096_append` | 1,122 → 687 | 527 → 327 | +61.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/4096_middle` | 4,928 → 4,484 | 2,265 → 2,063 | +9.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/scrolls/4096_replace` | 845 → 641 | 404 → 309 | +30.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/1_append` | 63 → 33 | 29 → 15 | +93.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/1_middle` | 58 → 38 | 27 → 17 | +58.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/1_replace` | 61 → 30 | 28 → 14 | +100.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/32_append` | 415 → 157 | 189 → 72 | +162.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/32_middle` | 458 → 308 | 209 → 141 | +48.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/32_replace` | 281 → 192 | 128 → 88 | +45.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/4096_append` | 1,271 → 1,124 | 597 → 536 | +11.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/4096_middle` | 11,495 → 10,745 | 5,288 → 4,920 | +7.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `insert/speeds/4096_replace` | 983 → 928 | 464 → 521 | -10.9% | 0 → 0 | 0 → 0 | 0 → 0 |

### Timing tidiers and ordered controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `tidy/scrolls/0_ordered` | 20 → 20 | 10 → 9 | +11.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/0_duplicates` | 22 → 20 | 10 → 9 | +11.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/0_reverse` | 21 → 20 | 10 → 9 | +11.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/1_ordered` | 89 → 86 | 41 → 40 | +2.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/1_duplicates` | 92 → 90 | 43 → 42 | +2.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/1_reverse` | 88 → 82 | 41 → 39 | +5.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/32_ordered` | 425 → 434 | 195 → 199 | -2.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/32_duplicates` | 473 → 506 | 218 → 232 | -6.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/32_reverse` | 8,142 → 2,865 | 3,716 → 1,309 | +183.9% | 1 → 1 | 0 → 0 | 512 → 512 |
| `tidy/scrolls/4096_ordered` | 53,229 → 48,905 | 24,413 → 22,447 | +8.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/4096_duplicates` | 59,097 → 58,511 | 27,087 → 26,730 | +1.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/scrolls/4096_reverse` | 11,508,882 → 9,267,356 | 5,252,927 → 4,230,773 | +24.2% | 1 → 1 | 0 → 0 | 65,536 → 65,536 |
| `tidy/speeds/0_ordered` | 21 → 19 | 10 → 9 | +11.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/0_duplicates` | 20 → 20 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/0_reverse` | 20 → 19 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/1_ordered` | 50 → 48 | 23 → 22 | +4.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/1_duplicates` | 50 → 46 | 23 → 22 | +4.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/1_reverse` | 47 → 48 | 22 → 23 | -4.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/32_ordered` | 656 → 586 | 301 → 268 | +12.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/32_duplicates` | 688 → 622 | 315 → 288 | +9.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/32_reverse` | 9,010 → 3,801 | 4,112 → 1,737 | +136.7% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `tidy/speeds/4096_ordered` | 66,991 → 64,811 | 30,710 → 29,643 | +3.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/4096_duplicates` | 73,284 → 65,155 | 33,577 → 29,743 | +12.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `tidy/speeds/4096_reverse` | 23,323,411 → 21,851,591 | 10,644,627 → 9,974,250 | +6.7% | 1 → 1 | 0 → 0 | 131,072 → 131,072 |

### Unchanged raw cleanup controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `raw_map/0_clean` | 4 → 5 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/0_first` | 4 → 4 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/0_middle` | 4 → 4 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/0_last` | 4 → 4 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/1_clean` | 49 → 46 | 22 → 21 | +4.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/1_first` | 277 → 288 | 126 → 131 | -3.8% | 1 → 1 | 0 → 0 | 13 → 13 |
| `raw_map/1_middle` | 272 → 306 | 124 → 139 | -10.8% | 1 → 1 | 0 → 0 | 13 → 13 |
| `raw_map/1_last` | 260 → 299 | 119 → 136 | -12.5% | 1 → 1 | 0 → 0 | 13 → 13 |
| `raw_map/128_clean` | 7,555 → 8,544 | 3,446 → 3,900 | -11.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/128_first` | 9,358 → 9,917 | 4,272 → 4,525 | -5.6% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `raw_map/128_middle` | 12,597 → 13,213 | 5,746 → 6,033 | -4.8% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `raw_map/128_last` | 17,184 → 18,514 | 7,840 → 8,449 | -7.2% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `raw_map/4096_clean` | 238,028 → 278,203 | 108,559 → 126,982 | -14.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/4096_first` | 316,030 → 323,178 | 144,234 → 147,520 | -2.2% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `raw_map/4096_middle` | 389,847 → 493,493 | 177,890 → 225,276 | -21.0% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `raw_map/4096_last` | 487,304 → 529,200 | 222,377 → 241,505 | -7.9% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |

### Unchanged native BPM timing controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `bpm_snapshot/0_sm_plain` | 1,668 → 1,605 | 761 → 737 | +3.3% | 5 → 5 | 0 → 0 | 120 → 120 |
| `bpm_snapshot/0_sm_stops` | 2,834 → 2,834 | 1,302 → 1,294 | +0.6% | 9 → 9 | 0 → 0 | 264 → 264 |
| `bpm_snapshot/0_ssc_plain` | 1,203 → 1,275 | 549 → 585 | -6.2% | 3 → 3 | 0 → 0 | 96 → 96 |
| `bpm_snapshot/0_ssc_stops` | 1,868 → 1,982 | 853 → 904 | -5.6% | 4 → 4 | 0 → 0 | 128 → 128 |
| `bpm_snapshot/1_sm_plain` | 2,025 → 1,934 | 926 → 883 | +4.9% | 5 → 5 | 0 → 0 | 88 → 88 |
| `bpm_snapshot/1_sm_stops` | 3,206 → 3,241 | 1,470 → 1,480 | -0.7% | 9 → 9 | 0 → 0 | 232 → 232 |
| `bpm_snapshot/1_ssc_plain` | 1,696 → 1,628 | 775 → 746 | +3.9% | 3 → 3 | 0 → 0 | 64 → 64 |
| `bpm_snapshot/1_ssc_stops` | 2,226 → 2,450 | 1,017 → 1,120 | -9.2% | 4 → 4 | 0 → 0 | 96 → 96 |
| `bpm_snapshot/32_sm_plain` | 41,157 → 41,214 | 18,784 → 18,806 | -0.1% | 5 → 5 | 0 → 0 | 2,512 → 2,512 |
| `bpm_snapshot/32_sm_stops` | 41,650 → 40,893 | 19,001 → 18,656 | +1.8% | 9 → 9 | 0 → 0 | 2,656 → 2,656 |
| `bpm_snapshot/32_ssc_plain` | 40,082 → 33,180 | 18,311 → 15,141 | +20.9% | 3 → 3 | 0 → 0 | 1,744 → 1,744 |
| `bpm_snapshot/32_ssc_stops` | 38,571 → 33,792 | 17,598 → 15,413 | +14.2% | 4 → 4 | 0 → 0 | 1,776 → 1,776 |
| `bpm_snapshot/4096_sm_plain` | 4,586,328 → 5,190,948 | 2,093,220 → 2,369,740 | -11.7% | 5 → 5 | 0 → 0 | 338,512 → 338,512 |
| `bpm_snapshot/4096_sm_stops` | 4,668,802 → 5,392,025 | 2,130,467 → 2,461,627 | -13.5% | 9 → 9 | 0 → 0 | 338,656 → 338,656 |
| `bpm_snapshot/4096_ssc_plain` | 4,349,495 → 4,939,526 | 1,984,977 → 2,253,760 | -11.9% | 3 → 3 | 0 → 0 | 240,208 → 240,208 |
| `bpm_snapshot/4096_ssc_stops` | 4,555,386 → 4,830,032 | 2,080,400 → 2,205,410 | -5.7% | 4 → 4 | 0 → 0 | 240,240 → 240,240 |

### Cached chart snapshot leaves

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `bpm_cache/0_sm_cached_final` | 1,716 → 1,549 | 785 → 707 | +11.0% | 4 → 3 | 0 → 0 | 50 → 32 |
| `bpm_cache/0_sm_cached_more` | 1,463 → 1,748 | 670 → 801 | -16.4% | 4 → 4 | 0 → 0 | 50 → 50 |
| `bpm_cache/0_sm_first_final` | 3,011 → 3,292 | 1,376 → 1,504 | -8.5% | 9 → 8 | 0 → 0 | 170 → 152 |
| `bpm_cache/0_ssc_cached_final` | 1,613 → 1,484 | 736 → 679 | +8.4% | 4 → 3 | 0 → 0 | 50 → 32 |
| `bpm_cache/0_ssc_cached_more` | 1,485 → 1,644 | 698 → 753 | -7.3% | 4 → 4 | 0 → 0 | 50 → 50 |
| `bpm_cache/0_ssc_first_final` | 2,805 → 2,791 | 1,282 → 1,277 | +0.4% | 7 → 6 | 0 → 0 | 146 → 128 |
| `bpm_cache/1_sm_cached_final` | 1,771 → 1,698 | 824 → 781 | +5.5% | 5 → 4 | 0 → 0 | 64 → 45 |
| `bpm_cache/1_sm_cached_more` | 1,834 → 1,740 | 839 → 794 | +5.7% | 5 → 5 | 0 → 0 | 64 → 64 |
| `bpm_cache/1_sm_first_final` | 3,762 → 3,807 | 1,715 → 1,762 | -2.7% | 10 → 9 | 0 → 0 | 152 → 133 |
| `bpm_cache/1_ssc_cached_final` | 1,748 → 1,638 | 798 → 750 | +6.4% | 5 → 4 | 0 → 0 | 64 → 45 |
| `bpm_cache/1_ssc_cached_more` | 1,708 → 1,727 | 779 → 788 | -1.1% | 5 → 5 | 0 → 0 | 64 → 64 |
| `bpm_cache/1_ssc_first_final` | 3,515 → 3,449 | 1,610 → 1,576 | +2.2% | 8 → 7 | 0 → 0 | 128 → 109 |
| `bpm_cache/32_sm_cached_final` | 3,561 → 2,336 | 1,639 → 1,066 | +53.8% | 5 → 4 | 0 → 0 | 1,190 → 515 |
| `bpm_cache/32_sm_cached_more` | 3,678 → 3,195 | 1,681 → 1,460 | +15.1% | 5 → 5 | 0 → 0 | 1,190 → 1,190 |
| `bpm_cache/32_sm_first_final` | 43,125 → 38,332 | 19,687 → 17,493 | +12.5% | 10 → 9 | 0 → 0 | 3,702 → 3,027 |
| `bpm_cache/32_ssc_cached_final` | 3,174 → 1,993 | 1,450 → 910 | +59.3% | 5 → 4 | 0 → 0 | 1,190 → 515 |
| `bpm_cache/32_ssc_cached_more` | 3,077 → 2,857 | 1,406 → 1,305 | +7.7% | 5 → 5 | 0 → 0 | 1,190 → 1,190 |
| `bpm_cache/32_ssc_first_final` | 39,008 → 36,343 | 17,803 → 16,596 | +7.3% | 8 → 7 | 0 → 0 | 2,934 → 2,259 |
| `bpm_cache/4096_sm_cached_final` | 210,918 → 88,502 | 96,317 → 40,407 | +138.4% | 5 → 4 | 0 → 0 | 166,506 → 70,981 |
| `bpm_cache/4096_sm_cached_more` | 208,101 → 213,932 | 95,223 → 97,637 | -2.5% | 5 → 5 | 0 → 0 | 166,506 → 166,506 |
| `bpm_cache/4096_sm_first_final` | 5,515,252 → 5,286,262 | 2,517,757 → 2,413,520 | +4.3% | 10 → 9 | 0 → 0 | 505,018 → 409,493 |
| `bpm_cache/4096_ssc_cached_final` | 227,600 → 99,031 | 104,120 → 45,203 | +130.3% | 5 → 4 | 0 → 0 | 166,506 → 70,981 |
| `bpm_cache/4096_ssc_cached_more` | 192,728 → 212,652 | 88,083 → 97,080 | -9.3% | 5 → 5 | 0 → 0 | 166,506 → 166,506 |
| `bpm_cache/4096_ssc_first_final` | 4,657,336 → 4,670,506 | 2,125,740 → 2,131,580 | -0.3% | 8 → 7 | 0 → 0 | 406,714 → 311,189 |

### Timing tag leaves

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `timing_tag/owned/0_blank` | 29 → 2 | 13 → 1 | +1200.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/owned/0_clean` | 29 → 2 | 13 → 1 | +1200.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/owned/0_first` | 29 → 2 | 13 → 1 | +1200.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/owned/0_last` | 34 → 2 | 16 → 1 | +1500.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/owned/1_blank` | 168 → 14 | 77 → 6 | +1183.3% | 1 → 0 | 0 → 0 | 6 → 0 |
| `timing_tag/owned/1_clean` | 186 → 175 | 85 → 80 | +6.2% | 1 → 1 | 0 → 0 | 9 → 9 |
| `timing_tag/owned/1_first` | 236 → 230 | 108 → 105 | +2.9% | 1 → 1 | 0 → 0 | 13 → 13 |
| `timing_tag/owned/1_last` | 241 → 215 | 110 → 98 | +12.2% | 1 → 1 | 0 → 0 | 13 → 13 |
| `timing_tag/owned/128_blank` | 2,341 → 102 | 1,067 → 47 | +2170.2% | 1 → 0 | 0 → 0 | 768 → 0 |
| `timing_tag/owned/128_clean` | 7,755 → 7,864 | 3,539 → 3,591 | -1.4% | 1 → 1 | 0 → 0 | 1,507 → 1,507 |
| `timing_tag/owned/128_first` | 8,019 → 8,269 | 3,658 → 3,773 | -3.0% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `timing_tag/owned/128_last` | 7,798 → 7,904 | 3,557 → 3,605 | -1.3% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `timing_tag/owned/4096_blank` | 79,621 → 2,535 | 36,360 → 1,162 | +3029.1% | 1 → 0 | 0 → 0 | 24,576 → 0 |
| `timing_tag/owned/4096_clean` | 245,252 → 259,954 | 111,915 → 118,665 | -5.7% | 1 → 1 | 0 → 0 | 54,565 → 54,565 |
| `timing_tag/owned/4096_first` | 305,994 → 338,730 | 139,583 → 154,624 | -9.7% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `timing_tag/owned/4096_last` | 270,944 → 322,373 | 123,679 → 147,062 | -15.9% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `timing_tag/cow/0_blank` | 11 → 2 | 5 → 1 | +400.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/0_clean` | 11 → 2 | 5 → 1 | +400.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/0_first` | 11 → 2 | 5 → 1 | +400.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/0_last` | 11 → 2 | 5 → 1 | +400.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/1_blank` | 213 → 16 | 97 → 8 | +1112.5% | 1 → 0 | 0 → 0 | 6 → 0 |
| `timing_tag/cow/1_clean` | 66 → 63 | 30 → 29 | +3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/1_first` | 276 → 278 | 126 → 127 | -0.8% | 1 → 1 | 0 → 0 | 13 → 13 |
| `timing_tag/cow/1_last` | 261 → 265 | 119 → 121 | -1.7% | 1 → 1 | 0 → 0 | 13 → 13 |
| `timing_tag/cow/128_blank` | 6,080 → 102 | 2,774 → 47 | +5802.1% | 1 → 0 | 0 → 0 | 768 → 0 |
| `timing_tag/cow/128_clean` | 7,605 → 6,758 | 3,475 → 3,083 | +12.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/128_first` | 7,898 → 8,199 | 3,603 → 3,744 | -3.8% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `timing_tag/cow/128_last` | 15,430 → 14,968 | 7,038 → 6,832 | +3.0% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `timing_tag/cow/4096_blank` | 187,036 → 2,540 | 85,362 → 1,164 | +7233.5% | 1 → 0 | 0 → 0 | 24,576 → 0 |
| `timing_tag/cow/4096_clean` | 251,297 → 275,532 | 114,679 → 125,703 | -8.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/4096_first` | 248,599 → 332,733 | 113,533 → 151,985 | -25.3% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `timing_tag/cow/4096_last` | 505,454 → 524,227 | 230,659 → 239,130 | -3.5% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |

### Blank timing callers

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `blank_load/0_analyze` | 28,520 → 26,895 | 13,041 → 12,262 | +6.4% | 37 → 37 | 6 → 6 | 4,940 → 4,940 |
| `blank_load/0_duration` | 7,590 → 6,916 | 3,466 → 3,159 | +9.7% | 9 → 9 | 0 → 0 | 1,272 → 1,272 |
| `blank_load/0_peak` | 7,481 → 7,136 | 3,416 → 3,263 | +4.7% | 10 → 10 | 0 → 0 | 1,280 → 1,280 |
| `blank_load/0_snapshot` | 8,789 → 9,989 | 4,011 → 4,563 | -12.1% | 16 → 15 | 2 → 2 | 1,555 → 1,536 |
| `blank_load/1_analyze` | 29,977 → 25,763 | 13,689 → 11,771 | +16.3% | 53 → 41 | 6 → 6 | 5,036 → 4,964 |
| `blank_load/1_duration` | 7,783 → 6,370 | 3,559 → 2,909 | +22.3% | 13 → 9 | 0 → 0 | 1,296 → 1,272 |
| `blank_load/1_peak` | 7,055 → 5,979 | 3,221 → 2,730 | +18.0% | 14 → 10 | 0 → 0 | 1,304 → 1,280 |
| `blank_load/1_snapshot` | 9,893 → 9,856 | 4,514 → 4,497 | +0.4% | 20 → 15 | 2 → 2 | 1,579 → 1,536 |
| `blank_load/128_analyze` | 63,504 → 41,876 | 28,983 → 19,144 | +51.4% | 53 → 41 | 6 → 6 | 17,228 → 8,012 |
| `blank_load/128_duration` | 31,358 → 8,907 | 14,294 → 4,100 | +248.6% | 13 → 9 | 0 → 0 | 4,344 → 1,272 |
| `blank_load/128_peak` | 39,058 → 9,662 | 17,843 → 4,410 | +304.6% | 14 → 10 | 0 → 0 | 4,352 → 1,280 |
| `blank_load/128_snapshot` | 33,041 → 11,963 | 15,078 → 5,459 | +176.2% | 20 → 15 | 2 → 2 | 4,627 → 1,536 |
| `blank_load/4096_analyze` | 1,136,696 → 347,444 | 518,982 → 158,720 | +227.0% | 53 → 41 | 6 → 6 | 398,156 → 103,244 |
| `blank_load/4096_duration` | 891,899 → 61,998 | 406,943 → 28,300 | +1338.0% | 13 → 9 | 0 → 0 | 99,576 → 1,272 |
| `blank_load/4096_peak` | 866,053 → 65,470 | 395,279 → 29,889 | +1222.5% | 14 → 10 | 0 → 0 | 99,584 → 1,280 |
| `blank_load/4096_snapshot` | 839,427 → 52,928 | 383,272 → 24,149 | +1487.1% | 20 → 15 | 2 → 2 | 99,859 → 1,536 |

### Complete BPM snapshot callers

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `snapshot_load/0_clean_sm_global` | 6,708 → 7,165 | 3,064 → 3,267 | -6.2% | 15 → 14 | 0 → 0 | 1,564 → 1,546 |
| `snapshot_load/0_clean_ssc_global` | 6,919 → 5,976 | 3,154 → 2,726 | +15.7% | 13 → 12 | 0 → 0 | 1,540 → 1,522 |
| `snapshot_load/0_clean_ssc_local` | 7,227 → 6,270 | 3,301 → 2,859 | +15.5% | 13 → 12 | 0 → 0 | 1,540 → 1,522 |
| `snapshot_load/0_last_sm_global` | 5,678 → 5,483 | 2,589 → 2,512 | +3.1% | 15 → 14 | 0 → 0 | 1,564 → 1,546 |
| `snapshot_load/0_last_ssc_global` | 5,785 → 6,764 | 2,643 → 3,090 | -14.5% | 13 → 12 | 0 → 0 | 1,540 → 1,522 |
| `snapshot_load/0_last_ssc_local` | 6,554 → 6,026 | 2,989 → 2,757 | +8.4% | 13 → 12 | 0 → 0 | 1,540 → 1,522 |
| `snapshot_load/1_clean_sm_global` | 7,554 → 7,902 | 3,455 → 3,609 | -4.3% | 18 → 17 | 1 → 1 | 1,587 → 1,568 |
| `snapshot_load/1_clean_ssc_global` | 7,657 → 7,795 | 3,497 → 3,555 | -1.6% | 16 → 15 | 1 → 1 | 1,563 → 1,544 |
| `snapshot_load/1_clean_ssc_local` | 10,148 → 11,081 | 4,633 → 5,061 | -8.5% | 17 → 17 | 3 → 3 | 1,617 → 1,617 |
| `snapshot_load/1_last_sm_global` | 7,066 → 7,358 | 3,222 → 3,360 | -4.1% | 19 → 18 | 0 → 0 | 1,586 → 1,567 |
| `snapshot_load/1_last_ssc_global` | 9,104 → 8,122 | 4,197 → 3,704 | +13.3% | 17 → 16 | 0 → 0 | 1,562 → 1,543 |
| `snapshot_load/1_last_ssc_local` | 12,038 → 12,530 | 5,493 → 5,719 | -4.0% | 20 → 20 | 2 → 2 | 1,642 → 1,642 |
| `snapshot_load/32_clean_sm_global` | 55,681 → 54,713 | 25,423 → 24,982 | +1.8% | 18 → 17 | 1 → 1 | 7,301 → 6,626 |
| `snapshot_load/32_clean_ssc_global` | 58,425 → 51,412 | 26,675 → 23,443 | +13.8% | 16 → 15 | 1 → 1 | 6,533 → 5,858 |
| `snapshot_load/32_clean_ssc_local` | 124,525 → 116,294 | 56,837 → 53,059 | +7.1% | 17 → 17 | 3 → 3 | 8,091 → 8,091 |
| `snapshot_load/32_last_sm_global` | 62,599 → 62,713 | 28,643 → 28,602 | +0.1% | 19 → 18 | 1 → 1 | 7,672 → 6,997 |
| `snapshot_load/32_last_ssc_global` | 63,637 → 59,433 | 29,033 → 27,131 | +7.0% | 17 → 16 | 1 → 1 | 6,904 → 6,229 |
| `snapshot_load/32_last_ssc_local` | 135,925 → 134,098 | 62,018 → 61,232 | +1.3% | 20 → 20 | 3 → 3 | 9,180 → 9,180 |
| `snapshot_load/4096_clean_sm_global` | 7,917,782 → 7,831,870 | 3,616,490 → 3,574,780 | +1.2% | 18 → 17 | 1 → 1 | 836,563 → 741,038 |
| `snapshot_load/4096_clean_ssc_global` | 7,967,444 → 8,066,164 | 3,637,095 → 3,681,685 | -1.2% | 16 → 15 | 1 → 1 | 738,259 → 642,734 |
| `snapshot_load/4096_clean_ssc_local` | 16,543,155 → 16,299,708 | 7,558,010 → 7,440,670 | +1.6% | 17 → 17 | 3 → 3 | 972,909 → 972,909 |
| `snapshot_load/4096_last_sm_global` | 8,200,333 → 7,942,498 | 3,744,155 → 3,627,405 | +3.2% | 19 → 18 | 1 → 1 | 891,144 → 795,619 |
| `snapshot_load/4096_last_ssc_global` | 7,844,260 → 8,590,121 | 3,580,945 → 3,922,150 | -8.7% | 17 → 16 | 1 → 1 | 792,840 → 697,315 |
| `snapshot_load/4096_last_ssc_local` | 17,081,260 → 18,263,234 | 7,796,240 → 8,337,645 | -6.5% | 20 → 20 | 3 → 3 | 1,136,628 → 1,136,628 |

### Duration callers

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `raw_duration/0_clean_sm_global` | 5,632 → 5,220 | 2,569 → 2,387 | +7.6% | 11 → 11 | 0 → 0 | 1,296 → 1,296 |
| `raw_duration/0_clean_ssc_global` | 5,516 → 5,098 | 2,516 → 2,326 | +8.2% | 9 → 9 | 0 → 0 | 1,272 → 1,272 |
| `raw_duration/0_clean_ssc_local` | 5,714 → 5,212 | 2,616 → 2,377 | +10.1% | 9 → 9 | 0 → 0 | 1,272 → 1,272 |
| `raw_duration/0_last_sm_global` | 5,021 → 4,709 | 2,290 → 2,148 | +6.6% | 11 → 11 | 0 → 0 | 1,296 → 1,296 |
| `raw_duration/0_last_ssc_global` | 5,507 → 5,628 | 2,519 → 2,567 | -1.9% | 9 → 9 | 0 → 0 | 1,272 → 1,272 |
| `raw_duration/0_last_ssc_local` | 6,108 → 6,158 | 2,785 → 2,815 | -1.1% | 9 → 9 | 0 → 0 | 1,272 → 1,272 |
| `raw_duration/1_clean_sm_global` | 5,717 → 5,599 | 2,626 → 2,566 | +2.3% | 11 → 11 | 0 → 0 | 1,264 → 1,264 |
| `raw_duration/1_clean_ssc_global` | 6,249 → 5,427 | 2,864 → 2,475 | +15.7% | 9 → 9 | 0 → 0 | 1,240 → 1,240 |
| `raw_duration/1_clean_ssc_local` | 6,750 → 6,264 | 3,101 → 2,866 | +8.2% | 9 → 9 | 0 → 0 | 1,240 → 1,240 |
| `raw_duration/1_last_sm_global` | 5,606 → 5,170 | 2,565 → 2,364 | +8.5% | 12 → 12 | 0 → 0 | 1,277 → 1,277 |
| `raw_duration/1_last_ssc_global` | 6,552 → 6,153 | 2,994 → 2,807 | +6.7% | 10 → 10 | 0 → 0 | 1,253 → 1,253 |
| `raw_duration/1_last_ssc_local` | 6,944 → 7,186 | 3,166 → 3,277 | -3.4% | 11 → 11 | 0 → 0 | 1,266 → 1,266 |
| `raw_duration/32_clean_sm_global` | 17,942 → 17,400 | 8,227 → 7,959 | +3.4% | 11 → 11 | 0 → 0 | 3,440 → 3,440 |
| `raw_duration/32_clean_ssc_global` | 16,798 → 16,243 | 7,663 → 7,412 | +3.4% | 9 → 9 | 0 → 0 | 2,672 → 2,672 |
| `raw_duration/32_clean_ssc_local` | 20,305 → 19,677 | 9,262 → 8,977 | +3.2% | 9 → 9 | 0 → 0 | 2,672 → 2,672 |
| `raw_duration/32_last_sm_global` | 20,343 → 20,782 | 9,310 → 9,479 | -1.8% | 12 → 12 | 0 → 0 | 3,799 → 3,799 |
| `raw_duration/32_last_ssc_global` | 20,018 → 19,500 | 9,139 → 8,900 | +2.7% | 10 → 10 | 0 → 0 | 3,031 → 3,031 |
| `raw_duration/32_last_ssc_local` | 26,284 → 26,352 | 11,995 → 12,041 | -0.4% | 11 → 11 | 0 → 0 | 3,390 → 3,390 |
| `raw_duration/4096_clean_sm_global` | 1,903,581 → 1,866,321 | 869,730 → 852,330 | +2.0% | 11 → 11 | 0 → 0 | 306,928 → 306,928 |
| `raw_duration/4096_clean_ssc_global` | 1,752,697 → 1,731,306 | 800,155 → 790,790 | +1.2% | 9 → 9 | 0 → 0 | 208,624 → 208,624 |
| `raw_duration/4096_clean_ssc_local` | 2,193,760 → 2,162,755 | 1,003,565 → 987,205 | +1.7% | 9 → 9 | 0 → 0 | 208,624 → 208,624 |
| `raw_duration/4096_last_sm_global` | 2,393,626 → 2,371,840 | 1,092,885 → 1,083,040 | +0.9% | 12 → 12 | 0 → 0 | 361,497 → 361,497 |
| `raw_duration/4096_last_ssc_global` | 2,227,277 → 2,196,218 | 1,016,495 → 1,002,420 | +1.4% | 10 → 10 | 0 → 0 | 263,193 → 263,193 |
| `raw_duration/4096_last_ssc_local` | 2,951,430 → 2,969,528 | 1,347,470 → 1,355,895 | -0.6% | 11 → 11 | 0 → 0 | 317,762 → 317,762 |

### Complete timing cleanup

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `timing_cleanup/0_ordered` | 856 → 893 | 393 → 410 | -4.1% | 2 → 2 | 0 → 0 | 24 → 24 |
| `timing_cleanup/0_duplicates` | 853 → 981 | 391 → 450 | -13.1% | 2 → 2 | 0 → 0 | 24 → 24 |
| `timing_cleanup/0_reverse` | 914 → 893 | 419 → 410 | +2.2% | 2 → 2 | 0 → 0 | 24 → 24 |
| `timing_cleanup/1_ordered` | 3,876 → 4,937 | 1,768 → 2,252 | -21.5% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/1_duplicates` | 3,876 → 5,081 | 1,769 → 2,318 | -23.7% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/1_reverse` | 4,935 → 4,772 | 2,263 → 2,176 | +4.0% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/32_ordered` | 50,323 → 50,081 | 23,007 → 22,860 | +0.6% | 14 → 14 | 0 → 0 | 5,528 → 5,528 |
| `timing_cleanup/32_duplicates` | 50,130 → 50,649 | 22,892 → 23,109 | -0.9% | 14 → 14 | 0 → 0 | 4,600 → 4,600 |
| `timing_cleanup/32_reverse` | 70,655 → 56,679 | 32,230 → 25,867 | +24.6% | 24 → 24 | 0 → 0 | 10,136 → 10,136 |
| `timing_cleanup/4096_ordered` | 6,903,012 → 7,223,141 | 3,151,800 → 3,296,985 | -4.4% | 14 → 14 | 0 → 0 | 731,704 → 731,704 |
| `timing_cleanup/4096_duplicates` | 6,788,070 → 6,893,562 | 3,099,020 → 3,149,405 | -1.6% | 14 → 14 | 0 → 0 | 611,032 → 611,032 |
| `timing_cleanup/4096_reverse` | 42,217,061 → 39,788,666 | 19,267,630 → 18,161,815 | +6.1% | 24 → 24 | 0 → 0 | 1,321,528 → 1,321,528 |

### Full analysis controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `analyze/fast_fake_lifts` | 642,446 → 640,982 | 293,399 → 292,701 | +0.2% | 31 → 31 | 4 → 4 | 59,028 → 59,028 |
| `analyze/camellia` | 440,511,521 → 449,263,665 | 201,105,640 → 205,120,220 | -2.0% | 110 → 110 | 0 → 0 | 5,263,624 → 5,263,624 |
| `analyze/fast_camellia` | 55,890,890 → 56,284,893 | 25,520,560 → 25,699,080 | -0.7% | 115 → 115 | 0 → 0 | 7,051,152 → 7,051,152 |
| `analyze/mixed_small` | 49,372 → 49,600 | 22,528 → 22,635 | -0.5% | 59 → 59 | 3 → 3 | 7,460 → 7,460 |

### Focused control recheck

Longer batches check the exact slower samples and neighboring controls. Original main samples remain above. Allocation results remain non-increasing. Iterations: raw cleanup 1/128 entries 20,000, 4,096 entries 1,000; native timing 4,096 entries 100; cache leaves 0 entries 10,000, 4,096 entries 100; timing-tag leaves 4,096 entries 500; blank snapshot and small complete snapshots 5,000; composed one-entry timing cleanup 20,000. The 210 main plus 42 recheck cases total 252 measurements.

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `raw_map/1_clean` | 50 → 51 | 23 → 23 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/1_first` | 289 → 319 | 132 → 145 | -9.0% | 1 → 1 | 0 → 0 | 13 → 13 |
| `raw_map/1_middle` | 284 → 306 | 130 → 140 | -7.1% | 1 → 1 | 0 → 0 | 13 → 13 |
| `raw_map/1_last` | 265 → 308 | 121 → 141 | -14.2% | 1 → 1 | 0 → 0 | 13 → 13 |
| `raw_map/128_clean` | 8,118 → 8,687 | 3,704 → 3,964 | -6.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/128_first` | 9,333 → 9,762 | 4,260 → 4,455 | -4.4% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `raw_map/128_middle` | 13,624 → 14,218 | 6,216 → 6,487 | -4.2% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `raw_map/128_last` | 18,171 → 18,387 | 8,293 → 8,390 | -1.2% | 1 → 1 | 0 → 0 | 1,511 → 1,511 |
| `raw_map/4096_clean` | 292,024 → 294,898 | 133,240 → 134,569 | -1.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `raw_map/4096_first` | 354,042 → 344,630 | 161,649 → 157,329 | +2.7% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `raw_map/4096_middle` | 472,297 → 480,355 | 215,545 → 219,233 | -1.7% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `raw_map/4096_last` | 616,077 → 630,095 | 281,161 → 287,536 | -2.2% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `bpm_snapshot/4096_sm_plain` | 5,513,386 → 5,759,471 | 2,517,261 → 2,629,123 | -4.3% | 5 → 5 | 0 → 0 | 338,512 → 338,512 |
| `bpm_snapshot/4096_sm_stops` | 5,558,456 → 5,530,403 | 2,539,952 → 2,524,579 | +0.6% | 9 → 9 | 0 → 0 | 338,656 → 338,656 |
| `bpm_snapshot/4096_ssc_plain` | 5,376,451 → 5,197,095 | 2,453,879 → 2,372,454 | +3.4% | 3 → 3 | 0 → 0 | 240,208 → 240,208 |
| `bpm_snapshot/4096_ssc_stops` | 4,870,771 → 5,237,178 | 2,223,854 → 2,390,161 | -7.0% | 4 → 4 | 0 → 0 | 240,240 → 240,240 |
| `bpm_cache/0_sm_cached_final` | 1,457 → 1,185 | 666 → 547 | +21.8% | 4 → 3 | 0 → 0 | 50 → 32 |
| `bpm_cache/0_sm_cached_more` | 1,539 → 1,366 | 706 → 624 | +13.1% | 4 → 4 | 0 → 0 | 50 → 50 |
| `bpm_cache/0_sm_first_final` | 2,956 → 2,551 | 1,349 → 1,169 | +15.4% | 9 → 8 | 0 → 0 | 170 → 152 |
| `bpm_cache/0_ssc_cached_final` | 1,431 → 1,272 | 653 → 580 | +12.6% | 4 → 3 | 0 → 0 | 50 → 32 |
| `bpm_cache/0_ssc_cached_more` | 1,401 → 1,289 | 639 → 588 | +8.7% | 4 → 4 | 0 → 0 | 50 → 50 |
| `bpm_cache/0_ssc_first_final` | 2,386 → 2,195 | 1,089 → 1,007 | +8.1% | 7 → 6 | 0 → 0 | 146 → 128 |
| `bpm_cache/4096_ssc_cached_final` | 211,378 → 96,415 | 96,461 → 44,017 | +119.1% | 5 → 4 | 0 → 0 | 166,506 → 70,981 |
| `bpm_cache/4096_ssc_cached_more` | 203,338 → 220,580 | 92,798 → 100,724 | -7.9% | 5 → 5 | 0 → 0 | 166,506 → 166,506 |
| `timing_tag/owned/4096_blank` | 61,204 → 2,488 | 27,914 → 1,143 | +2342.2% | 1 → 0 | 0 → 0 | 24,576 → 0 |
| `timing_tag/owned/4096_clean` | 272,496 → 267,217 | 124,328 → 121,930 | +2.0% | 1 → 1 | 0 → 0 | 54,565 → 54,565 |
| `timing_tag/owned/4096_first` | 264,515 → 291,712 | 120,707 → 133,105 | -9.3% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `timing_tag/owned/4096_last` | 257,271 → 282,896 | 117,441 → 129,156 | -9.1% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `timing_tag/cow/4096_blank` | 194,303 → 2,409 | 88,630 → 1,099 | +7964.6% | 1 → 0 | 0 → 0 | 24,576 → 0 |
| `timing_tag/cow/4096_clean` | 255,799 → 230,196 | 116,689 → 105,005 | +11.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `timing_tag/cow/4096_first` | 271,345 → 273,948 | 123,847 → 125,091 | -1.0% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `timing_tag/cow/4096_last` | 525,113 → 507,298 | 239,837 → 231,496 | +3.6% | 1 → 1 | 0 → 0 | 54,569 → 54,569 |
| `blank_load/0_snapshot` | 7,852 → 7,487 | 3,582 → 3,416 | +4.9% | 16 → 15 | 2 → 2 | 1,555 → 1,536 |
| `snapshot_load/1_clean_sm_global` | 7,101 → 6,415 | 3,239 → 2,927 | +10.7% | 18 → 17 | 1 → 1 | 1,587 → 1,568 |
| `snapshot_load/1_clean_ssc_global` | 7,197 → 7,172 | 3,284 → 3,271 | +0.4% | 16 → 15 | 1 → 1 | 1,563 → 1,544 |
| `snapshot_load/1_clean_ssc_local` | 9,615 → 10,249 | 4,387 → 4,677 | -6.2% | 17 → 17 | 3 → 3 | 1,617 → 1,617 |
| `snapshot_load/1_last_sm_global` | 7,509 → 6,697 | 3,426 → 3,055 | +12.1% | 19 → 18 | 0 → 0 | 1,586 → 1,567 |
| `snapshot_load/1_last_ssc_global` | 7,794 → 6,938 | 3,562 → 3,171 | +12.3% | 17 → 16 | 0 → 0 | 1,562 → 1,543 |
| `snapshot_load/1_last_ssc_local` | 11,291 → 10,195 | 5,151 → 4,651 | +10.8% | 20 → 20 | 2 → 2 | 1,642 → 1,642 |
| `timing_cleanup/1_ordered` | 4,072 → 3,719 | 1,858 → 1,697 | +9.5% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/1_duplicates` | 4,294 → 3,774 | 1,961 → 1,722 | +13.9% | 14 → 14 | 0 → 0 | 192 → 192 |
| `timing_cleanup/1_reverse` | 4,084 → 3,363 | 1,865 → 1,534 | +21.6% | 14 → 14 | 0 → 0 | 192 → 192 |

### Validation

- Release tests: 147 core + 79 rssp + 29 integration regression tests = 255 passed, zero failed.
- After confirming the optimizations: `cargo test --release --test all_parity -- --test-threads=22` — 30,489 passed, zero failed.
- Strict release workspace Clippy for every target, formatting check, and diff whitespace check pass.
- Byte comparison includes explicit timing outputs, BPM snapshot fields/ranges, blank-map fallback, JSON reports, hashes, durations, peak NPS, labels, and the complete existing corpus. No golden expected data is changed.
- `corpus`: 174,790,323 UTF-8 bytes; SHA-256 `e7d2f22bd7b7f48c0335075d8d2ac355063b58809fdd77c42dcec23927a5759d`.
- `core-trace`: 1,880,137 UTF-8 bytes; SHA-256 `85e664e370568afd3d84cd3da2aca18bfcacf46ce1d0a1018a5dfe6deaaceae2`.
- `leaf-trace`: 29,124,186 UTF-8 bytes; SHA-256 `aac4e521dea9affd1f0e1baa95991e7dd0fa7f4bb10d109b82bed5ff84d290d7`.

### Reproduction

Keep the current harness/fixtures and package version for both builds; restore only the modified production functions to `b6a6af7` for the original executable. Adapt private helper-call arguments to each production signature. Save original executables before building the optimized sources. Setup and lifetime rules must match.

```powershell
cargo test --release -p rssp-core --lib
cargo test --release -p rssp --lib
cargo bench -p rssp --bench hotpath_perf --no-run
# Run each saved core test executable with the same selected filter and iterations:
$env:RSSP_PASS_FILTER='timing_tag/owned/4096_'
$env:RSSP_PASS_ITERS='100'
.\saved-core.exe pass_edges --skip _trace --ignored --nocapture --test-threads=1
$env:RSSP_HOT_FILTER='blank_load/4096_'
$env:RSSP_HOT_ITERS='100'
.\saved-hotpath.exe
# Alternate original/optimized, optimized/original, original/optimized.
cargo test --release --test all_parity -- --test-threads=22
```

## Pass 0.4.286: render stream breakdowns without a segment vector

Baseline: `3772598` (0.4.285). This pass increments the workspace patch version exactly once to **0.4.286**. It retains three optimizations:

1. **Render the three standard breakdowns directly from measures.** Remove the temporary `Vec<StreamSegment>` and its formatter. Trim ignored boundary breaks with the existing range scan, parse the first run once, and resume the existing segment visitor for sizing and output. The sizing pass counts actual segments using the same visitor and retains the original per-segment reservation policy for segmented outputs. Uniform active ranges require no segment pass. Single-output APIs keep their original collecting implementation.
2. **Share one bulk decimal writer.** Delete the duplicate `push_usize`. The existing integer writer handles single digits directly and appends its populated ASCII suffix once instead of reserving and pushing each character. Its unchecked UTF-8 conversion has a local ASCII invariant and safety comment; every suffix byte is generated from a decimal digit. Stream lengths and decimal timing formatting use the same implementation.
3. **Pre-size long run symbols.** Symbols of six or more decimal digits allocate their exact digit/affix length before writing. Short symbols retain the original first-allocation path. Large decorated values avoid one or two reallocations and copies; symbol text and affixes are unchanged.

Two measured code-layout hints keep shared range/collecting scans outside their callers. Without them, inlining produced regressions on long empty/leading ranges and uniform detailed output. The focused final check records those paths explicitly. No cache, dependency, dynamic dispatch, or public API is added.

On the 4,096-measure mixed fixture, the three-output path uses 81,958 → 73,756 cycles and 103,212 → 29,484 requested bytes (71.4% less churn), with four allocations plus one reallocation reduced to three allocations. Maximum `u64` decimal output uses 148 → 87 cycles (41.2% less). The maximum decorated run symbol uses 720 → 252 cycles (65.0% less) and 56 → 23 requested bytes, reducing three allocation/reallocation operations to one.

### Method

Windows x86-64, Intel Xeon E5-2696 v4, 44 logical processors, benchmark thread pinned to CPU 2; rustc 1.98.1, LLVM 22.1.8. Original and optimized builds use identical current fixtures/harnesses and package version, with only the production implementations restored to the baseline commit for the original build. Cargo release/bench profiles use fat LTO and one codegen unit.

Four warmup calls and seven timed batches per process; three alternating process pairs (old/new, new/old, old/new). Tables report the median of the three batch medians. Windows `QueryThreadCycleTime` measures thread CPU cycles; wall-clock ns and derived call throughput are also shown. Rounded ns limit precision for tiny calls. The counting System allocator runs separately from timing; bytes are total successful allocation/reallocation requests per call, **not peak live memory or RSS**. Returned outputs are destroyed inside both timed loops. Fixtures, reusable number buffers, and labels are prepared outside measurement. No own builds, tests, or corpus verification run during the final timed comparisons.

Main iterations: integers/decimals 100,000; stream 0/1/32 measures 10,000 and 4,096 measures 1,000; run symbols 20,000; standard/SN streams and hash batches 1,000; analysis/report/cleanup 100 (existing Camellia full-analysis fixture uses 10). Dense, mixed, alternating, empty, leading, trailing, and late-gap fixtures check both leaves and callers. The main sweep has 292 cases. Longer focused checks below preserve the slower main samples rather than replacing them. All measured allocation counts, reallocations, and requested bytes are non-increasing.

CPU gains are workload-specific; the control tables include slower samples. The stable allocation reduction is the primary gain for dense stream output. This pass makes no claim of uniformly faster end-to-end analysis. Broad single-output rewrites, speculative hash changes, approximate sizing that over-reserved returned strings, and exact sizing on tiny symbols were rejected.

### Decimal writer and composed decimal formatting

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `uint/0` | 19 → 9 | 9 → 4 | +125.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `uint/7` | 14 → 10 | 6 → 4 | +50.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `uint/123` | 27 → 25 | 12 → 11 | +9.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `uint/1234567` | 55 → 40 | 25 → 18 | +38.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `uint/18446744073709551615` | 148 → 87 | 68 → 40 | +70.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `decimal/zero` | 35 → 34 | 16 → 15 | +6.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `decimal/bpm` | 62 → 52 | 28 → 24 | +16.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `decimal/negative` | 82 → 64 | 37 → 29 | +27.6% | 0 → 0 | 0 → 0 | 0 → 0 |

### Standard stream leaves and three-output renderer

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `stream/0_empty_visit` | 3 → 2 | 1 → 1 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/0_empty_detailed` | 159 → 142 | 72 → 65 | +10.8% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_empty_partial` | 159 → 142 | 73 → 65 | +12.3% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_empty_simple` | 140 → 145 | 64 → 66 | -3.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_empty_total` | 137 → 139 | 62 → 64 | -3.1% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_empty_three` | 441 → 387 | 201 → 177 | +13.6% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/0_single_visit` | 3 → 2 | 1 → 1 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/0_single_detailed` | 157 → 147 | 71 → 67 | +6.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_single_partial` | 156 → 150 | 71 → 69 | +2.9% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_single_simple` | 163 → 151 | 75 → 69 | +8.7% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_single_total` | 158 → 151 | 72 → 69 | +4.3% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_single_three` | 415 → 429 | 189 → 196 | -3.6% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/0_mixed_visit` | 3 → 3 | 1 → 1 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/0_mixed_detailed` | 155 → 150 | 71 → 69 | +2.9% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_mixed_partial` | 152 → 150 | 69 → 69 | +0.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_mixed_simple` | 153 → 150 | 70 → 69 | +1.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_mixed_total` | 157 → 149 | 71 → 68 | +4.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_mixed_three` | 427 → 427 | 195 → 195 | +0.0% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/0_leading_visit` | 3 → 3 | 1 → 1 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/0_leading_detailed` | 157 → 158 | 72 → 72 | +0.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_leading_partial` | 160 → 160 | 73 → 73 | +0.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_leading_simple` | 154 → 151 | 71 → 69 | +2.9% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_leading_total` | 154 → 151 | 70 → 69 | +1.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_leading_three` | 419 → 405 | 191 → 185 | +3.2% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/0_alternating_visit` | 3 → 3 | 1 → 1 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/0_alternating_detailed` | 154 → 153 | 70 → 70 | +0.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_alternating_partial` | 151 → 151 | 69 → 69 | +0.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_alternating_simple` | 167 → 144 | 76 → 66 | +15.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_alternating_total` | 153 → 141 | 70 → 64 | +9.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_alternating_three` | 424 → 407 | 193 → 186 | +3.8% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/0_late_visit` | 3 → 2 | 1 → 1 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/0_late_detailed` | 156 → 139 | 71 → 64 | +10.9% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_late_partial` | 156 → 150 | 71 → 69 | +2.9% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_late_simple` | 149 → 140 | 68 → 64 | +6.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_late_total` | 153 → 150 | 70 → 69 | +1.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_late_three` | 422 → 429 | 193 → 196 | -1.5% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/0_trailing_visit` | 3 → 2 | 1 → 1 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/0_trailing_detailed` | 140 → 150 | 64 → 69 | -7.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_trailing_partial` | 153 → 151 | 70 → 69 | +1.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_trailing_simple` | 139 → 151 | 64 → 69 | -7.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_trailing_total` | 141 → 151 | 64 → 69 | -7.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/0_trailing_three` | 400 → 427 | 182 → 195 | -6.7% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/1_empty_visit` | 4 → 4 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/1_empty_detailed` | 167 → 163 | 77 → 75 | +2.7% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_empty_partial` | 164 → 161 | 75 → 73 | +2.7% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_empty_simple` | 165 → 160 | 76 → 73 | +4.1% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_empty_total` | 157 → 150 | 72 → 68 | +5.9% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_empty_three` | 434 → 409 | 198 → 188 | +5.3% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/1_single_visit` | 10 → 10 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/1_single_detailed` | 331 → 334 | 151 → 152 | -0.7% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_single_partial` | 333 → 314 | 152 → 144 | +5.6% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_single_simple` | 334 → 326 | 153 → 149 | +2.7% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_single_total` | 176 → 177 | 81 → 81 | +0.0% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/1_single_three` | 637 → 485 | 291 → 221 | +31.7% | 4 → 3 | 0 → 0 | 42 → 10 |
| `stream/1_mixed_visit` | 4 → 4 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/1_mixed_detailed` | 169 → 166 | 78 → 76 | +2.6% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_mixed_partial` | 165 → 169 | 75 → 77 | -2.6% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_mixed_simple` | 165 → 167 | 75 → 76 | -1.3% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_mixed_total` | 153 → 159 | 70 → 72 | -2.8% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/1_mixed_three` | 441 → 428 | 201 → 195 | +3.1% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/1_leading_visit` | 10 → 10 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/1_leading_detailed` | 331 → 315 | 151 → 144 | +4.9% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_leading_partial` | 333 → 315 | 152 → 144 | +5.6% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_leading_simple` | 334 → 329 | 152 → 150 | +1.3% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_leading_total` | 176 → 174 | 80 → 79 | +1.3% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/1_leading_three` | 631 → 484 | 288 → 221 | +30.3% | 4 → 3 | 0 → 0 | 42 → 10 |
| `stream/1_alternating_visit` | 10 → 10 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/1_alternating_detailed` | 336 → 321 | 153 → 147 | +4.1% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_alternating_partial` | 333 → 314 | 152 → 144 | +5.6% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_alternating_simple` | 335 → 323 | 153 → 147 | +4.1% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_alternating_total` | 176 → 173 | 80 → 79 | +1.3% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/1_alternating_three` | 658 → 448 | 301 → 204 | +47.5% | 4 → 3 | 0 → 0 | 42 → 10 |
| `stream/1_late_visit` | 10 → 10 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/1_late_detailed` | 328 → 303 | 150 → 138 | +8.7% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_late_partial` | 333 → 315 | 152 → 144 | +5.6% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_late_simple` | 336 → 314 | 153 → 143 | +7.0% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_late_total` | 174 → 173 | 80 → 79 | +1.3% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/1_late_three` | 640 → 451 | 292 → 207 | +41.1% | 4 → 3 | 0 → 0 | 42 → 10 |
| `stream/1_trailing_visit` | 10 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/1_trailing_detailed` | 328 → 323 | 150 → 147 | +2.0% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_trailing_partial` | 328 → 316 | 150 → 145 | +3.4% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_trailing_simple` | 332 → 328 | 151 → 150 | +0.7% | 2 → 2 | 0 → 0 | 30 → 30 |
| `stream/1_trailing_total` | 172 → 183 | 79 → 84 | -6.0% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/1_trailing_three` | 622 → 488 | 284 → 223 | +27.4% | 4 → 3 | 0 → 0 | 42 → 10 |
| `stream/32_empty_visit` | 37 → 37 | 17 → 17 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/32_empty_detailed` | 181 → 191 | 83 → 88 | -5.7% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/32_empty_partial` | 180 → 194 | 82 → 88 | -6.8% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/32_empty_simple` | 189 → 193 | 86 → 88 | -2.3% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/32_empty_total` | 176 → 189 | 80 → 87 | -8.0% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/32_empty_three` | 419 → 416 | 191 → 190 | +0.5% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/32_single_visit` | 58 → 58 | 27 → 27 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/32_single_detailed` | 394 → 412 | 180 → 188 | -4.3% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_partial` | 368 → 421 | 168 → 192 | -12.5% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_simple` | 384 → 417 | 175 → 191 | -8.4% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_total` | 205 → 223 | 94 → 102 | -7.8% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_single_three` | 698 → 547 | 319 → 250 | +27.6% | 4 → 3 | 0 → 0 | 426 → 12 |
| `stream/32_mixed_visit` | 92 → 101 | 42 → 46 | -8.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/32_mixed_detailed` | 719 → 632 | 328 → 289 | +13.5% | 2 → 2 | 0 → 0 | 486 → 486 |
| `stream/32_mixed_partial` | 607 → 620 | 277 → 283 | -2.1% | 2 → 2 | 0 → 0 | 486 → 486 |
| `stream/32_mixed_simple` | 669 → 616 | 305 → 281 | +8.5% | 2 → 2 | 0 → 0 | 486 → 486 |
| `stream/32_mixed_total` | 219 → 223 | 100 → 102 | -2.0% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_mixed_three` | 1,312 → 1,049 | 601 → 478 | +25.7% | 4 → 3 | 0 → 0 | 642 → 234 |
| `stream/32_leading_visit` | 42 → 43 | 19 → 20 | -5.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/32_leading_detailed` | 380 → 376 | 173 → 172 | +0.6% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_leading_partial` | 368 → 384 | 168 → 175 | -4.0% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_leading_simple` | 385 → 375 | 176 → 171 | +2.9% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_leading_total` | 218 → 224 | 100 → 102 | -2.0% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_leading_three` | 678 → 554 | 311 → 253 | +22.9% | 4 → 3 | 0 → 0 | 444 → 10 |
| `stream/32_alternating_visit` | 137 → 134 | 63 → 62 | +1.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/32_alternating_detailed` | 818 → 654 | 374 → 298 | +25.5% | 2 → 2 | 0 → 0 | 504 → 504 |
| `stream/32_alternating_partial` | 811 → 663 | 370 → 303 | +22.1% | 2 → 2 | 0 → 0 | 504 → 504 |
| `stream/32_alternating_simple` | 674 → 654 | 307 → 298 | +3.0% | 2 → 2 | 0 → 0 | 504 → 504 |
| `stream/32_alternating_total` | 230 → 217 | 105 → 99 | +6.1% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_alternating_three` | 1,406 → 1,169 | 642 → 533 | +20.5% | 4 → 3 | 0 → 0 | 696 → 288 |
| `stream/32_late_visit` | 65 → 63 | 30 → 29 | +3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/32_late_detailed` | 427 → 438 | 195 → 200 | -2.5% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_late_partial` | 416 → 434 | 190 → 198 | -4.0% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_late_simple` | 423 → 422 | 193 → 193 | +0.0% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_late_total` | 219 → 228 | 100 → 104 | -3.8% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_late_three` | 796 → 598 | 364 → 273 | +33.3% | 4 → 3 | 0 → 0 | 444 → 36 |
| `stream/32_trailing_visit` | 59 → 59 | 27 → 27 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/32_trailing_detailed` | 415 → 367 | 189 → 167 | +13.2% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_trailing_partial` | 400 → 345 | 182 → 158 | +15.2% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_trailing_simple` | 419 → 387 | 191 → 176 | +8.5% | 2 → 2 | 0 → 0 | 420 → 420 |
| `stream/32_trailing_total` | 213 → 224 | 97 → 102 | -4.9% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_trailing_three` | 714 → 519 | 326 → 237 | +37.6% | 4 → 3 | 0 → 0 | 444 → 10 |
| `stream/4096_empty_visit` | 4,418 → 4,616 | 2,016 → 2,113 | -4.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/4096_empty_detailed` | 4,940 → 4,968 | 2,256 → 2,272 | -0.7% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/4096_empty_partial` | 5,173 → 4,959 | 2,362 → 2,263 | +4.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/4096_empty_simple` | 5,106 → 4,960 | 2,345 → 2,265 | +3.5% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/4096_empty_total` | 6,060 → 5,841 | 2,764 → 2,672 | +3.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/4096_empty_three` | 5,030 → 5,291 | 2,299 → 2,413 | -4.7% | 3 → 3 | 0 → 0 | 33 → 33 |
| `stream/4096_single_visit` | 5,841 → 5,725 | 2,664 → 2,611 | +2.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/4096_single_detailed` | 6,400 → 6,489 | 2,918 → 2,964 | -1.6% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `stream/4096_single_partial` | 6,407 → 6,488 | 2,921 → 2,962 | -1.4% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `stream/4096_single_simple` | 6,401 → 6,499 | 2,928 → 2,965 | -1.2% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `stream/4096_single_total` | 5,899 → 5,836 | 2,696 → 2,663 | +1.2% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/4096_single_three` | 6,905 → 5,344 | 3,150 → 2,438 | +29.2% | 4 → 3 | 0 → 0 | 24,594 → 16 |
| `stream/4096_mixed_visit` | 11,275 → 11,575 | 5,147 → 5,285 | -2.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/4096_mixed_detailed` | 53,316 → 45,823 | 24,330 → 20,917 | +16.3% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `stream/4096_mixed_partial` | 43,945 → 38,205 | 20,073 → 17,450 | +15.0% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `stream/4096_mixed_simple` | 40,098 → 38,071 | 18,313 → 17,376 | +5.4% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `stream/4096_mixed_total` | 5,604 → 5,905 | 2,557 → 2,702 | -5.4% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/4096_mixed_three` | 81,958 → 73,756 | 37,400 → 33,649 | +11.1% | 4 → 3 | 1 → 0 | 103,212 → 29,484 |
| `stream/4096_leading_visit` | 4,762 → 4,614 | 2,174 → 2,112 | +2.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/4096_leading_detailed` | 5,733 → 5,661 | 2,620 → 2,584 | +1.4% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_leading_partial` | 5,786 → 5,746 | 2,647 → 2,633 | +0.5% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_leading_simple` | 5,878 → 5,610 | 2,689 → 2,565 | +4.8% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_leading_total` | 5,943 → 5,793 | 2,710 → 2,644 | +2.5% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/4096_leading_three` | 6,276 → 5,603 | 2,863 → 2,562 | +11.7% | 4 → 3 | 0 → 0 | 24,612 → 10 |
| `stream/4096_alternating_visit` | 14,705 → 14,685 | 6,718 → 6,709 | +0.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/4096_alternating_detailed` | 56,304 → 43,945 | 25,713 → 20,066 | +28.1% | 2 → 2 | 1 → 1 | 86,016 → 86,016 |
| `stream/4096_alternating_partial` | 55,958 → 43,907 | 25,530 → 20,034 | +27.4% | 2 → 2 | 1 → 1 | 86,016 → 86,016 |
| `stream/4096_alternating_simple` | 35,723 → 35,156 | 16,310 → 16,047 | +1.6% | 2 → 2 | 1 → 1 | 86,016 → 86,016 |
| `stream/4096_alternating_total` | 5,930 → 5,668 | 2,706 → 2,587 | +4.6% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/4096_alternating_three` | 91,187 → 82,242 | 41,613 → 37,531 | +10.9% | 4 → 3 | 1 → 0 | 110,592 → 36,864 |
| `stream/4096_late_visit` | 5,915 → 5,539 | 2,702 → 2,529 | +6.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/4096_late_detailed` | 6,662 → 5,975 | 3,046 → 2,726 | +11.7% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_late_partial` | 6,675 → 6,201 | 3,046 → 2,832 | +7.6% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_late_simple` | 6,692 → 6,082 | 3,056 → 2,774 | +10.2% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_late_total` | 5,949 → 5,430 | 2,720 → 2,479 | +9.7% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/4096_late_three` | 7,152 → 5,206 | 3,276 → 2,383 | +37.5% | 4 → 3 | 0 → 0 | 24,612 → 36 |
| `stream/4096_trailing_visit` | 6,498 → 5,872 | 2,970 → 2,679 | +10.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream/4096_trailing_detailed` | 7,381 → 6,780 | 3,367 → 3,093 | +8.9% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_trailing_partial` | 7,395 → 7,287 | 3,374 → 3,324 | +1.5% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_trailing_simple` | 7,425 → 7,286 | 3,388 → 3,324 | +1.9% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `stream/4096_trailing_total` | 5,779 → 5,801 | 2,637 → 2,648 | -0.4% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/4096_trailing_three` | 7,624 → 5,413 | 3,479 → 2,469 | +40.9% | 4 → 3 | 0 → 0 | 24,612 → 10 |

### Run symbol allocation

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `stream_run/1` | 166 → 168 | 77 → 77 | +0.0% | 1 → 1 | 0 → 0 | 8 → 8 |
| `stream_run/123` | 170 → 171 | 78 → 78 | +0.0% | 1 → 1 | 0 → 0 | 8 → 8 |
| `stream_run/1234567` | 424 → 184 | 194 → 84 | +131.0% | 1 → 1 | 1 → 0 | 24 → 10 |
| `stream_run/18446744073709551615` | 720 → 252 | 329 → 115 | +186.1% | 1 → 1 | 2 → 0 | 56 → 23 |

### Standard stream callers

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `standard/uniform_detailed` | 11,011 → 9,766 | 5,029 → 4,461 | +12.7% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `standard/uniform_partial` | 6,941 → 7,395 | 3,175 → 3,374 | -5.9% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `standard/uniform_simple` | 6,531 → 6,598 | 2,978 → 3,019 | -1.4% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `standard/uniform_total` | 5,911 → 6,572 | 2,706 → 3,001 | -9.8% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/uniform_three` | 6,974 → 5,484 | 3,182 → 2,503 | +27.1% | 4 → 3 | 0 → 0 | 24,594 → 16 |
| `standard/fragmented_detailed` | 47,523 → 40,759 | 21,694 → 18,607 | +16.6% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `standard/fragmented_partial` | 46,924 → 42,237 | 21,418 → 19,286 | +11.1% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `standard/fragmented_simple` | 43,077 → 42,904 | 19,661 → 19,585 | +0.4% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `standard/fragmented_total` | 5,740 → 6,214 | 2,621 → 2,835 | -7.5% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/fragmented_three` | 85,605 → 74,061 | 39,086 → 33,812 | +15.6% | 4 → 3 | 1 → 0 | 103,212 → 29,484 |
| `standard/empty_detailed` | 4,965 → 5,119 | 2,267 → 2,343 | -3.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_partial` | 4,963 → 5,233 | 2,266 → 2,395 | -5.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_simple` | 4,957 → 5,084 | 2,267 → 2,319 | -2.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_total` | 5,840 → 5,892 | 2,664 → 2,694 | -1.1% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_three` | 5,355 → 5,446 | 2,445 → 2,484 | -1.6% | 3 → 3 | 0 → 0 | 33 → 33 |
| `standard/short_detailed` | 717 → 589 | 330 → 269 | +22.7% | 2 → 2 | 0 → 0 | 492 → 492 |
| `standard/short_partial` | 720 → 603 | 331 → 276 | +19.9% | 2 → 2 | 0 → 0 | 492 → 492 |
| `standard/short_simple` | 657 → 645 | 300 → 294 | +2.0% | 2 → 2 | 0 → 0 | 492 → 492 |
| `standard/short_total` | 210 → 214 | 96 → 98 | -2.0% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/short_three` | 1,315 → 1,108 | 600 → 506 | +18.6% | 4 → 3 | 0 → 0 | 660 → 252 |
| `standard/leading_detailed` | 5,758 → 5,887 | 2,626 → 2,687 | -2.3% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `standard/leading_partial` | 5,747 → 5,865 | 2,625 → 2,675 | -1.9% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `standard/leading_simple` | 5,775 → 5,836 | 2,638 → 2,662 | -0.9% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `standard/leading_total` | 6,245 → 6,719 | 2,849 → 3,066 | -7.1% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/leading_three` | 6,080 → 5,333 | 2,774 → 2,434 | +14.0% | 4 → 3 | 0 → 0 | 24,612 → 12 |

### Stream outputs and SN controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `streams/uniform_combined` | 24,123 → 24,131 | 11,019 → 11,020 | -0.0% | 6 → 6 | 0 → 0 | 33 → 33 |
| `streams/uniform_cold` | 24,377 → 24,049 | 11,125 → 10,976 | +1.4% | 7 → 7 | 0 → 0 | 16,417 → 16,417 |
| `streams/fragmented_combined` | 216,392 → 179,428 | 98,773 → 81,894 | +20.6% | 6 → 6 | 0 → 0 | 72,039 → 72,039 |
| `streams/fragmented_cold` | 220,248 → 181,598 | 100,533 → 82,894 | +21.3% | 7 → 7 | 2 → 2 | 186,727 → 186,727 |
| `streams/empty_combined` | 5,216 → 5,269 | 2,381 → 2,411 | -1.2% | 3 → 3 | 0 → 0 | 33 → 33 |
| `streams/empty_cold` | 5,248 → 5,249 | 2,394 → 2,395 | -0.0% | 3 → 3 | 0 → 0 | 33 → 33 |
| `streams/short_combined` | 3,777 → 3,323 | 1,724 → 1,516 | +13.7% | 6 → 6 | 0 → 0 | 891 → 891 |
| `streams/short_cold` | 3,938 → 3,520 | 1,797 → 1,607 | +11.8% | 7 → 7 | 0 → 0 | 1,403 → 1,403 |
| `streams/leading_combined` | 6,219 → 6,304 | 2,844 → 2,880 | -1.2% | 6 → 6 | 0 → 0 | 33 → 33 |
| `streams/leading_cold` | 6,783 → 6,626 | 3,101 → 3,025 | +2.5% | 7 → 7 | 0 → 0 | 1,569 → 1,569 |
| `sn/uniform_detailed` | 12,107 → 11,704 | 5,522 → 5,344 | +3.3% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/uniform_partial` | 11,986 → 11,922 | 5,473 → 5,440 | +0.6% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/uniform_simple` | 11,664 → 11,923 | 5,330 → 5,457 | -2.3% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/uniform_three` | 12,254 → 12,089 | 5,594 → 5,518 | +1.4% | 3 → 3 | 0 → 0 | 480 → 480 |
| `sn/fragmented_detailed` | 82,503 → 76,172 | 37,645 → 34,764 | +8.3% | 1 → 1 | 6 → 6 | 20,320 → 20,320 |
| `sn/fragmented_partial` | 73,995 → 69,071 | 33,777 → 31,514 | +7.2% | 1 → 1 | 5 → 5 | 10,080 → 10,080 |
| `sn/fragmented_simple` | 71,596 → 66,604 | 32,664 → 30,393 | +7.5% | 1 → 1 | 5 → 5 | 10,080 → 10,080 |
| `sn/fragmented_three` | 153,574 → 136,509 | 70,097 → 62,313 | +12.5% | 3 → 3 | 16 → 16 | 40,480 → 40,480 |
| `sn/empty_detailed` | 4,833 → 4,964 | 2,212 → 2,267 | -2.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/empty_partial` | 4,707 → 4,847 | 2,147 → 2,218 | -3.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/empty_simple` | 4,692 → 4,804 | 2,140 → 2,192 | -2.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/empty_three` | 4,651 → 4,730 | 2,127 → 2,158 | -1.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/short_detailed` | 1,169 → 979 | 535 → 449 | +19.2% | 1 → 1 | 0 → 0 | 155 → 155 |
| `sn/short_partial` | 1,054 → 911 | 481 → 421 | +14.3% | 1 → 1 | 0 → 0 | 155 → 155 |
| `sn/short_simple` | 1,047 → 883 | 478 → 403 | +18.6% | 1 → 1 | 0 → 0 | 155 → 155 |
| `sn/short_three` | 2,552 → 2,187 | 1,165 → 1,000 | +16.5% | 3 → 3 | 0 → 0 | 465 → 465 |
| `sn/leading_detailed` | 5,272 → 5,246 | 2,406 → 2,393 | +0.5% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/leading_partial` | 5,315 → 5,269 | 2,427 → 2,403 | +1.0% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/leading_simple` | 5,514 → 5,415 | 2,515 → 2,471 | +1.8% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/leading_three` | 5,558 → 5,736 | 2,539 → 2,616 | -2.9% | 3 → 3 | 0 → 0 | 480 → 480 |

### Hash, analysis, report and cleanup controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `hash_batch/1_global` | 23,672 → 22,724 | 10,806 → 10,367 | +4.2% | 28 → 28 | 2 → 2 | 5,283 → 5,283 |
| `hash_batch/1_repeat` | 23,849 → 23,365 | 10,878 → 10,662 | +2.0% | 29 → 29 | 4 → 4 | 5,318 → 5,318 |
| `hash_batch/1_vary` | 24,162 → 24,273 | 11,044 → 11,080 | -0.3% | 29 → 29 | 4 → 4 | 5,318 → 5,318 |
| `hash_batch/1_distinct` | 31,179 → 31,117 | 14,228 → 14,204 | +0.2% | 36 → 36 | 18 → 18 | 5,563 → 5,563 |
| `hash_batch/128_global` | 155,152 → 147,475 | 70,842 → 67,326 | +5.2% | 28 → 28 | 2 → 2 | 12,213 → 12,213 |
| `hash_batch/128_repeat` | 198,666 → 191,521 | 90,708 → 87,419 | +3.8% | 29 → 29 | 4 → 4 | 19,178 → 19,178 |
| `hash_batch/128_vary` | 194,594 → 188,300 | 88,814 → 85,958 | +3.3% | 29 → 29 | 4 → 4 | 19,178 → 19,178 |
| `hash_batch/128_distinct` | 512,275 → 472,371 | 233,826 → 215,621 | +8.4% | 36 → 36 | 18 → 18 | 67,933 → 67,933 |
| `analyze/fast_fake_lifts` | 648,627 → 665,891 | 296,261 → 304,541 | -2.7% | 31 → 31 | 4 → 4 | 59,028 → 59,028 |
| `analyze/camellia` | 453,608,034 → 462,040,541 | 207,131,820 → 210,955,520 | -1.8% | 110 → 110 | 0 → 0 | 5,263,624 → 5,263,624 |
| `analyze/fast_camellia` | 57,753,063 → 56,283,838 | 26,389,810 → 25,701,490 | +2.7% | 115 → 115 | 0 → 0 | 7,051,152 → 7,051,152 |
| `analyze/mixed_small` | 51,240 → 49,333 | 23,378 → 22,655 | +3.2% | 59 → 59 | 3 → 3 | 7,460 → 7,460 |
| `report/json/16_clean` | 89,778 → 84,734 | 40,985 → 38,663 | +6.0% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_early` | 85,502 → 85,844 | 39,089 → 39,178 | -0.2% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_late` | 87,526 → 84,854 | 39,931 → 38,715 | +3.1% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_dense` | 87,947 → 85,225 | 40,131 → 38,885 | +3.2% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_comma` | 85,304 → 83,788 | 39,000 → 38,295 | +1.8% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_comma_quote` | 85,069 → 84,448 | 38,811 → 38,549 | +0.7% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_custom` | 84,716 → 85,010 | 38,644 → 38,881 | -0.6% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_clean` | 128,278 → 123,820 | 58,597 → 56,503 | +3.7% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_early` | 148,391 → 144,609 | 67,708 → 65,986 | +2.6% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_late` | 188,981 → 185,899 | 86,248 → 84,798 | +1.7% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_dense` | 328,431 → 318,328 | 150,208 → 145,323 | +3.4% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_comma` | 127,284 → 124,885 | 58,176 → 56,956 | +2.1% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_comma_quote` | 186,263 → 186,729 | 85,167 → 85,280 | -0.1% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_custom` | 131,650 → 123,414 | 60,203 → 56,309 | +6.9% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/camellia` | 21,275,485 → 21,121,851 | 9,713,845 → 9,643,297 | +0.7% | 30 → 30 | 0 → 0 | 850 → 850 |
| `cleanup/pair_1_clean` | 702 → 709 | 328 → 331 | -0.9% | 1 → 1 | 1 → 1 | 27 → 27 |
| `cleanup/speed_1_clean` | 902 → 922 | 419 → 428 | -2.1% | 1 → 1 | 1 → 1 | 36 → 36 |
| `cleanup/pair_1_early` | 803 → 803 | 375 → 375 | +0.0% | 2 → 2 | 0 → 0 | 26 → 26 |
| `cleanup/speed_1_early` | 1,879 → 1,313 | 864 → 640 | +35.0% | 2 → 2 | 1 → 1 | 64 → 64 |
| `cleanup/pair_1_late` | 751 → 803 | 350 → 374 | -6.4% | 2 → 2 | 0 → 0 | 26 → 26 |
| `cleanup/speed_1_late` | 1,256 → 1,267 | 580 → 585 | -0.9% | 2 → 2 | 1 → 1 | 64 → 64 |
| `cleanup/pair_128_clean` | 43,986 → 40,809 | 20,063 → 18,621 | +7.7% | 1 → 1 | 1 → 1 | 4,521 → 4,521 |
| `cleanup/speed_128_clean` | 70,809 → 70,174 | 32,300 → 32,023 | +0.9% | 1 → 1 | 1 → 1 | 5,673 → 5,673 |
| `cleanup/pair_128_early` | 49,346 → 48,696 | 22,547 → 22,215 | +1.5% | 2 → 2 | 1 → 1 | 6,044 → 6,044 |
| `cleanup/speed_128_early` | 75,980 → 72,764 | 34,672 → 33,270 | +4.2% | 2 → 2 | 1 → 1 | 7,580 → 7,580 |
| `cleanup/pair_128_late` | 54,403 → 53,367 | 24,847 → 24,419 | +1.8% | 2 → 2 | 1 → 1 | 6,044 → 6,044 |
| `cleanup/speed_128_late` | 83,114 → 81,465 | 38,038 → 37,228 | +2.2% | 2 → 2 | 1 → 1 | 7,580 → 7,580 |
| `cleanup/pair_4096_clean` | 1,463,846 → 1,412,482 | 668,174 → 644,869 | +3.6% | 1 → 1 | 1 → 1 | 163,695 → 163,695 |
| `cleanup/speed_4096_clean` | 2,310,938 → 2,301,407 | 1,054,614 → 1,052,117 | +0.2% | 1 → 1 | 1 → 1 | 200,559 → 200,559 |
| `cleanup/pair_4096_early` | 1,869,852 → 1,875,118 | 853,564 → 855,799 | -0.3% | 2 → 2 | 1 → 1 | 218,276 → 218,276 |
| `cleanup/speed_4096_early` | 2,491,650 → 2,324,523 | 1,145,341 → 1,061,104 | +7.9% | 2 → 2 | 1 → 1 | 267,428 → 267,428 |
| `cleanup/pair_4096_late` | 1,843,087 → 1,800,565 | 841,638 → 821,807 | +2.4% | 2 → 2 | 1 → 1 | 218,276 → 218,276 |
| `cleanup/speed_4096_late` | 2,733,328 → 2,717,625 | 1,249,106 → 1,240,487 | +0.7% | 2 → 2 | 1 → 1 | 267,428 → 267,428 |
| `cleanup/0_ordered` | 904 → 913 | 420 → 424 | -0.9% | 2 → 2 | 0 → 0 | 24 → 24 |
| `cleanup/0_duplicates` | 904 → 913 | 420 → 424 | -0.9% | 2 → 2 | 0 → 0 | 24 → 24 |
| `cleanup/0_reverse` | 902 → 913 | 419 → 424 | -1.2% | 2 → 2 | 0 → 0 | 24 → 24 |
| `cleanup/1_ordered` | 4,588 → 4,023 | 2,098 → 1,842 | +13.9% | 14 → 14 | 0 → 0 | 192 → 192 |
| `cleanup/1_duplicates` | 4,181 → 4,157 | 1,913 → 1,902 | +0.6% | 14 → 14 | 0 → 0 | 192 → 192 |
| `cleanup/1_reverse` | 4,689 → 4,162 | 2,167 → 1,903 | +13.9% | 14 → 14 | 0 → 0 | 192 → 192 |
| `cleanup/32_ordered` | 52,381 → 50,913 | 23,897 → 23,384 | +2.2% | 14 → 14 | 0 → 0 | 5,528 → 5,528 |
| `cleanup/32_duplicates` | 49,438 → 48,970 | 22,551 → 22,337 | +1.0% | 14 → 14 | 0 → 0 | 4,600 → 4,600 |
| `cleanup/32_reverse` | 57,643 → 58,398 | 26,291 → 26,655 | -1.4% | 24 → 24 | 0 → 0 | 10,136 → 10,136 |
| `cleanup/4096_ordered` | 6,399,575 → 6,319,932 | 2,921,802 → 2,885,001 | +1.3% | 14 → 14 | 0 → 0 | 731,704 → 731,704 |
| `cleanup/4096_duplicates` | 6,060,331 → 5,966,260 | 2,766,582 → 2,723,698 | +1.6% | 14 → 14 | 0 → 0 | 611,032 → 611,032 |
| `cleanup/4096_reverse` | 38,810,418 → 38,926,716 | 17,749,295 → 17,775,747 | -0.1% | 24 → 24 | 0 → 0 | 1,321,528 → 1,321,528 |

### Focused recheck

These comparisons use the final sources and original executables with identical lifetimes: 20,000 iterations for every selected core/standard case, followed by 100,000 iterations for the five uniform 32-measure controls. Main samples remain above. The small single-output Simple case was 10.4% slower at 20,000 iterations and 3.1% slower at 100,000 (185 → 191 ns); adjacent Detailed/Partial/Total controls differ by 0–1.6%. This is a CPU tradeoff in the preserved collecting path, not a behavioral change or an allocation increase. All recheck allocation metrics remain non-increasing.

### Focused results

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `stream/4096_single_detailed` | 6,523 → 6,501 | 2,978 → 2,966 | +0.4% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `stream/4096_single_partial` | 6,502 → 6,473 | 2,968 → 2,954 | +0.5% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `stream/4096_single_simple` | 8,321 → 7,805 | 4,626 → 3,671 | +26.0% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `stream/4096_single_visit` | 5,790 → 5,705 | 2,643 → 2,605 | +1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `stream_run/1` | 162 → 164 | 74 → 75 | -1.3% | 1 → 1 | 0 → 0 | 8 → 8 |
| `stream_run/123` | 167 → 167 | 76 → 76 | +0.0% | 1 → 1 | 0 → 0 | 8 → 8 |
| `stream_run/1234567` | 422 → 182 | 192 → 83 | +131.3% | 1 → 1 | 1 → 0 | 24 → 10 |
| `stream_run/18446744073709551615` | 729 → 242 | 332 → 111 | +199.1% | 1 → 1 | 2 → 0 | 56 → 23 |
| `stream/0_trailing_simple` | 150 → 157 | 69 → 72 | -4.2% | 1 → 1 | 0 → 0 | 11 → 11 |
| `stream/32_single_partial` | 387 → 388 | 177 → 178 | -0.6% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_simple` | 394 → 440 | 180 → 201 | -10.4% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_total` | 212 → 210 | 97 → 96 | +1.0% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_mixed_visit` | 96 → 101 | 44 → 46 | -4.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `standard/uniform_total` | 6,540 → 5,813 | 2,984 → 2,652 | +12.5% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/fragmented_total` | 5,828 → 5,788 | 2,660 → 2,641 | +0.7% | 1 → 1 | 0 → 0 | 26 → 26 |

### Small uniform control recheck

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `stream/32_single_detailed` | 396 → 397 | 181 → 181 | +0.0% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_partial` | 407 → 402 | 186 → 183 | +1.6% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_simple` | 406 → 418 | 185 → 191 | -3.1% | 2 → 2 | 0 → 0 | 414 → 414 |
| `stream/32_single_total` | 212 → 214 | 97 → 98 | -1.0% | 1 → 1 | 0 → 0 | 26 → 26 |
| `stream/32_single_three` | 692 → 547 | 316 → 250 | +26.4% | 4 → 3 | 0 → 0 | 426 → 12 |

### Validation

- Release regression tests: 149 core + 79 rssp + 29 integration tests = 257 passed, zero failed.
- After confirming the optimizations: `cargo test --release --test all_parity -- --test-threads=22` — 30,489 passed, zero failed.
- Strict release workspace Clippy for all targets, formatting, and diff whitespace checks pass.
- Exact stream comparison exhausts all 4,096 twelve-measure stream/break patterns and the benchmark edge fixtures; decimal boundaries cover zero, digit transitions, an existing prefix, and `u64::MAX`.
- Complete corpus output remains byte-identical: 30,843 files, 56,125 supported charts, 30,489 successes and 354 matched errors. Outputs include reports, hashes, timings, labels, durations and NPS. No golden data is changed.
- Corpus: 174,790,323 UTF-8 bytes; SHA-256 `e7d2f22bd7b7f48c0335075d8d2ac355063b58809fdd77c42dcec23927a5759d`.
- Core trace: 4,276 rows, 2,184,815 UTF-8 bytes; SHA-256 `ce3a83d70e36729241907bb26d070e391ac01ca12b439cb764a0a0c976ba1372`.
- Leaf trace: 57 rows, 29,124,186 UTF-8 bytes; SHA-256 `aac4e521dea9affd1f0e1baa95991e7dd0fa7f4bb10d109b82bed5ff84d290d7`.

### Reproduction

Keep the current harness/fixtures and package version for both builds; restore only the modified production functions in `math.rs` and `streams.rs` to `3772598` for the original build. Keep the test module declarations and save original executables before rebuilding optimized sources. Use identical setup, output destruction and buffer reuse for both versions.

```powershell
cargo test --release -p rssp-core --lib
cargo test --release -p rssp --lib
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_PASS_FILTER='stream/4096_'
$env:RSSP_PASS_ITERS='1000'
.\saved-core.exe pass_edges --skip _trace --ignored --nocapture --test-threads=1
# Use uint/ or decimal/ with 100000 iterations, stream_run/ with 20000.
# Focused checks: selected core stream paths/symbols and standard callers use 20000.
# The five uniform 32-measure core controls additionally use 100000.
$env:RSSP_HOT_FILTER='standard/'
$env:RSSP_HOT_ITERS='1000'
.\saved-hotpath.exe
# Alternate old/new, new/old, old/new; compare exact corpus and _trace outputs separately.
cargo clippy --release --workspace --all-targets -- -D warnings
cargo test --release --test optimization_edges
cargo test --release --test all_parity -- --test-threads=22
```

## Pass 0.4.287: avoid temporary summary and timing storage

Baseline: `b22ddef` (0.4.286). This pass increments the workspace patch version exactly once to **0.4.287** and retains three optimizations:

1. **Use the existing 32-value stack selection for owned BPM summaries.** Fold the display range while collecting selected values through a monomorphized visitor; existing statistics-only calls use a no-op visitor. This removes the temporary vector for maps of 2–32 entries and non-finite singletons while preserving filtering, fallback ordering, median selection, and average accumulation. Delete the forwarding scratch wrapper and put its unchanged implementation under the public signature. Caller-owned scratch contents and capacity behavior are preserved.
2. **Extend owned NPS stack selection to 128 measures.** Keep the existing 64-value buffer for inputs up to that boundary, and use a 128-value buffer for 65–128 measures. Both sizes share the same bounded selection function. The existing uniform/zero-majority scans still bypass median copying; larger inputs use the original heap path. Caller-owned scratch and in-place APIs retain their original behavior. Empty, singleton and pair results stay in a small scalar entry point.
3. **Reuse a sole exact-sized owned timing source.** A delay, warp or fake vector already has the final packed layout when it is the only nonempty source. Return it with the corresponding prefix offsets rather than allocate a destination, copy it, and free it. A stop-only source already serves as the destination. Spare-capacity sources and mixed sources keep the original destination/reservation policy, preventing increased retained output storage.

The two buffer functions are explicitly kept out of line. This measured layout keeps bounded array storage out of scalar entry points. An ordinary inlining hint did not remove the scalar overhead and was discarded. No sorting rule or numeric operation is replaced, and no cache, dependency or dynamic dispatch is introduced.

### Representative final measurements

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `bpm_summary/8_dense_owned` | 253 → 135 | 115 → 62 | +85.5% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/32_reverse_owned` | 686 → 517 | 313 → 236 | +32.6% | 1 → 0 | 0 → 0 | 256 → 0 |
| `nps_summary/65_dense_owned` | 955 → 802 | 437 → 365 | +19.7% | 1 → 0 | 0 → 0 | 520 → 0 |
| `nps_summary/128_dense_owned` | 1,475 → 1,244 | 677 → 571 | +18.6% | 1 → 0 | 0 → 0 | 1,024 → 0 |
| `pack_timing/4096_2_false` | 59,911 → 27,958 | 27,422 → 12,854 | +113.3% | 1 → 0 | 0 → 0 | 65,536 → 0 |
| `pack_raw/32_2` | 8,442 → 7,910 | 3,855 → 3,611 | +6.8% | 4 → 3 | 0 → 0 | 1,168 → 656 |
| `pack_raw/4096_2` | 1,033,347 → 834,600 | 471,613 → 381,044 | +23.8% | 4 → 3 | 0 → 0 | 147,472 → 81,936 |

### Method

Windows x86-64, Intel Xeon E5-2696 v4, 44 logical processors, benchmark thread pinned to CPU 2; rustc 1.98.1, LLVM 22.1.8. Original and final builds use the same current fixtures, harnesses and package version; the original build restores only the production implementations from `b22ddef`. Cargo release/bench profiles use fat LTO and one codegen unit.

Four warmup calls and seven timed batches per process; three alternating process pairs (old/new, new/old, old/new). Tables report the median of the three batch medians. Windows `QueryThreadCycleTime` measures thread CPU cycles; wall-clock ns and derived call throughput are also shown. Rounded nanoseconds limit precision for tiny calls. The counting System allocator runs separately from timing; bytes are successful allocation/reallocation requests per call, **not peak live memory or RSS**.

Fixture construction and labels stay outside measurement. Owned statistics and returned packed/built timing outputs are destroyed inside both timed loops. Cold scratch inputs are prepared empty before the timer and destroyed after each batch; warm buffers are prepared with capacity and reused. In-place inputs are cloned before timing. Raw timing parsing is deliberately included in the composed builder benchmark. No own builds, tests or corpus verification run during the final timed comparisons.

The main sweep has 648 cases: 240 BPM summaries, 200 NPS summaries, 64 direct packing cases, 32 raw timing builders, and 112 stream/hash/analysis/report/cleanup controls. Summary and packing cases below 128 elements use 5,000 iterations; larger cases use 1,000, except 4,096-entry raw builders use 100. Stream/hash controls use 1,000; analysis/report/cleanup use 100 (existing Camellia full analysis uses 10). Longer scalar and summary checks use 100,000 iterations. Additional focused checks retain slower main samples instead of replacing them. All measured allocation counts, reallocations and requested bytes are non-increasing.

Unrelated Cargo/rustc work in other repositories was observed on the host during control rechecks; it was left running. CPU samples on this host are variable, so the deterministic allocation reductions and exact-output comparisons provide stronger evidence than isolated percentage changes. Longer alternating rechecks below expose that variability rather than attributing every control change to this patch.

The longer 8-entry warm BPM recheck is 139 -> 141 cycles (64 -> 65 ns), and the filtered 128-entry map recheck is 1,415 -> 1,444 cycles (646 -> 659 ns). The mixed delay/warp builder changes from a slower main/large sample to +4.3% throughput in its final recheck. Original/original calibration varies by up to 7.9% throughput on a stream control. These results do not justify a uniform CPU-speed claim.

CPU gains depend on input and code layout. Some controls are slower, including tiny scalar BPM calls; the complete tables and rechecks below expose those costs. This pass claims lower allocation churn for the specified paths and unchanged observable outputs, not uniformly higher throughput for every call.

The baseline sorter already panics on the 32-entry mixed NaN/infinity fallback fixture because its partial comparison does not define a total order. Mixed non-finite benchmark fixtures stay at 1, 2 and 8 entries; larger special BPM fixtures use homogeneous NaNs. The public BPM parser accepts non-finite values. This pass preserves the existing comparison behavior; it does not fix that pre-existing limitation.

### BPM summary paths and controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `bpm_summary/0_uniform_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_uniform_map` | 14 → 14 | 7 → 6 | +16.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_uniform_owned` | 3 → 7 | 2 → 3 | -33.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_uniform_cold` | 20 → 21 | 10 → 10 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_uniform_warm` | 20 → 19 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_reverse_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_reverse_map` | 14 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_reverse_owned` | 3 → 6 | 2 → 3 | -33.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_reverse_cold` | 20 → 21 | 9 → 10 | -10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_reverse_warm` | 19 → 20 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_dense_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_dense_map` | 13 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_dense_owned` | 3 → 7 | 2 → 3 | -33.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_dense_cold` | 21 → 21 | 10 → 10 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_dense_warm` | 20 → 19 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_filtered_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_filtered_map` | 13 → 14 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_filtered_owned` | 3 → 6 | 2 → 3 | -33.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_filtered_cold` | 20 → 24 | 9 → 12 | -25.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_filtered_warm` | 20 → 20 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_fallback_values` | 12 → 11 | 6 → 5 | +20.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_fallback_map` | 14 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_fallback_owned` | 3 → 6 | 2 → 3 | -33.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_fallback_cold` | 20 → 21 | 9 → 10 | -10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_fallback_warm` | 18 → 20 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_special_values` | 12 → 12 | 5 → 6 | -16.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_special_map` | 14 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_special_owned` | 3 → 6 | 2 → 3 | -33.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_special_cold` | 21 → 20 | 10 → 9 | +11.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/0_special_warm` | 19 → 20 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_uniform_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_uniform_map` | 13 → 14 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_uniform_owned` | 21 → 24 | 10 → 11 | -9.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_uniform_cold` | 227 → 222 | 104 → 102 | +2.0% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/1_uniform_warm` | 41 → 48 | 19 → 22 | -13.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_reverse_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_reverse_map` | 13 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_reverse_owned` | 21 → 23 | 10 → 11 | -9.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_reverse_cold` | 236 → 222 | 109 → 101 | +7.9% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/1_reverse_warm` | 40 → 44 | 19 → 20 | -5.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_dense_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_dense_map` | 13 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_dense_owned` | 22 → 22 | 10 → 10 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_dense_cold` | 227 → 221 | 104 → 101 | +3.0% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/1_dense_warm` | 40 → 40 | 18 → 18 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_filtered_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_filtered_map` | 13 → 14 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_filtered_owned` | 21 → 22 | 10 → 10 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_filtered_cold` | 224 → 232 | 103 → 107 | -3.7% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/1_filtered_warm` | 38 → 39 | 18 → 18 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_fallback_values` | 12 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_fallback_map` | 14 → 14 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_fallback_owned` | 21 → 22 | 10 → 10 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_fallback_cold` | 226 → 220 | 103 → 101 | +2.0% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/1_fallback_warm` | 38 → 39 | 18 → 18 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_special_values` | 37 → 40 | 17 → 18 | -5.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_special_map` | 40 → 41 | 18 → 19 | -5.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_special_owned` | 169 → 61 | 77 → 28 | +175.0% | 1 → 0 | 0 → 0 | 8 → 0 |
| `bpm_summary/1_special_cold` | 222 → 224 | 101 → 102 | -1.0% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/1_special_warm` | 39 → 40 | 18 → 18 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_uniform_values` | 53 → 55 | 24 → 25 | -4.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_uniform_map` | 59 → 57 | 27 → 26 | +3.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_uniform_owned` | 184 → 77 | 84 → 35 | +140.0% | 1 → 0 | 0 → 0 | 16 → 0 |
| `bpm_summary/2_uniform_cold` | 251 → 257 | 114 → 118 | -3.4% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/2_uniform_warm` | 58 → 59 | 27 → 27 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_reverse_values` | 66 → 64 | 30 → 29 | +3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_reverse_map` | 66 → 69 | 30 → 32 | -6.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_reverse_owned` | 200 → 80 | 91 → 37 | +145.9% | 1 → 0 | 0 → 0 | 16 → 0 |
| `bpm_summary/2_reverse_cold` | 246 → 262 | 113 → 120 | -5.8% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/2_reverse_warm` | 65 → 63 | 30 → 29 | +3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_dense_values` | 57 → 57 | 26 → 26 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_dense_map` | 61 → 57 | 28 → 26 | +7.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_dense_owned` | 187 → 69 | 85 → 32 | +165.6% | 1 → 0 | 0 → 0 | 16 → 0 |
| `bpm_summary/2_dense_cold` | 252 → 261 | 115 → 119 | -3.4% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/2_dense_warm` | 58 → 58 | 27 → 26 | +3.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_filtered_values` | 36 → 37 | 17 → 17 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_filtered_map` | 42 → 41 | 19 → 19 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_filtered_owned` | 176 → 61 | 80 → 28 | +185.7% | 1 → 0 | 0 → 0 | 16 → 0 |
| `bpm_summary/2_filtered_cold` | 223 → 232 | 102 → 106 | -3.8% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/2_filtered_warm` | 42 → 42 | 19 → 19 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_fallback_values` | 66 → 73 | 30 → 34 | -11.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_fallback_map` | 69 → 66 | 31 → 30 | +3.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_fallback_owned` | 193 → 89 | 88 → 41 | +114.6% | 1 → 0 | 0 → 0 | 16 → 0 |
| `bpm_summary/2_fallback_cold` | 245 → 242 | 112 → 111 | +0.9% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/2_fallback_warm` | 63 → 60 | 29 → 27 | +7.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_special_values` | 62 → 59 | 28 → 27 | +3.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_special_map` | 57 → 57 | 26 → 26 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_special_owned` | 186 → 73 | 85 → 33 | +157.6% | 1 → 0 | 0 → 0 | 16 → 0 |
| `bpm_summary/2_special_cold` | 258 → 245 | 120 → 113 | +6.2% | 1 → 1 | 0 → 0 | 32 → 32 |
| `bpm_summary/2_special_warm` | 64 → 59 | 29 → 27 | +7.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_uniform_values` | 70 → 67 | 32 → 31 | +3.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_uniform_map` | 76 → 65 | 36 → 30 | +20.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_uniform_owned` | 246 → 129 | 112 → 61 | +83.6% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/8_uniform_cold` | 306 → 295 | 140 → 135 | +3.7% | 1 → 1 | 0 → 0 | 64 → 64 |
| `bpm_summary/8_uniform_warm` | 119 → 101 | 55 → 46 | +19.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_reverse_values` | 111 → 114 | 51 → 53 | -3.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_reverse_map` | 99 → 104 | 45 → 47 | -4.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_reverse_owned` | 272 → 143 | 124 → 65 | +90.8% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/8_reverse_cold` | 346 → 363 | 158 → 166 | -4.8% | 1 → 1 | 0 → 0 | 64 → 64 |
| `bpm_summary/8_reverse_warm` | 131 → 134 | 60 → 61 | -1.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_dense_values` | 85 → 96 | 39 → 44 | -11.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_dense_map` | 85 → 92 | 39 → 42 | -7.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_dense_owned` | 253 → 135 | 115 → 62 | +85.5% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/8_dense_cold` | 322 → 312 | 147 → 142 | +3.5% | 1 → 1 | 0 → 0 | 64 → 64 |
| `bpm_summary/8_dense_warm` | 130 → 210 | 60 → 96 | -37.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_filtered_values` | 63 → 63 | 29 → 29 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_filtered_map` | 63 → 60 | 29 → 27 | +7.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_filtered_owned` | 218 → 84 | 100 → 39 | +156.4% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/8_filtered_cold` | 272 → 307 | 125 → 140 | -10.7% | 1 → 1 | 0 → 0 | 64 → 64 |
| `bpm_summary/8_filtered_warm` | 76 → 75 | 35 → 34 | +2.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_fallback_values` | 113 → 99 | 52 → 45 | +15.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_fallback_map` | 98 → 105 | 45 → 48 | -6.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_fallback_owned` | 259 → 152 | 118 → 70 | +68.6% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/8_fallback_cold` | 406 → 369 | 185 → 169 | +9.5% | 1 → 1 | 0 → 0 | 64 → 64 |
| `bpm_summary/8_fallback_warm` | 140 → 155 | 64 → 71 | -9.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_special_values` | 71 → 80 | 33 → 37 | -10.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_special_map` | 86 → 75 | 39 → 34 | +14.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/8_special_owned` | 245 → 108 | 112 → 49 | +128.6% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/8_special_cold` | 338 → 289 | 155 → 132 | +17.4% | 1 → 1 | 0 → 0 | 64 → 64 |
| `bpm_summary/8_special_warm` | 107 → 104 | 49 → 48 | +2.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_uniform_values` | 289 → 295 | 132 → 135 | -2.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_uniform_map` | 286 → 291 | 131 → 133 | -1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_uniform_owned` | 655 → 511 | 299 → 233 | +28.3% | 1 → 0 | 0 → 0 | 256 → 0 |
| `bpm_summary/32_uniform_cold` | 877 → 866 | 400 → 398 | +0.5% | 1 → 1 | 0 → 0 | 256 → 256 |
| `bpm_summary/32_uniform_warm` | 501 → 497 | 229 → 227 | +0.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_reverse_values` | 324 → 348 | 148 → 159 | -6.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_reverse_map` | 321 → 299 | 146 → 137 | +6.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_reverse_owned` | 686 → 517 | 313 → 236 | +32.6% | 1 → 0 | 0 → 0 | 256 → 0 |
| `bpm_summary/32_reverse_cold` | 866 → 882 | 397 → 406 | -2.2% | 1 → 1 | 0 → 0 | 256 → 256 |
| `bpm_summary/32_reverse_warm` | 520 → 504 | 237 → 231 | +2.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_dense_values` | 733 → 738 | 334 → 337 | -0.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_dense_map` | 724 → 757 | 332 → 347 | -4.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_dense_owned` | 1,121 → 950 | 511 → 434 | +17.7% | 1 → 0 | 0 → 0 | 256 → 0 |
| `bpm_summary/32_dense_cold` | 1,320 → 1,328 | 605 → 608 | -0.5% | 1 → 1 | 0 → 0 | 256 → 256 |
| `bpm_summary/32_dense_warm` | 981 → 973 | 448 → 444 | +0.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_filtered_values` | 201 → 199 | 92 → 91 | +1.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_filtered_map` | 187 → 208 | 86 → 95 | -9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_filtered_owned` | 427 → 278 | 195 → 127 | +53.5% | 1 → 0 | 0 → 0 | 256 → 0 |
| `bpm_summary/32_filtered_cold` | 619 → 644 | 283 → 294 | -3.7% | 1 → 1 | 0 → 0 | 256 → 256 |
| `bpm_summary/32_filtered_warm` | 280 → 278 | 128 → 127 | +0.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_fallback_values` | 785 → 777 | 358 → 355 | +0.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_fallback_map` | 820 → 791 | 374 → 364 | +2.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_fallback_owned` | 1,171 → 1,021 | 534 → 465 | +14.8% | 1 → 0 | 0 → 0 | 256 → 0 |
| `bpm_summary/32_fallback_cold` | 1,398 → 1,369 | 640 → 625 | +2.4% | 1 → 1 | 0 → 0 | 256 → 256 |
| `bpm_summary/32_fallback_warm` | 1,039 → 1,065 | 474 → 486 | -2.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_special_values` | 263 → 267 | 121 → 122 | -0.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_special_map` | 300 → 269 | 137 → 123 | +11.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/32_special_owned` | 675 → 519 | 308 → 237 | +30.0% | 1 → 0 | 0 → 0 | 256 → 0 |
| `bpm_summary/32_special_cold` | 873 → 873 | 399 → 399 | +0.0% | 1 → 1 | 0 → 0 | 256 → 256 |
| `bpm_summary/32_special_warm` | 523 → 516 | 239 → 235 | +1.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/33_uniform_values` | 494 → 500 | 225 → 229 | -1.7% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_uniform_map` | 464 → 495 | 212 → 226 | -6.2% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_uniform_owned` | 665 → 688 | 304 → 314 | -3.2% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_uniform_cold` | 894 → 892 | 408 → 407 | +0.2% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_uniform_warm` | 502 → 499 | 229 → 227 | +0.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/33_reverse_values` | 534 → 521 | 244 → 238 | +2.5% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_reverse_map` | 502 → 499 | 229 → 228 | +0.4% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_reverse_owned` | 673 → 702 | 307 → 320 | -4.1% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_reverse_cold` | 913 → 862 | 417 → 394 | +5.8% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_reverse_warm` | 511 → 527 | 233 → 240 | -2.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/33_dense_values` | 1,387 → 1,319 | 633 → 603 | +5.0% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_dense_map` | 1,329 → 1,336 | 607 → 610 | -0.5% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_dense_owned` | 1,506 → 1,524 | 687 → 695 | -1.2% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_dense_cold` | 1,742 → 1,715 | 797 → 783 | +1.8% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_dense_warm` | 1,339 → 1,286 | 611 → 587 | +4.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/33_filtered_values` | 396 → 377 | 182 → 175 | +4.0% | 1 → 1 | 0 → 0 | 128 → 128 |
| `bpm_summary/33_filtered_map` | 386 → 350 | 176 → 159 | +10.7% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_filtered_owned` | 437 → 454 | 200 → 207 | -3.4% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_filtered_cold` | 678 → 635 | 310 → 290 | +6.9% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_filtered_warm` | 275 → 275 | 126 → 125 | +0.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/33_fallback_values` | 1,140 → 1,173 | 520 → 535 | -2.8% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_fallback_map` | 1,184 → 1,193 | 540 → 544 | -0.7% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_fallback_owned` | 1,412 → 1,519 | 644 → 693 | -7.1% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_fallback_cold` | 1,657 → 1,662 | 757 → 759 | -0.3% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_fallback_warm` | 1,310 → 1,269 | 598 → 579 | +3.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/33_special_values` | 394 → 388 | 180 → 177 | +1.7% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_special_map` | 437 → 450 | 199 → 205 | -2.9% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_special_owned` | 691 → 725 | 315 → 331 | -4.8% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_special_cold` | 908 → 914 | 414 → 418 | -1.0% | 1 → 1 | 0 → 0 | 264 → 264 |
| `bpm_summary/33_special_warm` | 544 → 566 | 249 → 258 | -3.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/128_uniform_values` | 1,829 → 1,799 | 834 → 823 | +1.3% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_uniform_map` | 1,743 → 1,715 | 795 → 782 | +1.7% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_uniform_owned` | 2,155 → 2,233 | 983 → 1,020 | -3.6% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_uniform_cold` | 3,105 → 3,137 | 1,420 → 1,431 | -0.8% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_uniform_warm` | 2,018 → 2,065 | 922 → 942 | -2.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/128_reverse_values` | 2,122 → 2,181 | 976 → 995 | -1.9% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_reverse_map` | 1,964 → 2,128 | 899 → 973 | -7.6% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_reverse_owned` | 2,510 → 2,466 | 1,147 → 1,127 | +1.8% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_reverse_cold` | 3,204 → 3,266 | 1,464 → 1,492 | -1.9% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_reverse_warm` | 2,298 → 2,377 | 1,048 → 1,085 | -3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/128_dense_values` | 1,878 → 1,875 | 856 → 856 | +0.0% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_dense_map` | 1,745 → 1,791 | 796 → 817 | -2.6% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_dense_owned` | 2,240 → 2,302 | 1,023 → 1,051 | -2.7% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_dense_cold` | 3,139 → 3,028 | 1,435 → 1,383 | +3.8% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_dense_warm` | 2,061 → 2,107 | 941 → 965 | -2.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/128_filtered_values` | 1,636 → 1,537 | 749 → 701 | +6.8% | 1 → 1 | 0 → 0 | 512 → 512 |
| `bpm_summary/128_filtered_map` | 1,018 → 1,408 | 465 → 642 | -27.6% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_filtered_owned` | 1,187 → 1,252 | 544 → 571 | -4.7% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_filtered_cold` | 1,955 → 1,984 | 893 → 908 | -1.7% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_filtered_warm` | 992 → 1,042 | 453 → 476 | -4.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/128_fallback_values` | 3,874 → 3,845 | 1,767 → 1,756 | +0.6% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_fallback_map` | 4,031 → 4,021 | 1,846 → 1,838 | +0.4% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_fallback_owned` | 5,074 → 5,119 | 2,316 → 2,341 | -1.1% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_fallback_cold` | 5,993 → 5,939 | 2,735 → 2,710 | +0.9% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_fallback_warm` | 4,972 → 4,927 | 2,270 → 2,248 | +1.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/128_special_values` | 905 → 908 | 413 → 415 | -0.5% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_special_map` | 979 → 1,054 | 447 → 481 | -7.1% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_special_owned` | 2,054 → 2,110 | 936 → 962 | -2.7% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_special_cold` | 2,988 → 2,971 | 1,365 → 1,368 | -0.2% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `bpm_summary/128_special_warm` | 1,943 → 1,908 | 886 → 870 | +1.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/4096_uniform_values` | 68,693 → 74,771 | 31,358 → 34,133 | -8.1% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_uniform_map` | 48,357 → 51,781 | 22,072 → 23,657 | -6.7% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_uniform_owned` | 61,538 → 61,527 | 28,088 → 28,080 | +0.0% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_uniform_cold` | 98,041 → 98,047 | 44,753 → 44,763 | -0.0% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_uniform_warm` | 59,261 → 60,191 | 27,056 → 27,466 | -1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/4096_reverse_values` | 58,587 → 59,374 | 26,733 → 27,095 | -1.3% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_reverse_map` | 51,252 → 50,903 | 23,397 → 23,236 | +0.7% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_reverse_owned` | 63,367 → 63,704 | 28,920 → 29,088 | -0.6% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_reverse_cold` | 101,286 → 99,463 | 46,242 → 45,408 | +1.8% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_reverse_warm` | 63,321 → 61,620 | 28,894 → 28,112 | +2.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/4096_dense_values` | 56,797 → 55,445 | 25,922 → 25,299 | +2.5% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_dense_map` | 49,330 → 47,056 | 22,517 → 21,487 | +4.8% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_dense_owned` | 62,731 → 62,512 | 28,627 → 28,536 | +0.3% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_dense_cold` | 94,169 → 92,382 | 42,987 → 42,189 | +1.9% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_dense_warm` | 61,868 → 62,130 | 28,221 → 28,344 | -0.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/4096_filtered_values` | 33,473 → 34,295 | 15,294 → 15,649 | -2.3% | 1 → 1 | 0 → 0 | 16,384 → 16,384 |
| `bpm_summary/4096_filtered_map` | 28,333 → 33,655 | 12,934 → 15,363 | -15.8% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_filtered_owned` | 32,119 → 31,960 | 14,651 → 14,590 | +0.4% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_filtered_cold` | 51,888 → 51,487 | 23,689 → 23,529 | +0.7% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_filtered_warm` | 31,068 → 30,611 | 14,179 → 13,972 | +1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/4096_fallback_values` | 88,479 → 88,677 | 40,397 → 40,624 | -0.6% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_fallback_map` | 88,800 → 85,162 | 40,517 → 38,859 | +4.3% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_fallback_owned` | 121,791 → 120,523 | 55,569 → 55,017 | +1.0% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_fallback_cold` | 157,107 → 158,115 | 71,737 → 72,172 | -0.6% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_fallback_warm` | 124,933 → 122,468 | 57,050 → 55,889 | +2.1% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `bpm_summary/4096_special_values` | 27,904 → 26,202 | 12,729 → 11,973 | +6.3% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_special_map` | 26,865 → 26,152 | 12,263 → 11,942 | +2.7% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_special_owned` | 61,351 → 62,055 | 28,011 → 28,318 | -1.1% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_special_cold` | 91,252 → 89,912 | 41,662 → 41,056 | +1.5% | 2 → 2 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/4096_special_warm` | 61,587 → 58,648 | 28,110 → 26,778 | +5.0% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |

### NPS summary paths and controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `nps_summary/0_uniform_owned` | 17 → 3 | 8 → 2 | +300.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_uniform_cold` | 9 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_uniform_warm` | 5 → 5 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_uniform_in_place` | 3 → 3 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_dense_owned` | 16 → 3 | 7 → 2 | +250.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_dense_cold` | 9 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_dense_warm` | 5 → 5 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_dense_in_place` | 3 → 3 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_sparse_owned` | 16 → 3 | 8 → 2 | +300.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_sparse_cold` | 9 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_sparse_warm` | 5 → 5 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_sparse_in_place` | 3 → 3 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_negative_owned` | 17 → 3 | 8 → 2 | +300.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_negative_cold` | 10 → 9 | 5 → 4 | +25.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_negative_warm` | 5 → 5 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_negative_in_place` | 3 → 3 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_special_owned` | 17 → 3 | 8 → 2 | +300.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_special_cold` | 9 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_special_warm` | 5 → 5 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/0_special_in_place` | 3 → 3 | 2 → 2 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_uniform_owned` | 18 → 4 | 8 → 2 | +300.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_uniform_cold` | 9 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_uniform_warm` | 8 → 8 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_uniform_in_place` | 19 → 19 | 11 → 9 | +22.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_dense_owned` | 15 → 3 | 7 → 2 | +250.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_dense_cold` | 8 → 8 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_dense_warm` | 7 → 7 | 3 → 3 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_dense_in_place` | 18 → 19 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_sparse_owned` | 18 → 4 | 8 → 2 | +300.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_sparse_cold` | 9 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_sparse_warm` | 8 → 7 | 4 → 3 | +33.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_sparse_in_place` | 18 → 20 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_negative_owned` | 16 → 4 | 7 → 2 | +250.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_negative_cold` | 9 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_negative_warm` | 7 → 8 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_negative_in_place` | 18 → 17 | 9 → 8 | +12.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_special_owned` | 16 → 3 | 7 → 2 | +250.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_special_cold` | 8 → 9 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_special_warm` | 7 → 8 | 4 → 4 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_special_in_place` | 18 → 20 | 9 → 9 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_uniform_owned` | 26 → 8 | 12 → 4 | +200.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_uniform_cold` | 14 → 12 | 7 → 6 | +16.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_uniform_warm` | 14 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_uniform_in_place` | 33 → 36 | 15 → 17 | -11.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_dense_owned` | 21 → 7 | 10 → 3 | +233.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_dense_cold` | 14 → 12 | 7 → 6 | +16.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_dense_warm` | 13 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_dense_in_place` | 33 → 36 | 15 → 17 | -11.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_sparse_owned` | 20 → 8 | 9 → 4 | +125.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_sparse_cold` | 14 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_sparse_warm` | 13 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_sparse_in_place` | 32 → 36 | 15 → 17 | -11.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_negative_owned` | 20 → 7 | 9 → 3 | +200.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_negative_cold` | 14 → 12 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_negative_warm` | 14 → 14 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_negative_in_place` | 31 → 35 | 14 → 16 | -12.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_special_owned` | 21 → 6 | 10 → 3 | +233.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_special_cold` | 13 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_special_warm` | 12 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_special_in_place` | 33 → 35 | 15 → 16 | -6.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_uniform_owned` | 83 → 91 | 38 → 42 | -9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_uniform_cold` | 280 → 261 | 128 → 119 | +7.6% | 1 → 1 | 0 → 0 | 64 → 64 |
| `nps_summary/8_uniform_warm` | 60 → 60 | 28 → 28 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_uniform_in_place` | 58 → 61 | 27 → 28 | -3.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_dense_owned` | 122 → 129 | 56 → 59 | -5.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_dense_cold` | 286 → 323 | 131 → 149 | -12.1% | 1 → 1 | 0 → 0 | 64 → 64 |
| `nps_summary/8_dense_warm` | 103 → 101 | 47 → 46 | +2.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_dense_in_place` | 96 → 97 | 44 → 45 | -2.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_sparse_owned` | 86 → 91 | 40 → 41 | -2.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_sparse_cold` | 273 → 281 | 125 → 129 | -3.1% | 1 → 1 | 0 → 0 | 64 → 64 |
| `nps_summary/8_sparse_warm` | 60 → 62 | 28 → 29 | -3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_sparse_in_place` | 58 → 61 | 27 → 28 | -3.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_negative_owned` | 141 → 145 | 64 → 67 | -4.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_negative_cold` | 355 → 339 | 162 → 155 | +4.5% | 1 → 1 | 0 → 0 | 64 → 64 |
| `nps_summary/8_negative_warm` | 137 → 121 | 63 → 55 | +14.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_negative_in_place` | 115 → 115 | 53 → 53 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_special_owned` | 103 → 107 | 47 → 49 | -4.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_special_cold` | 295 → 316 | 135 → 144 | -6.2% | 1 → 1 | 0 → 0 | 64 → 64 |
| `nps_summary/8_special_warm` | 128 → 101 | 59 → 48 | +22.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_special_in_place` | 76 → 77 | 35 → 36 | -2.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_uniform_owned` | 356 → 335 | 164 → 153 | +7.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_uniform_cold` | 741 → 843 | 339 → 385 | -11.9% | 1 → 1 | 0 → 0 | 256 → 256 |
| `nps_summary/32_uniform_warm` | 301 → 302 | 137 → 139 | -1.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_uniform_in_place` | 304 → 328 | 139 → 150 | -7.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_dense_owned` | 426 → 468 | 195 → 213 | -8.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_dense_cold` | 804 → 837 | 369 → 382 | -3.4% | 1 → 1 | 0 → 0 | 256 → 256 |
| `nps_summary/32_dense_warm` | 407 → 419 | 186 → 191 | -2.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_dense_in_place` | 408 → 411 | 186 → 188 | -1.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_sparse_owned` | 324 → 333 | 148 → 153 | -3.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_sparse_cold` | 727 → 814 | 332 → 372 | -10.8% | 1 → 1 | 0 → 0 | 256 → 256 |
| `nps_summary/32_sparse_warm` | 303 → 304 | 138 → 139 | -0.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_sparse_in_place` | 308 → 314 | 140 → 144 | -2.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_negative_owned` | 340 → 353 | 155 → 161 | -3.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_negative_cold` | 734 → 797 | 336 → 364 | -7.7% | 1 → 1 | 0 → 0 | 256 → 256 |
| `nps_summary/32_negative_warm` | 317 → 331 | 145 → 151 | -4.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_negative_in_place` | 321 → 339 | 147 → 155 | -5.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_special_owned` | 322 → 346 | 147 → 158 | -7.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_special_cold` | 714 → 799 | 326 → 365 | -10.7% | 1 → 1 | 0 → 0 | 256 → 256 |
| `nps_summary/32_special_warm` | 312 → 323 | 143 → 147 | -2.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_special_in_place` | 300 → 349 | 137 → 160 | -14.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_uniform_owned` | 249 → 211 | 114 → 96 | +18.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_uniform_cold` | 203 → 202 | 93 → 92 | +1.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_uniform_warm` | 202 → 226 | 92 → 103 | -10.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_uniform_in_place` | 221 → 224 | 102 → 103 | -1.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_dense_owned` | 830 → 790 | 380 → 361 | +5.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_dense_cold` | 1,288 → 1,330 | 588 → 607 | -3.1% | 1 → 1 | 0 → 0 | 512 → 512 |
| `nps_summary/64_dense_warm` | 794 → 790 | 362 → 360 | +0.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_dense_in_place` | 758 → 769 | 347 → 351 | -1.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_sparse_owned` | 212 → 211 | 97 → 97 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_sparse_cold` | 202 → 213 | 93 → 97 | -4.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_sparse_warm` | 205 → 212 | 94 → 98 | -4.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_sparse_in_place` | 225 → 240 | 103 → 110 | -6.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_negative_owned` | 709 → 736 | 324 → 336 | -3.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_negative_cold` | 1,207 → 1,288 | 551 → 588 | -6.3% | 1 → 1 | 0 → 0 | 512 → 512 |
| `nps_summary/64_negative_warm` | 729 → 718 | 333 → 328 | +1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_negative_in_place` | 679 → 675 | 310 → 308 | +0.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_special_owned` | 691 → 717 | 316 → 327 | -3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_special_cold` | 1,244 → 1,222 | 569 → 558 | +2.0% | 1 → 1 | 0 → 0 | 512 → 512 |
| `nps_summary/64_special_warm` | 693 → 749 | 316 → 341 | -7.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_special_in_place` | 681 → 722 | 311 → 330 | -5.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_uniform_owned` | 237 → 222 | 109 → 101 | +7.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_uniform_cold` | 233 → 210 | 107 → 97 | +10.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_uniform_warm` | 221 → 214 | 101 → 98 | +3.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_uniform_in_place` | 230 → 233 | 106 → 106 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_dense_owned` | 955 → 802 | 437 → 365 | +19.7% | 1 → 0 | 0 → 0 | 520 → 0 |
| `nps_summary/65_dense_cold` | 1,335 → 1,290 | 610 → 588 | +3.7% | 1 → 1 | 0 → 0 | 520 → 520 |
| `nps_summary/65_dense_warm` | 773 → 768 | 353 → 350 | +0.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_dense_in_place` | 750 → 724 | 343 → 331 | +3.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_sparse_owned` | 265 → 236 | 121 → 108 | +12.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_sparse_cold` | 248 → 216 | 114 → 99 | +15.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_sparse_warm` | 213 → 215 | 98 → 98 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_sparse_in_place` | 233 → 234 | 107 → 107 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_negative_owned` | 897 → 712 | 409 → 325 | +25.8% | 1 → 0 | 0 → 0 | 520 → 0 |
| `nps_summary/65_negative_cold` | 1,184 → 1,196 | 541 → 546 | -0.9% | 1 → 1 | 0 → 0 | 520 → 520 |
| `nps_summary/65_negative_warm` | 673 → 697 | 307 → 318 | -3.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_negative_in_place` | 676 → 680 | 309 → 311 | -0.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_special_owned` | 863 → 724 | 394 → 330 | +19.4% | 1 → 0 | 0 → 0 | 520 → 0 |
| `nps_summary/65_special_cold` | 1,154 → 1,229 | 527 → 561 | -6.1% | 1 → 1 | 0 → 0 | 520 → 520 |
| `nps_summary/65_special_warm` | 663 → 753 | 304 → 344 | -11.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_special_in_place` | 685 → 710 | 313 → 324 | -3.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_uniform_owned` | 426 → 418 | 195 → 191 | +2.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_uniform_cold` | 397 → 399 | 182 → 183 | -0.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_uniform_warm` | 399 → 403 | 183 → 185 | -1.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_uniform_in_place` | 441 → 457 | 203 → 210 | -3.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_dense_owned` | 1,475 → 1,244 | 677 → 571 | +18.6% | 1 → 0 | 0 → 0 | 1,024 → 0 |
| `nps_summary/128_dense_cold` | 2,244 → 2,314 | 1,028 → 1,065 | -3.5% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `nps_summary/128_dense_warm` | 1,304 → 1,340 | 598 → 620 | -3.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_dense_in_place` | 1,214 → 1,281 | 558 → 589 | -5.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_sparse_owned` | 422 → 420 | 193 → 192 | +0.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_sparse_cold` | 398 → 399 | 182 → 183 | -0.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_sparse_warm` | 399 → 404 | 183 → 185 | -1.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_sparse_in_place` | 424 → 489 | 195 → 225 | -13.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_negative_owned` | 1,924 → 1,742 | 877 → 797 | +10.0% | 1 → 0 | 0 → 0 | 1,024 → 0 |
| `nps_summary/128_negative_cold` | 2,749 → 2,950 | 1,257 → 1,348 | -6.8% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `nps_summary/128_negative_warm` | 1,726 → 1,889 | 787 → 862 | -8.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_negative_in_place` | 1,653 → 1,748 | 759 → 801 | -5.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_special_owned` | 1,524 → 1,400 | 695 → 641 | +8.4% | 1 → 0 | 0 → 0 | 1,024 → 0 |
| `nps_summary/128_special_cold` | 2,508 → 2,674 | 1,144 → 1,220 | -6.2% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `nps_summary/128_special_warm` | 1,294 → 1,319 | 590 → 603 | -2.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_special_in_place` | 1,280 → 1,308 | 588 → 602 | -2.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_uniform_owned` | 427 → 437 | 195 → 200 | -2.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_uniform_cold` | 404 → 404 | 185 → 185 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_uniform_warm` | 402 → 408 | 184 → 187 | -1.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_uniform_in_place` | 426 → 430 | 196 → 198 | -1.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_dense_owned` | 1,355 → 1,365 | 618 → 623 | -0.8% | 1 → 1 | 0 → 0 | 1,032 → 1,032 |
| `nps_summary/129_dense_cold` | 2,237 → 2,233 | 1,023 → 1,021 | +0.2% | 1 → 1 | 0 → 0 | 1,032 → 1,032 |
| `nps_summary/129_dense_warm` | 1,144 → 1,128 | 522 → 515 | +1.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_dense_in_place` | 1,107 → 1,120 | 506 → 516 | -1.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_sparse_owned` | 429 → 439 | 196 → 201 | -2.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_sparse_cold` | 406 → 407 | 186 → 186 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_sparse_warm` | 406 → 409 | 186 → 187 | -0.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_sparse_in_place` | 427 → 433 | 196 → 199 | -1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_negative_owned` | 1,505 → 1,475 | 687 → 675 | +1.8% | 1 → 1 | 0 → 0 | 1,032 → 1,032 |
| `nps_summary/129_negative_cold` | 2,249 → 2,438 | 1,030 → 1,115 | -7.6% | 1 → 1 | 0 → 0 | 1,032 → 1,032 |
| `nps_summary/129_negative_warm` | 1,291 → 1,320 | 591 → 605 | -2.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_negative_in_place` | 1,254 → 1,278 | 573 → 584 | -1.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_special_owned` | 1,421 → 1,380 | 650 → 630 | +3.2% | 1 → 1 | 0 → 0 | 1,032 → 1,032 |
| `nps_summary/129_special_cold` | 2,119 → 2,338 | 968 → 1,070 | -9.5% | 1 → 1 | 0 → 0 | 1,032 → 1,032 |
| `nps_summary/129_special_warm` | 1,185 → 1,240 | 541 → 569 | -4.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/129_special_in_place` | 1,157 → 1,227 | 529 → 564 | -6.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_uniform_owned` | 12,574 → 12,303 | 5,744 → 5,617 | +2.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_uniform_cold` | 12,441 → 12,540 | 5,682 → 5,720 | -0.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_uniform_warm` | 12,420 → 12,255 | 5,700 → 5,599 | +1.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_uniform_in_place` | 15,025 → 15,439 | 6,859 → 7,056 | -2.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_dense_owned` | 46,915 → 49,382 | 21,424 → 22,578 | -5.1% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `nps_summary/4096_dense_cold` | 90,915 → 90,303 | 41,509 → 41,364 | +0.4% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `nps_summary/4096_dense_warm` | 43,445 → 43,455 | 19,833 → 19,841 | -0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_dense_in_place` | 41,844 → 42,330 | 19,105 → 19,327 | -1.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_sparse_owned` | 12,355 → 12,491 | 5,636 → 5,705 | -1.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_sparse_cold` | 12,193 → 12,440 | 5,572 → 5,675 | -1.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_sparse_warm` | 12,570 → 12,576 | 5,733 → 5,749 | -0.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_sparse_in_place` | 15,058 → 15,596 | 6,918 → 7,122 | -2.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_negative_owned` | 40,019 → 40,538 | 18,267 → 18,521 | -1.4% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `nps_summary/4096_negative_cold` | 75,692 → 80,276 | 34,558 → 36,664 | -5.7% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `nps_summary/4096_negative_warm` | 36,825 → 37,695 | 16,805 → 17,207 | -2.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_negative_in_place` | 38,328 → 38,286 | 17,518 → 17,481 | +0.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_special_owned` | 40,492 → 39,427 | 18,544 → 17,994 | +3.1% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `nps_summary/4096_special_cold` | 76,705 → 78,200 | 35,052 → 35,708 | -1.8% | 1 → 1 | 0 → 0 | 32,768 → 32,768 |
| `nps_summary/4096_special_warm` | 39,861 → 40,511 | 18,210 → 18,491 | -1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/4096_special_in_place` | 38,164 → 39,413 | 17,424 → 18,008 | -3.2% | 0 → 0 | 0 → 0 | 0 → 0 |

### Owned timing packing

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `pack_timing/0_0_false` | 57 → 51 | 26 → 24 | +8.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_0_true` | 50 → 51 | 23 → 24 | -4.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_1_false` | 49 → 45 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_1_true` | 57 → 45 | 26 → 21 | +23.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_2_false` | 50 → 46 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_2_true` | 53 → 49 | 24 → 23 | +4.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_4_false` | 50 → 53 | 24 → 25 | -4.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_4_true` | 50 → 51 | 23 → 24 | -4.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_8_false` | 57 → 44 | 27 → 21 | +28.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_8_true` | 50 → 50 | 23 → 23 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_3_false` | 50 → 46 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_3_true` | 50 → 45 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_6_false` | 52 → 46 | 24 → 21 | +14.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_6_true` | 50 → 51 | 23 → 24 | -4.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_15_false` | 50 → 49 | 23 → 23 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/0_15_true` | 50 → 46 | 24 → 21 | +14.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/1_0_false` | 48 → 43 | 22 → 20 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/1_0_true` | 48 → 43 | 22 → 20 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/1_1_false` | 128 → 122 | 59 → 56 | +5.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/1_1_true` | 144 → 138 | 66 → 63 | +4.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/1_2_false` | 273 → 124 | 125 → 57 | +119.3% | 1 → 0 | 0 → 0 | 16 → 0 |
| `pack_timing/1_2_true` | 319 → 302 | 146 → 138 | +5.8% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_timing/1_4_false` | 275 → 137 | 126 → 63 | +100.0% | 1 → 0 | 0 → 0 | 16 → 0 |
| `pack_timing/1_4_true` | 330 → 302 | 151 → 138 | +9.4% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_timing/1_8_false` | 279 → 123 | 128 → 57 | +124.6% | 1 → 0 | 0 → 0 | 16 → 0 |
| `pack_timing/1_8_true` | 299 → 300 | 137 → 137 | +0.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_timing/1_3_false` | 396 → 414 | 181 → 189 | -4.2% | 0 → 0 | 1 → 1 | 32 → 32 |
| `pack_timing/1_3_true` | 224 → 232 | 103 → 106 | -2.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/1_6_false` | 346 → 350 | 158 → 160 | -1.2% | 1 → 1 | 0 → 0 | 32 → 32 |
| `pack_timing/1_6_true` | 382 → 379 | 175 → 173 | +1.2% | 1 → 1 | 0 → 0 | 32 → 32 |
| `pack_timing/1_15_false` | 546 → 583 | 250 → 266 | -6.0% | 0 → 0 | 1 → 1 | 64 → 64 |
| `pack_timing/1_15_true` | 595 → 618 | 272 → 282 | -3.5% | 0 → 0 | 1 → 1 | 64 → 64 |
| `pack_timing/32_0_false` | 48 → 43 | 22 → 20 | +10.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/32_0_true` | 48 → 49 | 22 → 23 | -4.3% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/32_1_false` | 342 → 332 | 156 → 152 | +2.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/32_1_true` | 555 → 546 | 255 → 249 | +2.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/32_2_false` | 511 → 333 | 234 → 152 | +53.9% | 1 → 0 | 0 → 0 | 512 → 0 |
| `pack_timing/32_2_true` | 740 → 733 | 338 → 339 | -0.3% | 1 → 1 | 0 → 0 | 512 → 512 |
| `pack_timing/32_4_false` | 518 → 323 | 237 → 148 | +60.1% | 1 → 0 | 0 → 0 | 512 → 0 |
| `pack_timing/32_4_true` | 771 → 757 | 354 → 348 | +1.7% | 1 → 1 | 0 → 0 | 512 → 512 |
| `pack_timing/32_8_false` | 536 → 330 | 247 → 151 | +63.6% | 1 → 0 | 0 → 0 | 512 → 0 |
| `pack_timing/32_8_true` | 772 → 702 | 353 → 320 | +10.3% | 1 → 1 | 0 → 0 | 512 → 512 |
| `pack_timing/32_3_false` | 916 → 884 | 418 → 405 | +3.2% | 0 → 0 | 1 → 1 | 1,024 → 1,024 |
| `pack_timing/32_3_true` | 1,262 → 1,133 | 581 → 517 | +12.4% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/32_6_false` | 854 → 812 | 390 → 371 | +5.1% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `pack_timing/32_6_true` | 1,439 → 1,228 | 657 → 560 | +17.3% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `pack_timing/32_15_false` | 1,683 → 1,662 | 783 → 759 | +3.2% | 0 → 0 | 1 → 1 | 2,048 → 2,048 |
| `pack_timing/32_15_true` | 3,130 → 3,201 | 1,428 → 1,463 | -2.4% | 0 → 0 | 1 → 1 | 2,048 → 2,048 |
| `pack_timing/4096_0_false` | 48 → 44 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/4096_0_true` | 48 → 44 | 23 → 21 | +9.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/4096_1_false` | 25,137 → 25,405 | 11,476 → 11,616 | -1.2% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/4096_1_true` | 41,287 → 41,640 | 18,940 → 19,029 | -0.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/4096_2_false` | 59,911 → 27,958 | 27,422 → 12,854 | +113.3% | 1 → 0 | 0 → 0 | 65,536 → 0 |
| `pack_timing/4096_2_true` | 43,739 → 68,937 | 19,974 → 31,638 | -36.9% | 1 → 1 | 0 → 0 | 65,536 → 65,536 |
| `pack_timing/4096_4_false` | 24,910 → 1,882 | 11,384 → 861 | +1222.2% | 1 → 0 | 0 → 0 | 65,536 → 0 |
| `pack_timing/4096_4_true` | 41,326 → 39,255 | 18,950 → 18,005 | +5.2% | 1 → 1 | 0 → 0 | 65,536 → 65,536 |
| `pack_timing/4096_8_false` | 24,056 → 1,745 | 10,996 → 803 | +1269.4% | 1 → 0 | 0 → 0 | 65,536 → 0 |
| `pack_timing/4096_8_true` | 42,833 → 39,638 | 19,559 → 18,177 | +7.6% | 1 → 1 | 0 → 0 | 65,536 → 65,536 |
| `pack_timing/4096_3_false` | 80,692 → 70,174 | 40,138 → 32,114 | +25.0% | 0 → 0 | 1 → 1 | 131,072 → 131,072 |
| `pack_timing/4096_3_true` | 113,502 → 117,435 | 51,830 → 53,688 | -3.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/4096_6_false` | 61,281 → 65,790 | 27,982 → 30,061 | -6.9% | 1 → 1 | 0 → 0 | 131,072 → 131,072 |
| `pack_timing/4096_6_true` | 109,820 → 111,422 | 50,226 → 51,010 | -1.5% | 1 → 1 | 0 → 0 | 131,072 → 131,072 |
| `pack_timing/4096_15_false` | 164,188 → 158,117 | 74,979 → 72,239 | +3.8% | 0 → 0 | 1 → 1 | 262,144 → 262,144 |
| `pack_timing/4096_15_true` | 288,205 → 299,449 | 131,822 → 136,776 | -3.6% | 0 → 0 | 1 → 1 | 262,144 → 262,144 |

### Raw timing builder

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `pack_raw/0_0` | 876 → 884 | 400 → 404 | -1.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/0_1` | 867 → 905 | 396 → 415 | -4.6% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/0_2` | 865 → 877 | 395 → 400 | -1.2% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/0_4` | 879 → 878 | 401 → 400 | +0.2% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/0_8` | 864 → 870 | 394 → 397 | -0.8% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/0_3` | 900 → 865 | 411 → 395 | +4.1% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/0_6` | 889 → 921 | 405 → 421 | -3.8% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/0_15` | 870 → 871 | 397 → 397 | +0.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/1_0` | 876 → 893 | 400 → 408 | -2.0% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/1_1` | 1,459 → 1,478 | 666 → 675 | -1.3% | 3 → 3 | 0 → 0 | 36 → 36 |
| `pack_raw/1_2` | 1,664 → 1,465 | 760 → 668 | +13.8% | 4 → 3 | 0 → 0 | 52 → 36 |
| `pack_raw/1_4` | 1,589 → 1,359 | 726 → 621 | +16.9% | 4 → 3 | 0 → 0 | 52 → 36 |
| `pack_raw/1_8` | 1,610 → 1,432 | 734 → 653 | +12.4% | 4 → 3 | 0 → 0 | 52 → 36 |
| `pack_raw/1_3` | 2,088 → 2,157 | 952 → 984 | -3.3% | 4 → 4 | 1 → 1 | 88 → 88 |
| `pack_raw/1_6` | 1,995 → 2,072 | 910 → 947 | -3.9% | 5 → 5 | 0 → 0 | 88 → 88 |
| `pack_raw/1_15` | 2,996 → 2,911 | 1,367 → 1,329 | +2.9% | 6 → 6 | 1 → 1 | 160 → 160 |
| `pack_raw/32_0` | 889 → 841 | 408 → 386 | +5.7% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/32_1` | 8,464 → 8,449 | 3,865 → 3,857 | +0.2% | 3 → 3 | 0 → 0 | 656 → 656 |
| `pack_raw/32_2` | 8,442 → 7,910 | 3,855 → 3,611 | +6.8% | 4 → 3 | 0 → 0 | 1,168 → 656 |
| `pack_raw/32_4` | 8,522 → 7,799 | 3,892 → 3,558 | +9.4% | 4 → 3 | 0 → 0 | 1,168 → 656 |
| `pack_raw/32_8` | 8,218 → 8,216 | 3,750 → 3,749 | +0.0% | 4 → 3 | 0 → 0 | 1,168 → 656 |
| `pack_raw/32_3` | 16,225 → 15,985 | 7,407 → 7,298 | +1.5% | 4 → 4 | 1 → 1 | 2,320 → 2,320 |
| `pack_raw/32_6` | 15,803 → 14,628 | 7,214 → 6,675 | +8.1% | 5 → 5 | 0 → 0 | 2,320 → 2,320 |
| `pack_raw/32_15` | 30,486 → 30,464 | 13,912 → 13,903 | +0.1% | 6 → 6 | 1 → 1 | 4,624 → 4,624 |
| `pack_raw/4096_0` | 766 → 786 | 356 → 365 | -2.5% | 1 → 1 | 0 → 0 | 16 → 16 |
| `pack_raw/4096_1` | 823,735 → 823,518 | 376,065 → 375,845 | +0.1% | 3 → 3 | 0 → 0 | 81,936 → 81,936 |
| `pack_raw/4096_2` | 1,033,347 → 834,600 | 471,613 → 381,044 | +23.8% | 4 → 3 | 0 → 0 | 147,472 → 81,936 |
| `pack_raw/4096_4` | 962,402 → 836,016 | 439,267 → 381,620 | +15.1% | 4 → 3 | 0 → 0 | 147,472 → 81,936 |
| `pack_raw/4096_8` | 877,939 → 849,371 | 400,730 → 387,655 | +3.4% | 4 → 3 | 0 → 0 | 147,472 → 81,936 |
| `pack_raw/4096_3` | 1,770,320 → 2,054,630 | 808,256 → 939,244 | -13.9% | 4 → 4 | 1 → 1 | 294,928 → 294,928 |
| `pack_raw/4096_6` | 1,654,213 → 2,002,132 | 755,003 → 914,301 | -17.4% | 5 → 5 | 0 → 0 | 294,928 → 294,928 |
| `pack_raw/4096_15` | 3,236,258 → 3,738,634 | 1,477,256 → 1,706,819 | -13.4% | 6 → 6 | 1 → 1 | 589,840 → 589,840 |

### Standard stream controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `standard/uniform_detailed` | 9,536 → 9,411 | 4,350 → 4,302 | +1.1% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `standard/uniform_partial` | 6,782 → 7,168 | 3,096 → 3,269 | -5.3% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `standard/uniform_simple` | 6,178 → 6,958 | 2,821 → 3,177 | -11.2% | 2 → 2 | 0 → 0 | 24,582 → 24,582 |
| `standard/uniform_total` | 5,168 → 5,896 | 2,357 → 2,698 | -12.6% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/uniform_three` | 4,997 → 5,230 | 2,279 → 2,384 | -4.4% | 3 → 3 | 0 → 0 | 16 → 16 |
| `standard/fragmented_detailed` | 38,510 → 39,782 | 17,575 → 18,176 | -3.3% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `standard/fragmented_partial` | 35,744 → 40,082 | 16,305 → 18,313 | -11.0% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `standard/fragmented_simple` | 36,081 → 42,250 | 16,463 → 19,280 | -14.6% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `standard/fragmented_total` | 5,439 → 5,955 | 2,480 → 2,716 | -8.7% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/fragmented_three` | 71,156 → 70,479 | 32,468 → 32,186 | +0.9% | 3 → 3 | 0 → 0 | 29,484 → 29,484 |
| `standard/empty_detailed` | 4,763 → 4,706 | 2,174 → 2,147 | +1.3% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_partial` | 5,008 → 4,833 | 2,292 → 2,205 | +3.9% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_simple` | 4,867 → 5,210 | 2,220 → 2,394 | -7.3% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_total` | 5,504 → 5,751 | 2,510 → 2,626 | -4.4% | 1 → 1 | 0 → 0 | 11 → 11 |
| `standard/empty_three` | 5,205 → 5,121 | 2,374 → 2,336 | +1.6% | 3 → 3 | 0 → 0 | 33 → 33 |
| `standard/short_detailed` | 544 → 636 | 249 → 290 | -14.1% | 2 → 2 | 0 → 0 | 492 → 492 |
| `standard/short_partial` | 580 → 568 | 265 → 260 | +1.9% | 2 → 2 | 0 → 0 | 492 → 492 |
| `standard/short_simple` | 528 → 579 | 241 → 264 | -8.7% | 2 → 2 | 0 → 0 | 492 → 492 |
| `standard/short_total` | 182 → 180 | 85 → 83 | +2.4% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/short_three` | 980 → 1,095 | 450 → 500 | -10.0% | 3 → 3 | 0 → 0 | 252 → 252 |
| `standard/leading_detailed` | 4,821 → 5,651 | 2,198 → 2,577 | -14.7% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `standard/leading_partial` | 5,470 → 5,754 | 2,497 → 2,630 | -5.1% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `standard/leading_simple` | 5,778 → 5,356 | 2,638 → 2,446 | +7.8% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `standard/leading_total` | 5,899 → 5,647 | 2,691 → 2,577 | +4.4% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/leading_three` | 5,437 → 5,420 | 2,481 → 2,479 | +0.1% | 3 → 3 | 0 → 0 | 12 → 12 |

### Stream and SN controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `streams/uniform_combined` | 23,269 → 22,358 | 10,620 → 10,196 | +4.2% | 6 → 6 | 0 → 0 | 33 → 33 |
| `streams/uniform_cold` | 23,128 → 23,633 | 10,546 → 10,782 | -2.2% | 7 → 7 | 0 → 0 | 16,417 → 16,417 |
| `streams/fragmented_combined` | 167,626 → 167,476 | 76,481 → 76,420 | +0.1% | 6 → 6 | 0 → 0 | 72,039 → 72,039 |
| `streams/fragmented_cold` | 172,696 → 166,289 | 78,816 → 75,882 | +3.9% | 7 → 7 | 2 → 2 | 186,727 → 186,727 |
| `streams/empty_combined` | 4,553 → 4,442 | 2,077 → 2,028 | +2.4% | 3 → 3 | 0 → 0 | 33 → 33 |
| `streams/empty_cold` | 4,847 → 4,561 | 2,210 → 2,083 | +6.1% | 3 → 3 | 0 → 0 | 33 → 33 |
| `streams/short_combined` | 3,114 → 2,903 | 1,421 → 1,323 | +7.4% | 6 → 6 | 0 → 0 | 891 → 891 |
| `streams/short_cold` | 3,704 → 3,556 | 1,690 → 1,623 | +4.1% | 7 → 7 | 0 → 0 | 1,403 → 1,403 |
| `streams/leading_combined` | 6,107 → 5,782 | 2,785 → 2,637 | +5.6% | 6 → 6 | 0 → 0 | 33 → 33 |
| `streams/leading_cold` | 6,216 → 6,419 | 2,854 → 2,953 | -3.4% | 7 → 7 | 0 → 0 | 1,569 → 1,569 |
| `sn/uniform_detailed` | 10,874 → 10,719 | 4,960 → 4,894 | +1.3% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/uniform_partial` | 10,474 → 11,255 | 4,778 → 5,135 | -7.0% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/uniform_simple` | 10,724 → 10,711 | 4,893 → 4,901 | -0.2% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/uniform_three` | 10,400 → 12,062 | 4,751 → 5,502 | -13.6% | 3 → 3 | 0 → 0 | 480 → 480 |
| `sn/fragmented_detailed` | 69,119 → 70,845 | 31,524 → 32,328 | -2.5% | 1 → 1 | 6 → 6 | 20,320 → 20,320 |
| `sn/fragmented_partial` | 60,869 → 62,692 | 27,772 → 28,627 | -3.0% | 1 → 1 | 5 → 5 | 10,080 → 10,080 |
| `sn/fragmented_simple` | 59,870 → 61,276 | 27,309 → 27,977 | -2.4% | 1 → 1 | 5 → 5 | 10,080 → 10,080 |
| `sn/fragmented_three` | 120,455 → 122,452 | 54,967 → 55,866 | -1.6% | 3 → 3 | 16 → 16 | 40,480 → 40,480 |
| `sn/empty_detailed` | 4,239 → 4,096 | 1,934 → 1,867 | +3.6% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/empty_partial` | 4,442 → 3,975 | 2,029 → 1,812 | +12.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/empty_simple` | 4,113 → 4,009 | 1,882 → 1,830 | +2.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/empty_three` | 4,107 → 4,521 | 1,881 → 2,062 | -8.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `sn/short_detailed` | 805 → 830 | 368 → 379 | -2.9% | 1 → 1 | 0 → 0 | 155 → 155 |
| `sn/short_partial` | 765 → 869 | 349 → 396 | -11.9% | 1 → 1 | 0 → 0 | 155 → 155 |
| `sn/short_simple` | 783 → 984 | 359 → 449 | -20.0% | 1 → 1 | 0 → 0 | 155 → 155 |
| `sn/short_three` | 2,048 → 1,795 | 934 → 820 | +13.9% | 3 → 3 | 0 → 0 | 465 → 465 |
| `sn/leading_detailed` | 5,324 → 4,698 | 2,434 → 2,153 | +13.1% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/leading_partial` | 4,856 → 4,303 | 2,215 → 1,963 | +12.8% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/leading_simple` | 4,455 → 4,880 | 2,032 → 2,247 | -9.6% | 1 → 1 | 0 → 0 | 160 → 160 |
| `sn/leading_three` | 5,035 → 5,347 | 2,296 → 2,440 | -5.9% | 3 → 3 | 0 → 0 | 480 → 480 |

### Hash, analysis, report and cleanup controls

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `hash_batch/1_global` | 22,981 → 23,007 | 10,484 → 10,506 | -0.2% | 28 → 28 | 2 → 2 | 5,283 → 5,283 |
| `hash_batch/1_repeat` | 23,386 → 23,391 | 10,669 → 10,697 | -0.3% | 29 → 29 | 4 → 4 | 5,318 → 5,318 |
| `hash_batch/1_vary` | 23,315 → 23,622 | 10,639 → 10,778 | -1.3% | 29 → 29 | 4 → 4 | 5,318 → 5,318 |
| `hash_batch/1_distinct` | 30,605 → 30,420 | 13,970 → 13,878 | +0.7% | 36 → 36 | 18 → 18 | 5,563 → 5,563 |
| `hash_batch/128_global` | 148,365 → 148,392 | 67,717 → 67,746 | -0.0% | 28 → 28 | 2 → 2 | 12,213 → 12,213 |
| `hash_batch/128_repeat` | 188,572 → 189,172 | 86,083 → 86,338 | -0.3% | 29 → 29 | 4 → 4 | 19,178 → 19,178 |
| `hash_batch/128_vary` | 188,822 → 189,503 | 86,173 → 86,486 | -0.4% | 29 → 29 | 4 → 4 | 19,178 → 19,178 |
| `hash_batch/128_distinct` | 473,193 → 473,755 | 216,024 → 216,201 | -0.1% | 36 → 36 | 18 → 18 | 67,933 → 67,933 |
| `analyze/fast_fake_lifts` | 659,529 → 643,128 | 301,114 → 293,605 | +2.6% | 31 → 31 | 4 → 4 | 59,028 → 59,028 |
| `analyze/camellia` | 453,408,375 → 436,460,122 | 207,023,900 → 199,244,180 | +3.9% | 110 → 110 | 0 → 0 | 5,263,624 → 5,263,624 |
| `analyze/fast_camellia` | 56,789,524 → 57,600,202 | 25,932,280 → 26,306,660 | -1.4% | 115 → 115 | 0 → 0 | 7,051,152 → 7,051,152 |
| `analyze/mixed_small` | 51,844 → 55,823 | 23,710 → 25,482 | -7.0% | 59 → 59 | 3 → 3 | 7,460 → 7,460 |
| `report/json/16_clean` | 91,376 → 87,236 | 41,712 → 40,056 | +4.1% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_early` | 92,087 → 86,731 | 42,038 → 39,580 | +6.2% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_late` | 89,435 → 93,742 | 40,878 → 42,786 | -4.5% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_dense` | 90,089 → 94,857 | 41,176 → 43,346 | -5.0% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_comma` | 88,871 → 95,467 | 40,548 → 43,554 | -6.9% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_comma_quote` | 87,622 → 88,518 | 40,035 → 40,390 | -0.9% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/16_custom` | 92,717 → 88,775 | 42,300 → 40,510 | +4.4% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_clean` | 134,738 → 135,324 | 61,714 → 61,738 | -0.0% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_early` | 152,329 → 160,103 | 69,552 → 73,132 | -4.9% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_late` | 193,926 → 199,034 | 88,566 → 90,920 | -2.6% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_dense` | 334,474 → 337,947 | 152,753 → 154,238 | -1.0% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_comma` | 131,500 → 133,779 | 59,995 → 61,035 | -1.7% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_comma_quote` | 195,834 → 200,480 | 89,452 → 91,578 | -2.3% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/4096_custom` | 132,995 → 136,553 | 60,795 → 62,384 | -2.5% | 18 → 18 | 0 → 0 | 318 → 318 |
| `report/json/camellia` | 21,730,467 → 22,072,972 | 9,921,194 → 10,077,685 | -1.6% | 30 → 30 | 0 → 0 | 850 → 850 |
| `cleanup/pair_1_clean` | 663 → 683 | 309 → 323 | -4.3% | 1 → 1 | 1 → 1 | 27 → 27 |
| `cleanup/speed_1_clean` | 889 → 911 | 413 → 423 | -2.4% | 1 → 1 | 1 → 1 | 36 → 36 |
| `cleanup/pair_1_early` | 777 → 775 | 362 → 362 | +0.0% | 2 → 2 | 0 → 0 | 26 → 26 |
| `cleanup/speed_1_early` | 1,223 → 1,578 | 565 → 727 | -22.3% | 2 → 2 | 1 → 1 | 64 → 64 |
| `cleanup/pair_1_late` | 777 → 801 | 363 → 373 | -2.7% | 2 → 2 | 0 → 0 | 26 → 26 |
| `cleanup/speed_1_late` | 1,223 → 1,247 | 566 → 577 | -1.9% | 2 → 2 | 1 → 1 | 64 → 64 |
| `cleanup/pair_128_clean` | 44,892 → 43,072 | 20,487 → 19,650 | +4.3% | 1 → 1 | 1 → 1 | 4,521 → 4,521 |
| `cleanup/speed_128_clean` | 70,102 → 72,273 | 32,205 → 33,016 | -2.5% | 1 → 1 | 1 → 1 | 5,673 → 5,673 |
| `cleanup/pair_128_early` | 50,566 → 51,578 | 23,065 → 23,538 | -2.0% | 2 → 2 | 1 → 1 | 6,044 → 6,044 |
| `cleanup/speed_128_early` | 77,268 → 80,405 | 35,255 → 36,678 | -3.9% | 2 → 2 | 1 → 1 | 7,580 → 7,580 |
| `cleanup/pair_128_late` | 55,290 → 57,412 | 25,276 → 26,203 | -3.5% | 2 → 2 | 1 → 1 | 6,044 → 6,044 |
| `cleanup/speed_128_late` | 85,122 → 88,676 | 38,883 → 40,528 | -4.1% | 2 → 2 | 1 → 1 | 7,580 → 7,580 |
| `cleanup/pair_4096_clean` | 1,419,926 → 1,501,938 | 648,100 → 685,582 | -5.5% | 1 → 1 | 1 → 1 | 163,695 → 163,695 |
| `cleanup/speed_4096_clean` | 2,380,908 → 2,311,019 | 1,087,107 → 1,054,683 | +3.1% | 1 → 1 | 1 → 1 | 200,559 → 200,559 |
| `cleanup/pair_4096_early` | 1,949,538 → 1,939,278 | 890,186 → 885,240 | +0.6% | 2 → 2 | 1 → 1 | 218,276 → 218,276 |
| `cleanup/speed_4096_early` | 2,428,168 → 2,516,699 | 1,108,304 → 1,149,049 | -3.5% | 2 → 2 | 1 → 1 | 267,428 → 267,428 |
| `cleanup/pair_4096_late` | 1,852,323 → 1,831,712 | 845,841 → 836,052 | +1.2% | 2 → 2 | 1 → 1 | 218,276 → 218,276 |
| `cleanup/speed_4096_late` | 2,757,603 → 2,631,614 | 1,258,748 → 1,201,353 | +4.8% | 2 → 2 | 1 → 1 | 267,428 → 267,428 |
| `cleanup/0_ordered` | 913 → 920 | 424 → 427 | -0.7% | 2 → 2 | 0 → 0 | 24 → 24 |
| `cleanup/0_duplicates` | 911 → 913 | 424 → 424 | +0.0% | 2 → 2 | 0 → 0 | 24 → 24 |
| `cleanup/0_reverse` | 911 → 913 | 424 → 425 | -0.2% | 2 → 2 | 0 → 0 | 24 → 24 |
| `cleanup/1_ordered` | 4,175 → 4,289 | 1,911 → 1,963 | -2.6% | 14 → 14 | 0 → 0 | 192 → 192 |
| `cleanup/1_duplicates` | 4,168 → 4,109 | 1,907 → 1,879 | +1.5% | 14 → 14 | 0 → 0 | 192 → 192 |
| `cleanup/1_reverse` | 4,460 → 4,250 | 2,040 → 1,943 | +5.0% | 14 → 14 | 0 → 0 | 192 → 192 |
| `cleanup/32_ordered` | 50,384 → 49,822 | 22,989 → 22,745 | +1.1% | 14 → 14 | 0 → 0 | 5,528 → 5,528 |
| `cleanup/32_duplicates` | 50,257 → 47,381 | 22,924 → 21,690 | +5.7% | 14 → 14 | 0 → 0 | 4,600 → 4,600 |
| `cleanup/32_reverse` | 58,464 → 56,348 | 26,688 → 25,702 | +3.8% | 24 → 24 | 0 → 0 | 10,136 → 10,136 |
| `cleanup/4096_ordered` | 6,456,520 → 6,024,114 | 2,948,285 → 2,750,424 | +7.2% | 14 → 14 | 0 → 0 | 731,704 → 731,704 |
| `cleanup/4096_duplicates` | 6,204,306 → 5,932,332 | 2,832,553 → 2,709,311 | +4.5% | 14 → 14 | 0 → 0 | 611,032 → 611,032 |
| `cleanup/4096_reverse` | 39,148,530 → 39,010,771 | 17,916,580 → 17,805,024 | +0.6% | 24 → 24 | 0 → 0 | 1,321,528 → 1,321,528 |

### Focused checks

The scalar/selection table uses 100,000 iterations with the final core implementation. Packing checks use the main iteration counts. The large-buffer and unchanged-caller rechecks use the final binaries; their iteration counts are recorded in the reproduction commands below. These measurements do not supersede the main samples.

### Scalar and selection recheck

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `bpm_summary/0_uniform_owned` | 3 → 6 | 1 → 3 | -66.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/1_uniform_owned` | 21 → 26 | 10 → 12 | -16.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_filtered_values` | 38 → 38 | 17 → 17 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_filtered_map` | 45 → 44 | 21 → 20 | +5.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/2_filtered_owned` | 187 → 58 | 85 → 26 | +226.9% | 1 → 0 | 0 → 0 | 16 → 0 |
| `bpm_summary/8_dense_owned` | 272 → 145 | 124 → 66 | +87.9% | 1 → 0 | 0 → 0 | 64 → 0 |
| `bpm_summary/32_reverse_owned` | 650 → 511 | 297 → 233 | +27.5% | 1 → 0 | 0 → 0 | 256 → 0 |
| `nps_summary/0_uniform_owned` | 17 → 3 | 8 → 1 | +700.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/1_uniform_owned` | 17 → 4 | 8 → 2 | +300.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/2_uniform_owned` | 21 → 8 | 9 → 3 | +200.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_dense_owned` | 123 → 117 | 56 → 54 | +3.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_dense_owned` | 425 → 416 | 194 → 190 | +2.1% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/64_dense_owned` | 697 → 731 | 318 → 333 | -4.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_dense_owned` | 893 → 767 | 407 → 350 | +16.3% | 1 → 0 | 0 → 0 | 520 → 0 |
| `nps_summary/128_dense_owned` | 1,317 → 1,210 | 601 → 552 | +8.9% | 1 → 0 | 0 → 0 | 1,024 → 0 |
| `nps_summary/129_dense_owned` | 1,337 → 1,303 | 610 → 594 | +2.7% | 1 → 1 | 0 → 0 | 1,032 → 1,032 |
| `nps_summary/2_uniform_warm` | 12 → 13 | 6 → 6 | +0.0% | 0 → 0 | 0 → 0 | 0 → 0 |

### Longer summary control recheck

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `bpm_summary/8_dense_warm` | 139 → 141 | 64 → 65 | -1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `bpm_summary/128_filtered_map` | 1,415 → 1,444 | 646 → 659 | -2.0% | 1 → 1 | 0 → 0 | 1,024 → 1,024 |
| `nps_summary/32_special_in_place` | 406 → 384 | 185 → 175 | +5.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/128_sparse_in_place` | 626 → 615 | 286 → 281 | +1.8% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/32_uniform_cold` | 777 → 786 | 355 → 359 | -1.1% | 1 → 1 | 0 → 0 | 256 → 256 |
| `nps_summary/65_special_warm` | 703 → 713 | 321 → 326 | -1.5% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/8_dense_cold` | 341 → 342 | 156 → 156 | +0.0% | 1 → 1 | 0 → 0 | 64 → 64 |
| `bpm_summary/8_dense_owned` | 300 → 157 | 137 → 72 | +90.3% | 1 → 0 | 0 → 0 | 64 → 0 |
| `nps_summary/128_dense_owned` | 1,472 → 1,278 | 672 → 584 | +15.1% | 1 → 0 | 0 → 0 | 1,024 → 0 |

### Packing and composed gains recheck

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `pack_timing/4096_2_false` | 53,194 → 24,704 | 24,345 → 11,283 | +115.8% | 1 → 0 | 0 → 0 | 65,536 → 0 |
| `pack_timing/4096_4_false` | 54,595 → 26,046 | 24,941 → 11,888 | +109.8% | 1 → 0 | 0 → 0 | 65,536 → 0 |
| `pack_timing/4096_8_false` | 53,190 → 26,416 | 24,295 → 12,061 | +101.4% | 1 → 0 | 0 → 0 | 65,536 → 0 |
| `pack_timing/32_4_false` | 517 → 343 | 238 → 157 | +51.6% | 1 → 0 | 0 → 0 | 512 → 0 |
| `pack_raw/32_2` | 8,740 → 8,340 | 3,988 → 3,807 | +4.8% | 4 → 3 | 0 → 0 | 1,168 → 656 |
| `pack_raw/4096_2` | 1,038,569 → 1,016,494 | 474,185 → 464,036 | +2.2% | 4 → 3 | 0 → 0 | 147,472 → 81,936 |

### Large buffers and mixed builders recheck

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `pack_timing/4096_2_true` | 72,837 → 70,020 | 33,384 → 31,980 | +4.4% | 1 → 1 | 0 → 0 | 65,536 → 65,536 |
| `pack_timing/4096_3_true` | 156,322 → 150,796 | 71,505 → 68,854 | +3.9% | 0 → 0 | 0 → 0 | 0 → 0 |
| `pack_timing/4096_6_false` | 110,797 → 109,597 | 50,672 → 50,264 | +0.8% | 1 → 1 | 0 → 0 | 131,072 → 131,072 |
| `pack_timing/4096_15_true` | 365,855 → 377,747 | 167,364 → 172,731 | -3.1% | 0 → 0 | 1 → 1 | 262,144 → 262,144 |
| `pack_raw/4096_3` | 2,173,217 → 2,196,363 | 992,413 → 1,003,268 | -1.1% | 4 → 4 | 1 → 1 | 294,928 → 294,928 |
| `pack_raw/4096_6` | 2,059,340 → 2,246,304 | 940,419 → 1,025,883 | -8.3% | 5 → 5 | 0 → 0 | 294,928 → 294,928 |
| `pack_raw/4096_15` | 4,085,775 → 4,090,284 | 1,865,376 → 1,867,773 | -0.1% | 6 → 6 | 1 → 1 | 589,840 → 589,840 |
| `cleanup/4096_ordered` | 7,216,280 → 7,056,879 | 3,295,537 → 3,222,831 | +2.3% | 14 → 14 | 0 → 0 | 731,704 → 731,704 |

### Unchanged caller recheck

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `standard/leading_detailed` | 6,553 → 6,184 | 2,992 → 2,822 | +6.0% | 2 → 2 | 0 → 0 | 24,588 → 24,588 |
| `standard/fragmented_simple` | 41,662 → 42,870 | 19,014 → 19,569 | -2.8% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |
| `standard/uniform_total` | 7,067 → 6,106 | 3,225 → 2,787 | +15.7% | 1 → 1 | 0 → 0 | 26 → 26 |
| `standard/short_detailed` | 637 → 612 | 291 → 279 | +4.3% | 2 → 2 | 0 → 0 | 492 → 492 |
| `sn/short_simple` | 1,002 → 1,018 | 457 → 464 | -1.5% | 1 → 1 | 0 → 0 | 155 → 155 |
| `sn/uniform_three` | 12,329 → 12,733 | 5,628 → 5,810 | -3.1% | 3 → 3 | 0 → 0 | 480 → 480 |
| `streams/uniform_combined` | 25,273 → 24,790 | 11,533 → 11,315 | +1.9% | 6 → 6 | 0 → 0 | 33 → 33 |
| `report/json/16_dense` | 91,887 → 91,373 | 41,940 → 41,702 | +0.6% | 18 → 18 | 0 → 0 | 318 → 318 |
| `cleanup/speed_1_early` | 1,298 → 1,355 | 592 → 620 | -4.5% | 2 → 2 | 1 → 1 | 64 → 64 |

### Mixed delay/warp builder recheck

| Case | CPU cycles, old → new | ns, old → new | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `pack_raw/4096_6` | 2,272,721 → 2,178,744 | 1,037,753 → 995,435 | +4.3% | 5 → 5 | 0 → 0 | 294,928 → 294,928 |

### Timing variability calibration

Both columns in the following table run the identical original executable, with the same fixtures and alternating order. Their CPU/time differences therefore measure host/process variability, not an implementation change. This control does not establish that every slower optimized sample is noise; the optimized/original samples remain above.

### Original/original calibration

| Case | CPU cycles, original A → original B | ns, original A → original B | Call throughput | Allocs | Reallocs | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `pack_raw/4096_6` | 2,083,439 → 2,061,801 | 950,933 → 941,344 | +1.0% | 5 → 5 | 0 → 0 | 294,928 → 294,928 |
| `pack_timing/4096_2_true` | 68,458 → 68,645 | 31,372 → 31,425 | -0.2% | 1 → 1 | 0 → 0 | 65,536 → 65,536 |
| `bpm_summary/8_dense_warm` | 138 → 132 | 63 → 60 | +5.0% | 0 → 0 | 0 → 0 | 0 → 0 |
| `nps_summary/65_special_warm` | 646 → 654 | 296 → 298 | -0.7% | 0 → 0 | 0 → 0 | 0 → 0 |
| `standard/fragmented_simple` | 48,389 → 44,845 | 22,088 → 20,473 | +7.9% | 2 → 2 | 1 → 1 | 83,556 → 83,556 |

### Validation

- Release regression tests: 152 core + 79 rssp + 29 integration tests = 260 passed, zero failed.
- After confirming all three optimizations: `cargo test --release --test all_parity -- --test-threads=22` — 30,489 passed, zero failed.
- Strict release workspace Clippy for all targets, formatting and whitespace checks pass.
- Explicit edge assertions and bitwise transcripts cover empty/singleton inputs, BPM filtering/fallback, non-finite values and signed zero, both NPS cutoffs, dirty scratch buffers and all 16 timing-source combinations with exact/spare capacity.
- Complete corpus outputs, including reports, hashes, timings, labels, durations and NPS, remain byte-identical: 30,843 files, 56,125 supported charts, 30,489 successes and 354 matched errors. No golden data is changed.
- Corpus: 174,790,323 UTF-8 bytes; SHA-256 `e7d2f22bd7b7f48c0335075d8d2ac355063b58809fdd77c42dcec23927a5759d`.
- Core trace: 5,036 rows, 16,649,965 UTF-8 bytes; SHA-256 `546c1245fcc62d42c1e4f3dbec18d173c997fba0da68326b746a9ba373de2fe3`.
- Leaf trace: 57 rows, 29,124,186 UTF-8 bytes; SHA-256 `aac4e521dea9affd1f0e1baa95991e7dd0fa7f4bb10d109b82bed5ff84d290d7`.

### Reproduction

Keep the current harness/fixtures and package version for both builds. For the original build, restore only production bodies in `bpm.rs`, `nps.rs` and `timing.rs` to `b22ddef`; keep the current test module declarations. Save original executables before rebuilding the final bodies. Use identical setup, output destruction and buffer reuse for both versions.

```powershell
cargo test --release -p rssp-core --lib
cargo test --release -p rssp --lib
cargo bench -p rssp --bench hotpath_perf --no-run
$env:RSSP_PASS_FILTER='bpm_summary/32_'
$env:RSSP_PASS_ITERS='5000'
.\saved-core.exe bpm::pass_edges::summary_hotpath --exact --ignored --nocapture --test-threads=1
# NPS: nps::pass_edges::summary_hotpath and nps_summary/{count}_
# Packing/builders: timing::pass_edges::pack_hotpath and pack_timing/ or pack_raw/
# Use the main iteration counts above; scalar/selection rechecks use 100000.
$env:RSSP_HOT_FILTER='streams/'
$env:RSSP_HOT_ITERS='1000'
.\saved-hotpath.exe
# Final unchanged-caller rechecks use 5000 for selected stream/SN/report cases;
# cleanup/4096_ordered and the large core cases use their main counts.
# Alternate old/new, new/old, old/new; compare corpus and _trace outputs separately.
cargo clippy --release --workspace --all-targets -- -D warnings
cargo test --release --test optimization_edges
cargo test --release --test all_parity -- --test-threads=22
```
