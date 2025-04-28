# rssp - Rust Stepmania Simfile Parser

A command-line tool written in Rust for parsing, analyzing, and reporting statistics on StepMania simfiles (`.sm`, `.ssc`). It focuses on detailed analysis of 4-panel dance game charts (like DDR/ITG).

## Inspiration

This project is inspired by and builds upon ideas from [simfile-sidekick](https://gitlab.com/artimst/simfile-sidekick) by Steven Artim (artimst).

## Motivation

Why create another simfile parser?

* **Focus & Flexibility:** While previous tools might have focused on integrating with databases or Discord bots, `rssp` is primarily designed for direct command-line analysis of individual simfiles. However, its speed makes it trivial to script for processing thousands of files.
* **Performance:** The previous Python-based tool could take many minutes to process large collections (10k+ files). This Rust implementation achieves significantly better performance, capable of parsing similar numbers of files in seconds.
* **Enhanced Statistics & Features:** `rssp` aims to add features and refine statistics, including:
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
    *   BPM-Neutral hash (useful for comparing patterns regardless of speed)
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
    git clone https://github.com/your_username/rssp.git # TODO: Replace with your repo URL
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
rssp <path/to/simfile.sm_or_ssc> [options]
```

**Arguments:**

* `<path/to/simfile.sm_or_ssc>`: **Required.** The path to the simfile you want to analyze.

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
    ./target/release/rssp "My Songs/Cool Song/Cool Song.sm"
    ```
*   Generate a JSON report for an `.ssc` file:
    ```bash
    ./target/release/rssp "/path/to/itg/packs/Hard Stuff/My Fav/My Fav.ssc" --json
    ```
*   Generate the Full report and a density graph PNG:
    ```bash
    ./target/release/rssp song.sm --full --png
    ```
*   Generate a CSV report with a higher mono threshold and stripped title tags:
    ```bash
    ./target/release/rssp song.ssc --csv --mono-threshold 8 --strip-tags
    ```

## Output Formats

* **Pretty (Default):** A human-readable summary focusing on key song details, BPM, NPS, core stats (steps, jumps, hands, etc.), and primary patterns (Candles, Mono, Boxes, Anchors). Includes simplified breakdowns if available.
* **Full:** A comprehensive text output including all song metadata, detailed BPM info, full chart stats, all detected pattern counts (including complex ones like sweeps, spirals, etc.), and the detailed/partial/simplified breakdowns.
* **JSON:** A structured JSON object containing all song and chart information, suitable for programmatic use. Fields are grouped logically (e.g., `chart_info`, `arrow_stats`, `pattern_counts`).
* **CSV:** A header row followed by one data row per chart. Contains a flattened representation of most song and chart statistics, suitable for spreadsheets or data analysis pipelines.
* **PNG / PNG-Alt:** Generates density graph images visualizing NPS over the chart's duration. Useful for quickly identifying high-intensity sections. Filenames are based on the chart's unique SHA1 hash.

## TODO

* Doubles (8-panel) parsing
* Custom patterns flag --custom-pattern DULDUDLR
* Proper parsing for Tech (adopt from new ITGMania version)

## Contributing

Contributions are welcome! Please feel free to open an issue to report bugs or suggest features.
