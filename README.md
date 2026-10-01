# Coogles

Track Rust LOC across Git history with JSON output and a self-contained HTML dashboard.

## Install

```sh
cargo install --git https://github.com/marc2332/coogles --locked
```

## Usage

Scan all available first-parent history with 50 samples, including HEAD and the oldest commit:

```sh
coogles /path/to/repo --all --stops 50
```

Compare multiple repositories:

```sh
coogles /path/to/freya /path/to/coogles /path/to/servo --all --stops 50
```

Or sample every 10 commits over the last 300:

```sh
coogles /path/to/repo --commits 300 --step 10
```

By default, output is written to `report.json` and `report.html`. Open the HTML directly in your browser, no server required.

Use `--output` and `--html` to choose output paths, and `--threads` to limit workers.

The dashboard includes global line toggles, percentages, compact mode, time axes and a month-range slider. View settings persist across reloads.
