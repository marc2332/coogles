#!/usr/bin/env bash
set -euo pipefail

project_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf -- "$temporary_directory"' EXIT
mkdir -p "$temporary_directory/bin" "$temporary_directory/workspace/data" "$temporary_directory/mock"
cp "$project_directory/generate-reports.sh" "$temporary_directory/workspace/generate-reports.sh"
export MOCK_DIRECTORY="$temporary_directory/mock"
export PATH="$temporary_directory/bin:$PATH"

create_bare_repository() {
    mkdir -p "$1/objects" "$1/refs"
    printf '%s\n' 'ref: refs/heads/main' > "$1/HEAD"
    printf '%s\n' '[core]' 'bare = true' > "$1/config"
}
create_bare_repository "$temporary_directory/workspace/data/first"
create_bare_repository "$temporary_directory/workspace/data/owner/second"
create_bare_repository "$temporary_directory/workspace/data/.clone.active"
mkdir -p "$temporary_directory/workspace/data/with spaces/.git"
printf '%s\n' '[]' > "$temporary_directory/workspace/data/repositories.json"

cat > "$temporary_directory/bin/git" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == -C && "$3 $4" == 'rev-parse --git-dir' ]]
repository="$2"
if [[ -e "$repository/.git" || (-f "$repository/HEAD" && -f "$repository/config" && -d "$repository/objects") ]]; then
    printf '%s\n' "$repository"
else
    exit 1
fi
MOCK

cat > "$temporary_directory/bin/coogles" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
repository="$1"
shift
all=false
stops=''
threads=''
output=''
html=''
while (($# > 0)); do
    case "$1" in
        --all) all=true; shift ;;
        --stops) stops="$2"; shift 2 ;;
        --threads) threads="$2"; shift 2 ;;
        --output) output="$2"; shift 2 ;;
        --html) html="$2"; shift 2 ;;
        *) printf 'Unexpected option: %s\n' "$1" >&2; exit 1 ;;
    esac
done
[[ "$all" == true && -n "$stops" && -n "$threads" && -n "$output" && -n "$html" ]]
if [[ -n "${FAIL_REPOSITORY:-}" && "$repository" == *"/$FAIL_REPOSITORY" ]]; then
    printf '%s\n' 'partial output' > "$output"
    exit 1
fi
jq -n --arg repository "$repository" --arg stops "$stops" --arg threads "$threads" \
    '{repository:$repository, all:true, stops:$stops, threads:$threads}' > "$output"
printf '<html>%s</html>\n' "$repository" > "$html"
MOCK

cat > "$temporary_directory/bin/cargo" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == build && "$*" == *'--release'* && "$*" == *'--bin coogles'* ]]
printf '%s\n' "$*" >> "$MOCK_DIRECTORY/builds"
jq -n -c --arg binary "$COOGLES_MOCK_BINARY" \
    '{reason:"compiler-artifact",target:{name:"coogles"},executable:$binary}'
printf '%s\n' '{"reason":"build-finished","success":true}'
MOCK
chmod +x "$temporary_directory/bin/git" "$temporary_directory/bin/coogles" "$temporary_directory/bin/cargo"
export COOGLES_MOCK_BINARY="$temporary_directory/bin/coogles"
script="$temporary_directory/workspace/generate-reports.sh"

bash "$script" --workers 2 --stops 17 --threads 2 --binary "$COOGLES_MOCK_BINARY" > "$temporary_directory/success.log" 2>&1
for relative_name in first owner/second 'with spaces'; do
    directory="$temporary_directory/workspace/reports/$relative_name"
    [[ -s "$directory/report.html" && -s "$directory/report.json" ]]
    jq -e '.all == true and .stops == "17" and .threads == "2"' "$directory/report.json" >/dev/null
done
[[ ! -e "$temporary_directory/workspace/reports/.clone.active" ]]
[[ "$(grep -c '^DONE ' "$temporary_directory/success.log")" -eq 3 ]]
for repository_index in 1 2 3; do
    grep -q "^REPORT \[$repository_index/3\] " "$temporary_directory/success.log"
    grep -q "^DONE \[$repository_index/3\] " "$temporary_directory/success.log"
done
[[ -z "$(find "$temporary_directory/workspace/reports" -maxdepth 1 -name '.report.*' -print)" ]]

bash "$script" --workers 2 --stops 3 > "$temporary_directory/build.log" 2>&1
[[ "$(wc -l < "$MOCK_DIRECTORY/builds")" -eq 1 ]]
jq -e '.stops == "3" and .threads == "1"' "$temporary_directory/workspace/reports/first/report.json" >/dev/null

printf '%s\n' 'keep previous html' > "$temporary_directory/workspace/reports/first/report.html"
printf '%s\n' 'keep previous json' > "$temporary_directory/workspace/reports/first/report.json"
if FAIL_REPOSITORY=first bash "$script" --workers 2 --stops 9 --binary coogles > "$temporary_directory/failure.log" 2>&1; then
    printf '%s\n' 'Expected a failed repository analysis to fail the script' >&2
    exit 1
fi
grep -q '^FAILED \[[1-3]/3\] first$' "$temporary_directory/failure.log"
grep -q '^keep previous html$' "$temporary_directory/workspace/reports/first/report.html"
grep -q '^keep previous json$' "$temporary_directory/workspace/reports/first/report.json"
jq -e '.stops == "9"' "$temporary_directory/workspace/reports/owner/second/report.json" >/dev/null
[[ -z "$(find "$temporary_directory/workspace/reports" -maxdepth 1 -name '.report.*' -print)" ]]
[[ -z "$(find "$temporary_directory/workspace/reports" -maxdepth 1 -name '.batch.*' -print)" ]]

mkdir -p "$temporary_directory/empty/data"
cp "$project_directory/generate-reports.sh" "$temporary_directory/empty/generate-reports.sh"
if bash "$temporary_directory/empty/generate-reports.sh" --binary coogles > "$temporary_directory/empty.log" 2>&1; then
    printf '%s\n' 'Expected an empty data folder to be rejected' >&2
    exit 1
fi
if bash "$script" --workers 0 > /dev/null 2>&1; then exit 1; fi
if bash "$script" --stops 0 > /dev/null 2>&1; then exit 1; fi
if bash "$script" --threads 33 > /dev/null 2>&1; then exit 1; fi
printf '%s\n' 'Report script checks passed: flat and nested discovery, spaces, full history, workers, stops, threads, one build, failure isolation and cleanup'
