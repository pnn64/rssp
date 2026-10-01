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
