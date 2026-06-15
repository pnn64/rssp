# Crossover-cue annotation parity harness

This directory holds the **reference (ITGmania) side** of the row-level
StepParity annotation parity check. It is the companion to:

- `crates/rssp/tests/note_annotations_parity.rs` — the Rust test that diffs
  rssp's per-row annotations against the engine baselines.
- `crates/rssp/examples/dump_annotations.rs` — prints/serialises rssp's own
  annotations for a simfile (`cargo run --example dump_annotations -- chart.ssc [--json]`).

## Why this exists

`tech_counts_parity` already proves rssp's **aggregate** crossover/footswitch/etc.
counts match ITGmania across the corpus. Crossover *cues* (and tech tooling),
however, depend on **per-row** fidelity: which row is a crossover, which columns
the feet land on, **which foot** lands on each (inner pivot vs. outer crossed-over
arrow), and the full per-row tech breakdown. Two different per-row foot
assignments can produce the same totals, so aggregate parity is necessary but not
sufficient. This harness closes that gap by diffing the full per-row annotation
stream — foot identity and the complete `tech_counts`, not just the crossover flag.

## Source of truth

In the 8ms theme the foot/tech data is **not** computed in Lua. `SL-ChartParser.lua`
calls the C++ engine:

```lua
SL[pn].Streams.NoteAnnotations = steps:GetNoteAnnotations()  -- per-row {beat, footPlacement, tech}
local techCounts             = steps:CalculateTechCounts(player)
```

`CrossoverCues.lua` is purely a consumer of that precomputed array. So the
authority is **ITGmania's C++ StepParity foot-placement + tech classifier**, and
rssp-core's `step_parity.rs` is a Rust port of it.

## Schema

Each chart record in a baseline JSON gains a `note_annotations` array. Each
element:

| field          | type        | meaning                                              |
| -------------- | ----------- | ---------------------------------------------------- |
| `beat`         | float       | `annotation.beat`                                    |
| `columns`      | `[int]`     | foot-bearing columns, **0-indexed**, sorted          |
| `feet`         | `[int]`     | foot id on each column (parallel to `columns`): `None=0, LeftHeel=1, LeftToe=2, RightHeel=3, RightToe=4` |
| `note_count`   | int         | number of `footPlacement` keys (feet on this row)    |
| `tech_counts`  | object      | full per-row tech: one count per `TechCountsCategory` (`crossovers`, `half_crossovers`, `full_crossovers`, `footswitches`, `up_footswitches`, `down_footswitches`, `sideswitches`, `jacks`, `brackets`, `doublesteps`) |

`columns` is 0-indexed to line up with rssp's `RowAnnotation.column_mask` bit
positions (ITGmania `footPlacement` keys are 1-indexed, so the dumper subtracts
1). `feet` carries the `footPlacement` *value* for each column, and `tech_counts`
tallies the per-row `tech` list — together they cover the **whole**
`GetNoteAnnotations` payload (foot identity + full tech), matching rssp's
`RowAnnotation.feet` / `RowAnnotation.tech`. Per-row `tech_counts` summed over a
chart equal the aggregate `TechCounts`. As in the engine, crossover-ness is not a
stored field — derive it from `tech_counts.crossovers > 0`.

Example:

```json
[
  {
    "steps_type": "dance-single",
    "difficulty": "Challenge",
    "meter": 12,
    "note_annotations": [
      { "beat": 4.0, "columns": [0], "feet": [1], "note_count": 1,
        "tech_counts": { "crossovers": 0, "half_crossovers": 0, "full_crossovers": 0,
          "footswitches": 0, "up_footswitches": 0, "down_footswitches": 0,
          "sideswitches": 0, "jacks": 0, "brackets": 0, "doublesteps": 0 } },
      { "beat": 4.5, "columns": [3], "feet": [3], "note_count": 1,
        "tech_counts": { "crossovers": 1, "half_crossovers": 1, "full_crossovers": 0,
          "footswitches": 0, "up_footswitches": 0, "down_footswitches": 0,
          "sideswitches": 0, "jacks": 0, "brackets": 0, "doublesteps": 0 } }
    ]
  }
]
```

## Generating baselines

1. Drop `note_annotations_dump.lua` into
   [`pnn64/itgmania-reference-harness`](https://github.com/pnn64/itgmania-reference-harness)
   and call `NoteAnnotationsForSteps(steps)` where it already builds each chart
   record, attaching the result under `note_annotations`. Keep the harness's
   existing md5/baseline-path layout
   (`baseline/<md5[0:2]>/<md5>.json.zst`, md5 of the raw simfile bytes).
2. **Run it on the exact engine build the feature targets** — the 8ms fork, not
   stock ITGmania. StepParity cost weights drive foot placement, which drives
   crossover classification; record the engine commit next to the baselines.
3. Commit the regenerated baselines to
   [`pnn64/rssp-tests`](https://github.com/pnn64/rssp-tests) (the `tests/data`
   submodule).

## Running the parity test

Against the committed corpus + baselines:

```bash
cargo test --test note_annotations_parity
```

Against an arbitrary song library / baseline location (env overrides):

```bash
RSSP_PARITY_PACKS_DIR="D:/github/deadsync-0/target/local/songs" \
RSSP_PARITY_BASELINE_DIR="D:/path/to/baselines" \
cargo test --test note_annotations_parity
```

The test is **forward compatible**: charts whose baseline lacks a
`note_annotations` field (or whose simfile has no baseline at all) are skipped,
so it stays green until baselines are regenerated with annotation data. Once the
data is present it asserts, for every annotated row, exact equality of beat
(±1e-3), foot-bearing columns, **per-column foot identity** (`feet`), foot
count, and the **full per-row tech** (`tech_counts`) — crossover-ness included,
since `tech_counts.crossovers` is part of that vector. The `feet` / `tech_counts`
checks engage only when the baseline carries them, so older crossover-only
baselines keep passing.

## Sanity check already in place

Running `dump_annotations` over a real corpus shows the **per-row `tech_counts`
summed over a chart equal `tech_counts.crossovers`/etc.** in the aggregate. Since
`tech_counts_parity` already validates those aggregates against ITGmania golden
data, this is transitive evidence that the per-row tech is aggregate-correct vs
the engine; the harness above adds the missing per-row *position* and
*foot-identity* proof (e.g. ITL Online 2026: 0 mismatches over 179,404 rows).
