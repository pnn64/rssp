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
