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
