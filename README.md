# rssp - Rust Stepmania Simfile Parser

A command-line tool written in Rust for parsing, analyzing, and reporting statistics on StepMania simfiles (`.sm`, `.ssc`). It focuses on detailed analysis of 4-panel dance game charts (like DDR/ITG). It also powers the simfile parser in the [DeadSync](https://github.com/pnn64/deadsync) game engine, where it's included as a Git submodule.

## Inspiration

This project is inspired by Breakdown Buddy and builds upon ideas from [simfile-sidekick](https://gitlab.com/artimst/simfile-sidekick) by Steven Artim (artimst).

## Motivation

Why create another simfile parser?

* **Focus & Flexibility:** While previous tools might have focused on integrating with databases or Discord bots, `rssp` is primarily designed for direct command-line analysis of individual simfiles. However, its speed makes it trivial to script for processing thousands of files.
* **Performance:** The previous Python-based tool could take many minutes to process large collections (10k+ files). This Rust implementation achieves significantly better performance, capable of parsing similar numbers of files in seconds.
* **Enhanced Statistics & Features:** `rssp` aims to add features and refine statistics, including:
    *   Generating a chart's SHA1 hash.
    *   Detection of a wider range of predefined step patterns.
    *   An improved algorithm for detecting monotonic (mono) stepping sequences.
    *   Parsing of common technical notations embedded in chart metadata.
    *   The ability to strip common "tournament tags" from song titles for cleaner output.

## Features

*   **Simfile Parsing:** Supports both `.sm` and `.ssc` formats.
*   **Metadata Extraction:** Retrieves Title, Subtitle, Artist, Transliterations, Offset, BPMs, etc.
*   **Tag Cleaning:** Optionally strips common leading tags (e.g., `[Foo]`, `123- `) from titles.
*   **Chart Statistics:** Calculates comprehensive stats per chart:
    *   Total Arrows, Arrow counts per direction (L/D/U/R)
    *   Total Steps (lines with at least one arrow)
    *   Jumps, Hands, Holds, Rolls, Mines
*   **Density Analysis:**
    *   Calculates Notes Per Second (NPS): Median and Peak.
    *   Counts measures of different stream densities (16ths, 20ths, 24ths, 32nds) and break measures.
    *   Generates textual chart breakdowns in Detailed, Partial, and Simplified formats.
    *   Calculates Stream Percentage (raw and adjusted).
    *   Computes a "Tier BPM" based on sustained density and BPM.
*   **Difficulty Rating:**
    *   Calculates a Matrix Rating by aggregating stream sections and applying a difficulty matrix.
*   **Pattern Detection:** Identifies various common and complex step patterns:
    *   Candles (Left/Right) & Candle Percentage
    *   Mono (Left-Facing/Right-Facing) & Mono Percentage (configurable threshold)
    *   Boxes (LR, UD, Corners)
    *   Anchors (L, D, U, R)
    *   Towers, Triangles, Staircases (Regular, Alt, Double, Inverted)
    *   Sweeps (Regular, Candle, Inverted)
    *   Copters, Spirals, Turbo Candles, Hip Breakers, Doritos, Luchis (and their inverted variants)
*   **Technical Notation Parsing:** Extracts known technical notations (e.g., `STR+`, `BXF`, `FS-`) from the Credit/Description fields.
*   **Hashing:** Generates SHA1 hashes for charts:
    *   Standard hash (based on minimized notes + effective BPMs)
    *   BPM-Neutral hash (useful for comparing FB/FP charts regardless of speed)
*   **Multiple Output Formats:**
    *   `Pretty`: Human-readable summary (Default).
    *   `Full`: Detailed text output including all patterns and stats.
    *   `JSON`: Machine-readable structured output.
    *   `CSV`: Comma-separated values, one row per chart.
*   **Density Graph Generation:** Optionally creates PNG images visualizing the NPS density over the chart's duration (standard and alternative color schemes available).

## Installation

1. **Install Rust:** If you don't have Rust installed, get it from [rustup.rs](https://rustup.rs/).
2. **Clone the Repository:**
    ```bash
    git clone https://github.com/pnn64/rssp.git
    cd rssp
    ```
3. **Build the Project:**
    ```bash
    cargo build --release
    ```
    The executable will be located at `target/release/rssp`. You can copy this to a directory in your system's `PATH` for easier access.

## Usage

The basic command structure is:

```bash
rssp <path/to/simfile_or_folder> [options]
```

**Arguments:**

* `<path/to/simfile_or_folder>`: **Required.** The path to a simfile (`.sm` or `.ssc`) or a folder containing simfiles.
  * **Single File:** Analyzes the specified simfile.
  * **Folder:** Recursively scans the folder for simfiles. When both `.sm` and `.ssc` files exist in the same directory, the `.ssc` file is preferred.

**Options:**

* `--full`: Output the detailed "Full" report format instead of the default "Pretty" summary.
* `--json`: Output the report in JSON format.
* `--csv`: Output the report in CSV format (header row followed by one data row per chart).
* `--png`: Generate a density graph PNG image for each chart (using the default color scheme). The filename will be `<chart_hash>.png`.
* `--png-alt`: Generate a density graph PNG image for each chart (using the alternative color scheme). The filename will be `<chart_hash>-alt.png`.
* `--strip-tags`: Clean common prefixes like `[TAG]` or `123- ` from the song title before displaying.
* `--mono-threshold <N>`: Set the minimum number of consecutive steps required to count a segment as "mono" (default: 6). `<N>` must be a positive integer.

**Examples:**

*   Show the default pretty report for a `.sm` file:
    ```bash
    ./rssp "My Songs/Cool Song/Cool Song.sm"
    ```
*   Generate a JSON report for an `.ssc` file:
    ```bash
    ./rssp "/path/to/itg/packs/Hard Stuff/My Fav/My Fav.ssc" --json
    ```
*   Generate the Full report and a density graph PNG:
    ```bash
    ./rssp song.sm --full --png
    ```
*   Generate a CSV report with a higher mono threshold and stripped title tags:
    ```bash
    ./rssp song.ssc --csv --mono-threshold 8 --strip-tags
    ```
*   Analyze an entire folder recursively:
    ```bash
    ./rssp "C:/Games/ITGmania/Songs" --csv
    ```
*   Analyze a specific pack folder:
    ```bash
    ./rssp "Songs/MyCrazyPack" --json
    ```
*   Save folder analysis results to a file using shell redirection:
    ```bash
    ./rssp "Songs/MyPack" --csv > output.csv
    ```
*   Calculate a matrix rating without analyzing a file, use the `--matrix` flag with BPM and measure counts:
    ```bash
    ./rssp --matrix --bpm <BPM> --measures <MEASURES>
    ```

## Output Formats

* **Pretty (Default):** A human-readable summary focusing on key song details, BPM, NPS, core stats (steps, jumps, hands, etc.), and primary patterns (Candles, Mono, Boxes, Anchors). Includes simplified breakdowns if available.
* **Full:** A comprehensive text output including all song metadata, detailed BPM info, full chart stats, all detected pattern counts (including complex ones like sweeps, spirals, etc.), and the detailed/partial/simplified breakdowns.
* **JSON:** A structured JSON object containing all song and chart information, suitable for programmatic use. Fields are grouped logically (e.g., `chart_info`, `arrow_stats`, `pattern_counts`).
* **CSV:** A header row followed by one data row per chart. Contains a flattened representation of most song and chart statistics, suitable for spreadsheets or data analysis pipelines.
* **PNG / PNG-Alt:** Generates density graph images visualizing NPS over the chart's duration. Useful for quickly identifying high-intensity sections. Filenames are based on the chart's unique SHA1 hash.

## Sample Output
```
perfecttaste@LAPTOP ~ $ ./rssp timelessbeatz.sm --full
--- Song Details ---
Title: [18] [170] TIMELESS BEATZ
Subtitle: ~Breakbeats To Chill To~
Artist: Various Artists (mixed by Rems)
Length: 59m 43s
BPM: 170
Average BPM: 170.00
Median BPM: 170.00
BPM Data: 0.000=170.000
Offset: 0.009

Challenge 18 : CSktls Rems mang Janus5k Mango
---------------------------------------------
Step Type: dance-single
Matrix Rating: 18.6493
Tier BPM: 170
SHA1 Hash: da8bc528eb55ea86
BPM Neutral SHA1 Hash: 9b60cf786177ffc5

NPS: 11.33 Median/Peak
Total Stream: 1998 (78.72%/80.60% Adj.)
    16th_streams: 1998
    20th_streams: 0
    24th_streams: 0
    32nd_streams: 0
Total Break: 481 (19.40%)

--- Chart Info ---
Steps: 33004 (33022 arrows) [8320 left, 8164 down, 8235 up, 8303 right]
Jumps: 18
Hands: 0
Holds: 58
Rolls: 11
Mines: 0
Lifts: 0
Fakes: 0

--- Pattern Analysis ---
Candles: 2532 (1264 left, 1268 right)
Candle%: 15.34%
Mono: 6677 (3347 left-facing, 3330 right-facing)
Mono%: 20.23%
Boxes: 252 (45 LRLR, 24 UDUD, 26 LDLD, 28 LULU, 28 RDRD, 32 RURU)
Anchors: 605 (276 left, 19 down, 34 up, 276 right)

--- Detailed Breakdown ---
32 (16) 31 (3) 29 (17) 15 14 (2) 15 30 (2) 15 14 (2) 15 14 (34) 79 31 (3) 45 79 31 15 (17) 16 (24) 23 47 31 31 30 (2) 14 (2) 63 15 15 15 16 (16) 15 15 15 31 (33) 30 (2) 31 15 15 15 15 31 (17) 103 54 (26) 30 (2) 48 (16) 14 (2) 62 (2) 30 (2) 47 32 (40) 31 32 (32) 31 15 7 7 7 7 7 3 5 1 3 (33) 9 1 84 (16) 46 (2) 31 (25) 14 (2) 16 (48) 15 95 63
--- Partially Simplified ---
32 / 31 - 29 / 30* - 46* - 30* - 30* | 111* - 173* / 16 / 166* - 14 - 128* / 79* | 30 - 127* / 158* / 30 - 48 / 14 - 62 - 30 - 80* | 64* / 103* | 96* / 46 - 31 / 14 - 16 | 175*
--- Simplified Breakdown ---
32 / 63* / 142* | 287* / 16 / 312* / 79* | 159* / 158* / 80* / 192* | 64* / 103* | 96* / 79* / 32* | 175*

--- Other Patterns ---
Total Towers: 5 (1 LR, 0 UD, 2 LD, 1 LU, 1 RD, 0 RU)
Total Triangles: 2893 (727 LDL, 716 LUL, 701 RDR, 749 RUR)
Staircases: 5275 (1280 Left, 1341 Right, 1322 Left Inv, 1332 Right Inv)
Alt Staircases: 1153 (301 Left, 283 Right, 299 Left Inv, 270 Right Inv)
Double Staircases: 30 (7 Left, 8 Right, 7 Left Inv, 8 Right Inv)
Sweeps: 212 (46 Left, 51 Right, 49 Left Inv, 66 Right Inv)
Candle Sweeps: 29 (9 Left, 4 Right, 6 Left Inv, 10 Right Inv)
Copters: 61 (10 Left, 12 Right, 22 Left Inv, 17 Right Inv)
Spirals: 524 (134 Left, 131 Right, 138 Left Inv, 121 Right Inv)
Turbo Candles: 27 (5 Left, 8 Right, 7 Left Inv, 7 Right Inv)
Hip Breakers: 13 (4 Left, 3 Right, 3 Left Inv, 3 Right Inv)
Doritos: 896 (215 Left, 220 Right, 243 Left Inv, 218 Right Inv)
Luchis: 187 (51 Left DU, 44 Left UD, 54 Right DU, 38 Right UD)

Elapsed Time: 18.667896ms
```

## TODO
- [x] Count Fakes and Lifts
- [x] Properly parse Ivaltek (indented NOTES) from SHARPNELSTREAMZ v2
- [ ] Properly handle empty charts
- [x] Fix parsing of multiple simfile authors with '&' (for instance Maxx & Zaia)
- [x] Check that some special characters are parser correctly (CODE:Ø as opposed to CODE\:Ø)
- [x] Fix parsing for "No Tech" in artist name
- [ ] Doubles (8-panel) parsing
- [x] Custom patterns flag --custom-pattern DULDUDLR
- [x] Proper parsing for Tech (port Step Parity from ITGMania)
- [ ] Add many tests for edge case simfiles
- [x] Add "matrix rating" (calculate estimated rating based on rating matrix sheet)

## Contributing

Contributions are welcome! Please feel free to open an issue to report bugs or suggest features, especially if you have a simfile that does not parse properly/accurately.
