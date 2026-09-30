# Coogles

A Rust CLI that measures Rust source across Git history and writes JSON plus a self-contained HTML plot. No checkout, external Git process, network access or web server is needed during analysis.

## Install

```sh
cargo install --git https://github.com/marc2332/coogles --locked
```

Then run:

```sh
coogles /path/to/repository --all --stops 25
```

## Run

```sh
cargo run --release -- /path/to/repository \
  --commits 300 --step 10 \
  --output reports/history.json --html reports/history.html
```

This samples `HEAD~10`, `HEAD~20`, through `HEAD~300`, giving 30 stops. HEAD itself is excluded. The final requested offset is always included, even when it is not divisible by the step.

To specify the exact number of stops instead:

```sh
cargo run --release -- /home/marc/Projects/freya/trunk \
  --commits 100 --stops 10 \
  --output reports/freya.json --html reports/freya.html
```

To scan the entire available first-parent history:

```sh
cargo run --release -- /path/to/repository --all --stops 30
```

`--all` replaces `--commits`, not the traversal mode. It does not include side-branch commits separately. Use `--all --step 10` to sample every ten commits instead.

Stops are evenly distributed using integer commit offsets, with the last stop at the requested depth or the available history boundary. `--stops` and `--step` are mutually exclusive. `--threads N` limits Rayon workers.

Open the generated HTML directly in your browser. It embeds the JSON, provides five selectable curves, commit/count tooltips, a data table and a JSON download button. The horizontal axis is commit distance, not elapsed time. Commit dates appear in the tooltips.

History follows the first parent at merges. GitHub's total commit count includes merged branches, so it can exceed this history length. If the requested depth exceeds available history, the CLI warns and redistributes stops over the available history. The JSON records requested and actual depths and whether history was truncated. A shallow boundary is detected and reported separately. If there are fewer available offsets than requested stops, the stop count is reduced. A repository with only HEAD produces one stop at offset zero. Bare repositories are supported. Dirty working-tree changes are not included or modified.

## Counting rules

All tracked regular `.rs` files in each snapshot are included, including vendored and generated source if committed. Submodules and symbolic links are not followed.

- `code_loc`: total physical nonblank lines containing Rust tokens other than comments, including tests and examples. Attributes and brace-only lines count. A line containing both code and a trailing comment counts in both categories.
- `test_code_loc`: code in paths with a `tests/` component, configured Cargo test targets, `#[cfg(test)]` items and test functions. Recognizes attributes whose final segment is `test` or `rstest`, including `#[tokio::test]`. Test-only external modules and their descendants are included.
- `example_code_loc`: code in paths with an `examples/` component or configured Cargo example targets and their external module descendants.
- `comment_loc`: ordinary `//` comment lines, excluding documentation. `////` is an ordinary comment. Trailing comments count.
- `doc_loc`: `inner_doc_loc` (`//!`) plus `outer_doc_loc` (`///`). Rustdoc fenced code remains documentation, not example or test code.
- `block_comment_loc` and `block_doc_loc`: separate counts for block comments and block documentation, neither included in the line-comment metrics.

Categories can overlap and should not be summed as a partition of the repository. File-based tests/examples include helper code, not just test function bodies.

Historical `Cargo.toml` files are read to locate explicit `[[test]]` and `[[example]]` paths. This does not execute Cargo or build old code.

This is source analysis, not compiler-expanded analysis. Macros are not expanded. Arbitrary custom test attributes, `cfg_attr`, feature-only test configurations, conditional module paths, `include!` and unusual inline-module `#[path]` layouts are not fully resolved. Simple test predicates and `all`/`any` combinations that require `test` are recognized. Source shared between production and test module graphs may be classified as test code.

Parser failures retain lexical counts but may omit inline tests and module relationships. Each snapshot lists `parse_errors`, and both the CLI and HTML show a warning. Non-UTF-8 source or paths, invalid Cargo manifests and unsupported target paths produce explicit errors.

## Performance

- `gix` reads commit, tree and blob objects directly, with networking and worktree features disabled.
- Blob object IDs deduplicate unchanged file versions across snapshots.
- `rayon` analyzes unique blobs in batches of 256 with local repository handles.
- `ra-ap-rustc_lexer` distinguishes comments from literals, including raw strings and nested block comments.
- `syn` identifies inline test source ranges and external module relationships.
- Content analysis is cached separately from snapshot-specific path classification.

The cache lasts for one invocation. Snapshots still traverse their trees, and parsed metric summaries remain in memory. The recorded analysis time excludes output serialization and writing. Unicode identifier tables are pinned to match the lexer's Unicode version.

## Development

```sh
cargo check
cargo test --bin coogles
```

`reports/` is ignored so repository-specific generated output is not accidentally committed.
