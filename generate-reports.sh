#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf '%s\n' \
        'Generate a separate full-history report for every repository under ./data.' \
        'Usage: ./generate-reports.sh [--workers N] [--stops N] [--threads N] [--binary PATH]' \
        '  --workers N  Concurrent repositories, default 4, maximum 32' \
        '  --stops N    Samples per repository, default 50' \
        '  --threads N  Threads per repository, default 1, maximum 32' \
        '  --binary PATH  Use an existing coogles binary instead of building this checkout' \
        'Supports data/REPO and data/OWNER/REPO layouts. Output mirrors those paths in ./reports.'
}

WORKERS=4
STOPS=50
THREADS=1
COOGLES_BINARY=''
while (($# > 0)); do
    case "$1" in
        --workers|--stops|--threads|--binary)
            if (($# < 2)); then
                printf 'Missing value for %s\n' "$1" >&2
                exit 1
            fi
            case "$1" in
                --workers) WORKERS="$2" ;;
                --stops) STOPS="$2" ;;
                --threads) THREADS="$2" ;;
                --binary) COOGLES_BINARY="$2" ;;
            esac
            shift 2
            ;;
        --help|-h) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; usage >&2; exit 1 ;;
    esac
done

for count in "$WORKERS" "$THREADS"; do
    if [[ ! "$count" =~ ^[1-9][0-9]?$ ]] || ((count > 32)); then
        printf '%s\n' '--workers and --threads must be between 1 and 32' >&2
        exit 1
    fi
done
if [[ ! "$STOPS" =~ ^[1-9][0-9]*$ || ${#STOPS} -gt 18 ]]; then
    printf '%s\n' '--stops must be a positive integer with at most 18 digits' >&2
    exit 1
fi
for dependency in git xargs; do
    if ! command -v "$dependency" >/dev/null 2>&1; then
        printf 'Missing dependency: %s\n' "$dependency" >&2
        exit 1
    fi
done

PROJECT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIRECTORY="$PROJECT_DIRECTORY/data"
export REPORTS_DIRECTORY="$PROJECT_DIRECTORY/reports"
if [[ ! -d "$DATA_DIRECTORY" ]]; then
    printf 'Repository folder does not exist: %s\n' "$DATA_DIRECTORY" >&2
    exit 1
fi
mkdir -p -- "$REPORTS_DIRECTORY"
batch_directory="$(mktemp -d "$REPORTS_DIRECTORY/.batch.XXXXXX")"
trap 'rm -rf -- "$batch_directory"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

find -H "$DATA_DIRECTORY" -mindepth 1 -maxdepth 2 \
    \( -name .git -o -name '.clone.*' -o -name '.search.*' \) -prune -o -type d -print0 \
    > "$batch_directory/directories"
repository_count=0
while IFS= read -r -d '' directory; do
    if [[ -e "$directory/.git" || (-f "$directory/HEAD" && -f "$directory/config" && -d "$directory/objects") ]]; then
        if git -C "$directory" rev-parse --git-dir >/dev/null 2>&1; then
            relative_name="${directory#"$DATA_DIRECTORY"/}"
            repository_count=$((repository_count + 1))
            printf '%s\0%s\0%s\0' "$directory" "$relative_name" "$repository_count" >> "$batch_directory/repositories"
        else
            printf 'Invalid Git repository: %s\n' "$directory" >&2
            exit 1
        fi
    fi
done < "$batch_directory/directories"
if ((repository_count == 0)); then
    printf 'No repositories found under %s\n' "$DATA_DIRECTORY" >&2
    exit 1
fi

if [[ -z "$COOGLES_BINARY" ]]; then
    for dependency in cargo jq; do
        if ! command -v "$dependency" >/dev/null 2>&1; then
            printf 'Missing dependency: %s, needed to build coogles. Alternatively use --binary PATH.\n' "$dependency" >&2
            exit 1
        fi
    done
    printf '%s\n' 'Building coogles once in release mode...'
    if ! COOGLES_BINARY="$(cargo build --release --locked --bin coogles \
        --manifest-path "$PROJECT_DIRECTORY/Cargo.toml" --message-format=json-render-diagnostics \
        | jq -r 'select(.reason == "compiler-artifact" and .target.name == "coogles" and .executable != null) | .executable')"; then
        printf '%s\n' 'Failed to build coogles' >&2
        exit 1
    fi
fi
if [[ "$COOGLES_BINARY" != */* ]]; then
    COOGLES_BINARY="$(command -v -- "$COOGLES_BINARY" || true)"
fi
if [[ ! -f "$COOGLES_BINARY" || ! -x "$COOGLES_BINARY" ]]; then
    printf 'Coogles binary is not executable: %s\n' "$COOGLES_BINARY" >&2
    exit 1
fi
COOGLES_BINARY="$(cd -- "$(dirname -- "$COOGLES_BINARY")" && pwd)/$(basename -- "$COOGLES_BINARY")"
export COOGLES_BINARY STOPS THREADS
export REPOSITORY_COUNT="$repository_count"

generate_report() (
    set -euo pipefail
    repository="$1"
    relative_name="$2"
    repository_index="$3"
    output_directory="$REPORTS_DIRECTORY/$relative_name"
    temporary_directory="$(mktemp -d "$REPORTS_DIRECTORY/.report.XXXXXX")"
    trap 'rm -rf -- "$temporary_directory"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    printf 'REPORT [%s/%s] %s\n' "$repository_index" "$REPOSITORY_COUNT" "$relative_name"
    if ! "$COOGLES_BINARY" "$repository" --all --stops "$STOPS" --threads "$THREADS" \
        --output "$temporary_directory/report.json" --html "$temporary_directory/report.html"; then
        printf 'FAILED [%s/%s] %s\n' "$repository_index" "$REPOSITORY_COUNT" "$relative_name" >&2
        exit 1
    fi
    if [[ ! -s "$temporary_directory/report.json" || ! -s "$temporary_directory/report.html" ]]; then
        printf 'FAILED [%s/%s] %s: coogles did not produce both report files\n' "$repository_index" "$REPOSITORY_COUNT" "$relative_name" >&2
        exit 1
    fi
    mkdir -p -- "$output_directory"
    mv -- "$temporary_directory/report.json" "$output_directory/report.json"
    mv -- "$temporary_directory/report.html" "$output_directory/report.html"
    printf 'DONE [%s/%s] %s/report.html\n' "$repository_index" "$REPOSITORY_COUNT" "$output_directory"
)
export -f generate_report

printf 'Reporting on %s repositories with %s workers, %s stops and %s threads per worker.\n' \
    "$repository_count" "$WORKERS" "$STOPS" "$THREADS"
if xargs -0 -n 3 -P "$WORKERS" bash -c 'generate_report "$1" "$2" "$3"' _ < "$batch_directory/repositories"; then
    printf 'Ready: %s reports in %s\n' "$repository_count" "$REPORTS_DIRECTORY"
else
    printf '%s\n' 'Some repositories failed. Reports for successful repositories were generated.' >&2
    exit 1
fi
