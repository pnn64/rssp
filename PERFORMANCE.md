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
