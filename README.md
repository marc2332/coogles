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

## Clone top Rust repositories

From this checkout, with Git, `gh`, `jq` and `xargs` installed:

```sh
gh auth login
./clone-rust-repos.sh --jobs 4
```

Clones the top 1,000 public, non-fork Rust repositories by stars into ignored `data/OWNER/REPO` directories. Bare clones preserve full default-branch history without checkouts. Existing clones are skipped on reruns. Rankings are cached in `data/repositories.json`, delete that file to refetch them. Git clone progress is shown.

Use `--limit 100` for a smaller batch. Full histories can require substantial disk space.

## Generate reports for downloaded repositories

After cloning finishes:

```sh
./generate-reports.sh --workers 8 --stops 50
```

Always uses `--all` and writes `report.html` and `report.json` for each repository under `reports/REPO` or `reports/OWNER/REPO`, matching the layout in `data/`.

The script builds Coogles once in release mode. Use `--binary coogles` to use an installed binary instead. Workers are concurrent repositories, with one Rust thread per worker by default. Use `--threads N` to change that.

## Prepare GitHub Pages

```sh
./generate-index.sh
./prepare-github-pages.sh
```

This creates or reuses the local `.gh-pages` worktree and copies only HTML files while preserving `OWNER/REPO/report.html` paths. It adds `.nojekyll`, removes stale deployed reports and verifies index links. Repository report titles link to their GitHub origins.

The script does not commit or push. Review `.gh-pages`, then commit and push the `gh-pages` branch manually. Configure GitHub Pages to deploy from the branch root.
