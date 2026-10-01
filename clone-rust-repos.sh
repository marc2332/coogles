#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf '%s\n' \
        'Clone the top public, non-fork Rust repositories ranked by GitHub stars.' \
        'Usage: ./clone-rust-repos.sh [--jobs N] [--limit N]' \
        '  --jobs N   Concurrent clones, default 4, maximum 32' \
        '  --limit N  Repository count, default 1000, maximum 1000' \
        'Requires Git, GitHub CLI, jq and xargs. Authenticate with gh auth login.'
}

JOBS=4
REPOSITORY_COUNT=1000
while (($# > 0)); do
    case "$1" in
        --jobs|--limit)
            if (($# < 2)); then
                printf 'Missing value for %s\n' "$1" >&2
                exit 1
            fi
            if [[ "$1" == --jobs ]]; then JOBS="$2"; else REPOSITORY_COUNT="$2"; fi
            shift 2
            ;;
        --help|-h) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; usage >&2; exit 1 ;;
    esac
done

if [[ ! "$JOBS" =~ ^[1-9][0-9]?$ ]] || ((JOBS > 32)); then
    printf '%s\n' '--jobs must be between 1 and 32' >&2
    exit 1
fi
if [[ ! "$REPOSITORY_COUNT" =~ ^[1-9][0-9]{0,3}$ ]] || ((REPOSITORY_COUNT > 1000)); then
    printf '%s\n' '--limit must be between 1 and 1000' >&2
    exit 1
fi
for dependency in git jq xargs; do
    if ! command -v "$dependency" >/dev/null 2>&1; then
        printf 'Missing dependency: %s\n' "$dependency" >&2
        exit 1
    fi
done

PROJECT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export DATA_DIRECTORY="$PROJECT_DIRECTORY/data"
export GIT_TERMINAL_PROMPT=0
mkdir -p -- "$DATA_DIRECTORY"
search_directory="$(mktemp -d "$DATA_DIRECTORY/.search.XXXXXX")"
trap 'rm -rf -- "$search_directory"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

repositories_json="$DATA_DIRECTORY/repositories.json"
if [[ -e "$repositories_json" || -L "$repositories_json" ]]; then
    if [[ ! -f "$repositories_json" || ! -r "$repositories_json" ]]; then
        printf 'Cannot read repository cache: %s\n' "$repositories_json" >&2
        exit 1
    fi
    printf 'Using cached repositories from %s\n' "$repositories_json"
    source_json="$repositories_json"
else
    if ! command -v gh >/dev/null 2>&1; then
        printf '%s\n' 'Missing dependency: gh, required to fetch the repository list' >&2
        exit 1
    fi
    printf 'Fetching the top %s Rust repositories by stars...\n' "$REPOSITORY_COUNT"
    if ! GH_HOST=github.com gh search repos \
        --language Rust --visibility public --include-forks=false \
        --sort stars --order desc --limit "$REPOSITORY_COUNT" \
        --json fullName,stargazersCount,url > "$search_directory/raw.json"; then
        printf '%s\n' 'GitHub search failed. Check authentication and API rate limits.' >&2
        GH_HOST=github.com gh auth status --hostname github.com >&2 || true
        exit 1
    fi
    source_json="$search_directory/raw.json"
fi

jq -e --argjson count "$REPOSITORY_COUNT" '
    if type == "array" and length >= $count
        and (unique_by(.fullName) | length) == length
        and all(.[];
            (.fullName | type) == "string"
            and (.fullName | test("^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+$"))
            and (.fullName | split("/") | all(. != "." and . != ".."))
            and (.stargazersCount | type) == "number"
            and .stargazersCount >= 0)
    then sort_by(-.stargazersCount, .fullName)
    else error("Repository list is invalid or too short. Remove data/repositories.json to refresh it, or use a smaller --limit.")
    end
' "$source_json" > "$search_directory/repositories.json"
if [[ "$source_json" != "$repositories_json" ]]; then
    mv -- "$search_directory/repositories.json" "$repositories_json"
    source_json="$repositories_json"
else
    source_json="$search_directory/repositories.json"
fi
jq -j --argjson count "$REPOSITORY_COUNT" '.[:$count][] | .fullName + "\u0000"' "$source_json" > "$search_directory/names"

clone_repository() (
    set -euo pipefail
    full_name="$1"
    destination="$DATA_DIRECTORY/$full_name"
    clone_url="https://github.com/$full_name.git"
    if [[ -e "$destination" || -L "$destination" ]]; then
        if [[ "$(git -C "$destination" rev-parse --is-bare-repository 2>/dev/null || true)" != true ]]; then
            printf 'REFUSED %s: destination exists and is not a bare clone\n' "$full_name" >&2
            exit 1
        fi
        origin="$(git -C "$destination" remote get-url origin)"
        if [[ "$origin" != "$clone_url" && "$origin" != "${clone_url%.git}" && "$origin" != "git@github.com:$full_name.git" ]]; then
            printf 'REFUSED %s: existing clone has a different origin\n' "$full_name" >&2
            exit 1
        fi
        if [[ "$(git -C "$destination" rev-parse --is-shallow-repository)" == true || "$(git -C "$destination" config --get remote.origin.promisor || true)" == true ]]; then
            printf 'REFUSED %s: existing clone is shallow or partial\n' "$full_name" >&2
            exit 1
        fi
        printf 'SKIP %s\n' "$full_name"
        exit 0
    fi
    mkdir -p -- "$(dirname -- "$destination")"
    temporary_directory=''
    trap 'if [[ -n "$temporary_directory" ]]; then rm -rf -- "$temporary_directory"; fi' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    for attempt in 1 2 3; do
        temporary_directory="$(mktemp -d "$DATA_DIRECTORY/.clone.XXXXXX")"
        printf 'CLONE %s (attempt %s/3)\n' "$full_name" "$attempt"
        if git -c protocol.version=2 clone --bare --single-branch --no-tags --progress -- "$clone_url" "$temporary_directory"; then
            if [[ -e "$destination" || -L "$destination" ]]; then
                printf 'REFUSED %s: destination appeared during cloning\n' "$full_name" >&2
                exit 1
            fi
            mv -- "$temporary_directory" "$destination"
            printf 'DONE %s\n' "$full_name"
            exit 0
        fi
        rm -rf -- "$temporary_directory"
        if ((attempt < 3)); then sleep "$((attempt * 2))"; fi
    done
    printf 'FAILED %s\n' "$full_name" >&2
    exit 1
)
export -f clone_repository

printf 'Cloning with %s workers. Full history and blobs may require substantial disk space.\n' "$JOBS"
if xargs -0 -n 1 -P "$JOBS" bash -c 'clone_repository "$1"' _ < "$search_directory/names"; then
    printf 'Ready: %s repositories in %s\n' "$REPOSITORY_COUNT" "$DATA_DIRECTORY"
else
    printf '%s\n' 'Some clones failed. Rerun to retry, successful clones are skipped.' >&2
    exit 1
fi
