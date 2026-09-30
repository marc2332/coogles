mod analysis;
mod report;

use crate::{
    analysis::{Analysis, analyze},
    report::{Metrics, Snapshot},
};
use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use rayon::prelude::*;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
    time::Instant,
};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Plot Rust source metrics across first-parent Git history"
)]
struct Arguments {
    /// Repository path, including bare repositories
    repository: PathBuf,
    /// Walk this many first-parent edges back from HEAD
    #[arg(long, default_value_t = 300, conflicts_with = "all")]
    commits: usize,
    /// Scan the entire available first-parent history
    #[arg(long)]
    all: bool,
    /// Sample every N commits, also including the requested final offset
    #[arg(long, default_value_t = 10, conflicts_with = "stops")]
    step: usize,
    /// Sample exactly N evenly distributed stops, excluding HEAD
    #[arg(long)]
    stops: Option<usize>,
    /// Rayon worker count, defaults to available parallelism
    #[arg(long)]
    threads: Option<usize>,
    #[arg(long, default_value = "report.json")]
    output: PathBuf,
    #[arg(long, default_value = "report.html")]
    html: PathBuf,
}

#[derive(Serialize)]
struct Configuration {
    commits: usize,
    requested_commits: Option<usize>,
    all: bool,
    history_truncated: bool,
    shallow_boundary: bool,
    offsets: Vec<usize>,
    traversal: &'static str,
    includes_head: bool,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    repository: String,
    head: String,
    configuration: Configuration,
    definitions: &'static str,
    unique_rust_blobs: usize,
    analyzed_bytes: u64,
    elapsed_seconds: f64,
    snapshots: Vec<Snapshot>,
}

#[derive(Clone)]
struct FileEntry {
    path: String,
    object_id: gix::ObjectId,
}

struct Stop {
    offset: usize,
    commit: String,
    timestamp: i64,
    files: Vec<FileEntry>,
}

fn offsets(commits: usize, step: usize, stops: Option<usize>) -> Result<Vec<usize>> {
    ensure!(commits > 0, "--commits must be positive");
    if let Some(stops) = stops {
        ensure!(
            stops > 0 && stops <= commits,
            "--stops must be between 1 and --commits"
        );
        return (1..=stops)
            .map(|index| {
                let offset = (index as u128 * commits as u128) / stops as u128;
                Ok(offset as usize)
            })
            .collect();
    }
    ensure!(step > 0, "--step must be positive");
    let mut offsets: Vec<_> = (step..=commits).step_by(step).collect();
    if offsets.last() != Some(&commits) {
        offsets.push(commits);
    }
    Ok(offsets)
}

fn history_offsets(commits: usize, step: usize, stops: Option<usize>) -> Result<Vec<usize>> {
    if commits == 0 {
        return Ok(vec![0]);
    }
    offsets(commits, step, stops.map(|stops| stops.min(commits)))
}

fn collect_history(
    repository: &gix::Repository,
    head: gix::ObjectId,
    limit: Option<usize>,
) -> Result<(Vec<gix::ObjectId>, bool)> {
    let shallow = repository.shallow_commits()?;
    let mut history = Vec::new();
    let mut commit_id = head;
    loop {
        let commit = repository.find_object(commit_id)?.try_into_commit()?;
        history.push(commit_id);
        if limit == Some(history.len() - 1) {
            return Ok((history, false));
        }
        if shallow
            .as_ref()
            .is_some_and(|commits| commits.contains(&commit_id))
        {
            return Ok((history, true));
        }
        let Some(parent) = commit.parent_ids().next() else {
            return Ok((history, false));
        };
        commit_id = parent.detach();
    }
}

fn collect_files(repository: &gix::Repository, tree_id: gix::ObjectId) -> Result<Vec<FileEntry>> {
    let mut files = Vec::new();
    let mut pending = vec![(String::new(), tree_id)];
    while let Some((prefix, tree_id)) = pending.pop() {
        let tree = repository.find_object(tree_id)?.try_into_tree()?;
        for entry in tree.iter() {
            let entry = entry?;
            let name = std::str::from_utf8(entry.inner.filename.as_ref())
                .context("non-UTF-8 Git path is unsupported")?;
            let path = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.inner.mode.is_tree() {
                pending.push((path, entry.inner.oid.to_owned()));
            } else if entry.inner.mode.is_blob() && (path.ends_with(".rs") || name == "Cargo.toml")
            {
                files.push(FileEntry {
                    path,
                    object_id: entry.inner.oid.to_owned(),
                });
            }
        }
    }
    Ok(files)
}

fn normalize(path: &Path) -> Result<String> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => components.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                ensure!(components.pop().is_some(), "target path escapes repository");
            }
            _ => bail!("absolute target paths are unsupported"),
        }
    }
    Ok(components.join("/"))
}

fn module_base(path: &str) -> PathBuf {
    let path = Path::new(path);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("lib.rs" | "main.rs" | "mod.rs")
    ) {
        parent.to_owned()
    } else {
        parent.join(path.file_stem().unwrap_or_default())
    }
}

fn custom_targets(
    stop: &Stop,
    manifests: &HashMap<gix::ObjectId, toml::Value>,
) -> Result<(HashSet<String>, HashSet<String>)> {
    let mut tests = HashSet::new();
    let mut examples = HashSet::new();
    for file in &stop.files {
        let Some(manifest) = manifests.get(&file.object_id) else {
            continue;
        };
        let parent = Path::new(&file.path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        for (kind, targets) in [("test", &mut tests), ("example", &mut examples)] {
            if let Some(entries) = manifest.get(kind).and_then(toml::Value::as_array) {
                for entry in entries {
                    if let Some(path) = entry.get("path").and_then(toml::Value::as_str) {
                        targets.insert(normalize(&parent.join(path))?);
                    }
                }
            }
        }
    }
    Ok((tests, examples))
}

fn target_files(targets: &HashSet<String>, files: &[FileEntry]) -> HashSet<String> {
    files
        .iter()
        .filter(|file| targets.contains(&file.path))
        .map(|file| file.path.clone())
        .collect()
}

fn aggregate(
    stop: &Stop,
    analyses: &HashMap<gix::ObjectId, Analysis>,
    manifests: &HashMap<gix::ObjectId, toml::Value>,
) -> Result<Snapshot> {
    let (tests, examples) = custom_targets(stop, manifests)?;
    let mut test_files = target_files(&tests, &stop.files);
    let mut example_files = target_files(&examples, &stop.files);
    let paths: HashSet<_> = stop.files.iter().map(|file| file.path.as_str()).collect();
    let mut module_graph: HashMap<String, Vec<String>> = HashMap::new();
    for file in &stop.files {
        if file.path.split('/').any(|part| part == "tests") {
            test_files.insert(file.path.clone());
        }
        if file.path.split('/').any(|part| part == "examples") {
            example_files.insert(file.path.clone());
        }
        if let Some(analysis) = analyses.get(&file.object_id) {
            for module in &analysis.external_modules {
                let base = if module.from_file_parent {
                    Path::new(&file.path)
                        .parent()
                        .unwrap_or_else(|| Path::new(""))
                        .to_owned()
                } else {
                    module_base(&file.path)
                };
                let path = normalize(&base.join(&module.path))?;
                if paths.contains(path.as_str()) {
                    if module.test_only {
                        test_files.insert(path.clone());
                    }
                    module_graph
                        .entry(file.path.clone())
                        .or_default()
                        .push(path);
                }
            }
        }
    }
    for classified in [&mut test_files, &mut example_files] {
        let mut pending: Vec<_> = classified.iter().cloned().collect();
        while let Some(path) = pending.pop() {
            if let Some(children) = module_graph.get(&path) {
                for child in children {
                    if classified.insert(child.clone()) {
                        pending.push(child.clone());
                    }
                }
            }
        }
    }
    let mut metrics = Metrics::default();
    let mut parse_errors = Vec::new();
    for file in &stop.files {
        let Some(analysis) = analyses.get(&file.object_id) else {
            continue;
        };
        metrics.rust_files += 1;
        metrics.code_loc += analysis.code_loc;
        metrics.test_code_loc += if test_files.contains(&file.path) {
            analysis.code_loc
        } else {
            analysis.inline_test_loc
        };
        if example_files.contains(&file.path) {
            metrics.example_code_loc += analysis.code_loc;
        }
        metrics.comment_loc += analysis.comment_loc;
        metrics.inner_doc_loc += analysis.inner_doc_loc;
        metrics.outer_doc_loc += analysis.outer_doc_loc;
        metrics.block_comment_loc += analysis.block_comment_loc;
        metrics.block_doc_loc += analysis.block_doc_loc;
        if let Some(error) = &analysis.parse_error {
            parse_errors.push(format!("{}: {error}", file.path));
        }
    }
    metrics.doc_loc = metrics.inner_doc_loc + metrics.outer_doc_loc;
    parse_errors.sort();
    Ok(Snapshot {
        commit: stop.commit.clone(),
        offset: stop.offset,
        timestamp: stop.timestamp,
        metrics,
        parse_errors,
    })
}

fn write_output(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

fn main() -> Result<()> {
    let arguments = Arguments::parse();
    ensure!(
        arguments.all || arguments.commits > 0,
        "--commits must be positive"
    );
    ensure!(arguments.step > 0, "--step must be positive");
    ensure!(arguments.stops != Some(0), "--stops must be positive");
    ensure!(
        arguments.output != arguments.html,
        "JSON and HTML output paths must differ"
    );
    if let Some(threads) = arguments.threads {
        ensure!(threads > 0, "--threads must be positive");
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()?;
    }
    let started = Instant::now();
    let repository = gix::discover(&arguments.repository).context("opening Git repository")?;
    let head = repository.head_commit()?.id;
    let requested_commits = (!arguments.all).then_some(arguments.commits);
    let (history, shallow_boundary) = collect_history(&repository, head, requested_commits)?;
    let commits = history.len() - 1;
    let history_truncated = requested_commits.is_some_and(|requested| requested > commits);
    if history_truncated || shallow_boundary {
        let boundary = if shallow_boundary {
            "a shallow boundary"
        } else {
            "the repository root"
        };
        eprintln!(
            "Warning: reached {boundary} at HEAD~{commits}. Sampling available first-parent history instead. Merged-branch commits are not counted."
        );
    }
    let offsets = history_offsets(commits, arguments.step, arguments.stops)?;
    if arguments
        .stops
        .is_some_and(|requested| requested > offsets.len())
    {
        eprintln!(
            "Warning: only {} distinct stops are available",
            offsets.len()
        );
    }
    let mut stops = Vec::with_capacity(offsets.len());
    for offset in &offsets {
        let commit_id = history[*offset];
        let commit = repository.find_object(commit_id)?.try_into_commit()?;
        stops.push(Stop {
            offset: *offset,
            commit: commit_id.to_string(),
            timestamp: commit.time()?.seconds,
            files: collect_files(&repository, commit.tree_id()?.detach())?,
        });
    }
    let mut rust_ids = HashSet::new();
    let mut manifest_ids = HashSet::new();
    for stop in &stops {
        for file in &stop.files {
            if file.path.ends_with(".rs") {
                rust_ids.insert(file.object_id);
            } else {
                manifest_ids.insert(file.object_id);
            }
        }
    }
    let mut manifests = HashMap::new();
    for object_id in manifest_ids {
        let blob = repository.find_blob(object_id)?;
        let source = std::str::from_utf8(&blob.data)?;
        manifests.insert(
            object_id,
            toml::from_str::<toml::Value>(source)
                .with_context(|| format!("parsing historical Cargo.toml blob {object_id}"))?,
        );
    }
    let shared = repository.into_sync();
    let rust_ids: Vec<_> = rust_ids.into_iter().collect();
    let mut analyses = HashMap::with_capacity(rust_ids.len());
    let mut analyzed_bytes = 0;
    for batch in rust_ids.chunks(256) {
        let results: Vec<Result<_>> = batch
            .par_iter()
            .map_init(
                || shared.to_thread_local(),
                |repository, object_id| {
                    let blob = repository.find_blob(*object_id)?;
                    let source = std::str::from_utf8(&blob.data)
                        .with_context(|| format!("non-UTF-8 Rust blob {object_id}"))?;
                    Ok((*object_id, analyze(source), blob.data.len() as u64))
                },
            )
            .collect();
        for result in results {
            let (object_id, analysis, bytes) = result?;
            analyzed_bytes += bytes;
            analyses.insert(object_id, analysis);
        }
    }
    let snapshots = stops
        .iter()
        .map(|stop| aggregate(stop, &analyses, &manifests))
        .collect::<Result<Vec<_>>>()?;
    let report = Report {
        schema_version: 1,
        repository: arguments.repository.canonicalize()?.display().to_string(),
        head: head.to_string(),
        configuration: Configuration {
            commits,
            requested_commits,
            all: arguments.all,
            history_truncated,
            shallow_boundary,
            includes_head: offsets.contains(&0),
            offsets,
            traversal: "first-parent",
        },
        definitions: "Physical nonblank code lines, excluding comments. Tests include tests/ paths, configured Cargo test targets, test-only items and recognized test functions. Examples include examples/ paths and configured Cargo example targets. Line comments exclude docs, doc_loc includes //! and ///. Trailing comments count, categories may overlap. Block comments and block docs are separate. Source only, no macro expansion or cfg evaluation.",
        unique_rust_blobs: analyses.len(),
        analyzed_bytes,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        snapshots,
    };
    let json = serde_json::to_string_pretty(&report)?;
    write_output(&arguments.output, json.as_bytes())?;
    write_output(&arguments.html, report::html(&json).as_bytes())?;
    println!(
        "{} stops, {} unique Rust blobs, {:.2} seconds\nJSON: {}\nHTML: {}",
        report.snapshots.len(),
        report.unique_rust_blobs,
        report.elapsed_seconds,
        arguments.output.display(),
        arguments.html.display()
    );
    let failures: usize = report
        .snapshots
        .iter()
        .map(|snapshot| snapshot.parse_errors.len())
        .sum();
    if failures > 0 {
        eprintln!("Warning: {failures} file/snapshot parse failures, see parse_errors in JSON");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        Arguments, FileEntry, Stop, aggregate, analysis::analyze, history_offsets, module_base,
        normalize, offsets,
    };
    use clap::Parser;
    use std::{collections::HashMap, path::Path};

    #[test]
    fn samples_requested_depth_and_stop_count() {
        assert_eq!(
            offsets(100, 10, Some(10)).unwrap(),
            (10..=100).step_by(10).collect::<Vec<_>>()
        );
        assert_eq!(offsets(101, 10, Some(10)).unwrap().last(), Some(&101));
        assert_eq!(offsets(25, 10, None).unwrap(), vec![10, 20, 25]);
        assert!(offsets(0, 10, None).is_err());
        assert!(offsets(100, 0, None).is_err());
        assert!(offsets(5, 10, Some(10)).is_err());
    }

    #[test]
    fn accepts_all_history_and_clamps_sampling() {
        let arguments =
            Arguments::try_parse_from(["coogles", ".", "--all", "--stops", "10"]).unwrap();
        assert!(arguments.all);
        assert_eq!(arguments.stops, Some(10));
        assert!(Arguments::try_parse_from(["coogles", ".", "--all", "--commits", "100"]).is_err());
        assert_eq!(
            history_offsets(2238, 10, Some(10)).unwrap().last(),
            Some(&2238)
        );
        assert_eq!(history_offsets(3, 10, Some(10)).unwrap(), vec![1, 2, 3]);
        assert_eq!(history_offsets(0, 10, Some(10)).unwrap(), vec![0]);
    }

    #[test]
    fn classifies_custom_targets_and_external_modules_per_path() {
        let sources = [
            (
                "src/lib.rs",
                "#[cfg(test)]\n#[path = \"checks.rs\"]\nmod checks;\nfn production() {}\n",
            ),
            ("src/checks.rs", "mod helpers;\nfn helper() {}\n"),
            ("src/checks/helpers.rs", "fn shared() {}\n"),
            ("src/production.rs", "fn shared() {}\n"),
            ("demo/start.rs", "mod helpers;\nfn main() {}\n"),
            ("demo/start/helpers.rs", "fn example_helper() {}\n"),
            ("qa/check.rs", "fn custom_test() {}\n"),
        ];
        let mut analyses = HashMap::new();
        let mut files = Vec::new();
        for (index, (path, source)) in sources.iter().enumerate() {
            let identifier = if index == 3 { 2 } else { index };
            let object_id =
                gix::ObjectId::from_hex(format!("{identifier:040x}").as_bytes()).unwrap();
            analyses.insert(object_id, analyze(source));
            files.push(FileEntry {
                path: path.to_string(),
                object_id,
            });
        }
        let manifest_id =
            gix::ObjectId::from_hex(b"ffffffffffffffffffffffffffffffffffffffff").unwrap();
        files.push(FileEntry {
            path: "Cargo.toml".to_owned(),
            object_id: manifest_id,
        });
        let manifest = toml::from_str::<toml::Value>("[package]\nname = 'fixture'\nversion = '0.1.0'\n[[test]]\nname = 'check'\npath = 'qa/check.rs'\n[[example]]\nname = 'demo'\npath = 'demo/start.rs'\n").unwrap();
        let manifests = HashMap::from([(manifest_id, manifest)]);
        let stop = Stop {
            offset: 10,
            commit: "fixture".to_owned(),
            timestamp: 0,
            files,
        };
        let snapshot = aggregate(&stop, &analyses, &manifests).unwrap();
        assert_eq!(snapshot.metrics.test_code_loc, 7);
        assert_eq!(snapshot.metrics.example_code_loc, 3);
        assert_eq!(snapshot.metrics.rust_files, 7);
        assert!(snapshot.parse_errors.is_empty());
    }

    #[test]
    fn resolves_module_and_target_paths() {
        assert_eq!(module_base("src/lib.rs"), Path::new("src"));
        assert_eq!(module_base("src/widget.rs"), Path::new("src/widget"));
        assert_eq!(
            normalize(Path::new("crates/foo/../tests/check.rs")).unwrap(),
            "crates/tests/check.rs"
        );
        assert!(normalize(Path::new("../outside.rs")).is_err());
    }
}
