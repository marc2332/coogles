# Coogles

Track Rust LOC across Git history with JSON and HTML reports.

## Install

```sh
cargo install --git https://github.com/marc2332/coogles --locked
```

## Usage

```sh
coogles /path/to/repo --all --stops 50
```

Multiple repositories:

```sh
coogles /path/to/freya /path/to/coogles --all --stops 50
```

This creates `report.json` and `report.html`.

## Clone top Rust repositories

```sh
gh auth login
./clone-rust-repos.sh --jobs 4
```

Clones the top 1,000 Rust repositories into `data/`. Use `--limit N` for fewer.

## Generate downloaded reports

```sh
./generate-reports.sh --workers 8 --stops 50
./generate-index.sh
```

Reports are written under `reports/OWNER/REPO/`.

## Prepare GitHub Pages

```sh
./prepare-github-pages.sh
```

This prepares the local `.gh-pages` worktree. Commit and push that branch manually.
