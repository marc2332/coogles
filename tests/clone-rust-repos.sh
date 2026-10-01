#!/usr/bin/env bash
set -euo pipefail

project_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf -- "$temporary_directory"' EXIT
mkdir -p "$temporary_directory/bin" "$temporary_directory/workspace" "$temporary_directory/mock"
cp "$project_directory/clone-rust-repos.sh" "$temporary_directory/workspace/clone-rust-repos.sh"
export MOCK_DIRECTORY="$temporary_directory/mock"
export PATH="$temporary_directory/bin:$PATH"

cat > "$temporary_directory/bin/gh" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$MOCK_DIRECTORY/gh-arguments"
if [[ "${FAIL_GH:-0}" == 1 ]]; then exit 1; fi
if [[ "$1" == auth ]]; then exit 0; fi
[[ "$1 $2" == 'search repos' ]]
[[ "$*" == *'--language Rust'* && "$*" == *'--sort stars'* && "$*" == *'--visibility public'* ]]
if [[ "${DUPLICATE_RESULTS:-0}" == 1 ]]; then
    printf '%s\n' '[{"fullName":"alpha/first","stargazersCount":12},{"fullName":"alpha/first","stargazersCount":12},{"fullName":"gamma/third","stargazersCount":3}]'
else
    printf '%s\n' '[{"fullName":"alpha/first","stargazersCount":12},{"fullName":"beta/second","stargazersCount":45},{"fullName":"gamma/third","stargazersCount":3}]'
fi
MOCK

cat > "$temporary_directory/bin/git" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == -C ]]; then
    repository="$2"
    shift 2
    [[ -f "$repository/.mock-origin" ]] || exit 1
    case "$*" in
        'rev-parse --is-bare-repository') printf '%s\n' true ;;
        'rev-parse --is-shallow-repository') printf '%s\n' false ;;
        'remote get-url origin') head -n 1 "$repository/.mock-origin" ;;
        'config --get remote.origin.promisor') exit 1 ;;
        *) exit 1 ;;
    esac
    exit 0
fi
[[ "$1 $2 $3" == '-c protocol.version=2 clone' ]]
shift 3
bare=false
single_branch=false
no_tags=false
progress=false
while [[ "$1" != -- ]]; do
    case "$1" in
        --bare) bare=true ;;
        --single-branch) single_branch=true ;;
        --no-tags) no_tags=true ;;
        --progress) progress=true ;;
        *) printf 'Unexpected clone option: %s\n' "$1" >&2; exit 1 ;;
    esac
    shift
done
[[ "$bare" == true && "$single_branch" == true && "$no_tags" == true && "$progress" == true ]]
shift
clone_url="$1"
destination="$2"
[[ "${GIT_TERMINAL_PROMPT:-}" == 0 ]]
printf '%s\n' "$clone_url" >> "$MOCK_DIRECTORY/clones"
if [[ -n "${FAIL_REPOSITORY:-}" && "$clone_url" == "https://github.com/$FAIL_REPOSITORY.git" ]]; then exit 1; fi
printf '%s\n' "$clone_url" > "$destination/.mock-origin"
MOCK

printf '#!/usr/bin/env bash\nexit 0\n' > "$temporary_directory/bin/sleep"
chmod +x "$temporary_directory/bin/gh" "$temporary_directory/bin/git" "$temporary_directory/bin/sleep"

script="$temporary_directory/workspace/clone-rust-repos.sh"
bash "$script" --limit 3 --jobs 2 > "$temporary_directory/success.log" 2>&1
for full_name in alpha/first beta/second gamma/third; do
    [[ -f "$temporary_directory/workspace/data/$full_name/.mock-origin" ]]
done
jq -e 'map(.fullName) == ["beta/second", "alpha/first", "gamma/third"]' "$temporary_directory/workspace/data/repositories.json" >/dev/null
[[ "$(wc -l < "$MOCK_DIRECTORY/clones")" -eq 3 ]]

cp "$temporary_directory/workspace/data/repositories.json" "$temporary_directory/cached.json"
FAIL_GH=1 bash "$script" --limit 3 --jobs 2 > "$temporary_directory/resume.log" 2>&1
[[ "$(wc -l < "$MOCK_DIRECTORY/clones")" -eq 3 ]]
[[ "$(wc -l < "$MOCK_DIRECTORY/gh-arguments")" -eq 1 ]]
[[ "$(grep -c '^SKIP ' "$temporary_directory/resume.log")" -eq 3 ]]
FAIL_GH=1 bash "$script" --limit 1 > "$temporary_directory/smaller.log" 2>&1
[[ "$(grep -c '^SKIP ' "$temporary_directory/smaller.log")" -eq 1 ]]
cmp "$temporary_directory/cached.json" "$temporary_directory/workspace/data/repositories.json"
if FAIL_GH=1 bash "$script" --limit 4 > "$temporary_directory/insufficient.log" 2>&1; then
    printf '%s\n' 'Expected an insufficient cache to fail without refetching' >&2
    exit 1
fi
[[ "$(wc -l < "$MOCK_DIRECTORY/gh-arguments")" -eq 1 ]]
printf '{broken\n' > "$temporary_directory/workspace/data/repositories.json"
if FAIL_GH=1 bash "$script" --limit 3 > "$temporary_directory/corrupt.log" 2>&1; then
    printf '%s\n' 'Expected an invalid cache to fail without refetching' >&2
    exit 1
fi
[[ "$(wc -l < "$MOCK_DIRECTORY/gh-arguments")" -eq 1 ]]
cp "$temporary_directory/cached.json" "$temporary_directory/workspace/data/repositories.json"

mkdir -p "$temporary_directory/failure"
cp "$project_directory/clone-rust-repos.sh" "$temporary_directory/failure/clone-rust-repos.sh"
if FAIL_REPOSITORY=beta/second bash "$temporary_directory/failure/clone-rust-repos.sh" --limit 3 --jobs 2 > "$temporary_directory/failure.log" 2>&1; then
    printf '%s\n' 'Expected a failed clone to fail the script' >&2
    exit 1
fi
[[ -f "$temporary_directory/failure/data/alpha/first/.mock-origin" ]]
[[ -f "$temporary_directory/failure/data/gamma/third/.mock-origin" ]]
[[ ! -e "$temporary_directory/failure/data/beta/second" ]]
[[ -z "$(find "$temporary_directory/failure/data" -maxdepth 1 -name '.clone.*' -print)" ]]

mkdir -p "$temporary_directory/collision/data/alpha/first"
printf '%s\n' 'keep me' > "$temporary_directory/collision/data/alpha/first/keep"
cp "$project_directory/clone-rust-repos.sh" "$temporary_directory/collision/clone-rust-repos.sh"
if bash "$temporary_directory/collision/clone-rust-repos.sh" --limit 3 --jobs 2 > "$temporary_directory/collision.log" 2>&1; then
    printf '%s\n' 'Expected an existing non-Git directory to be rejected' >&2
    exit 1
fi
[[ -f "$temporary_directory/collision/data/alpha/first/keep" ]]

mkdir -p "$temporary_directory/duplicate"
cp "$project_directory/clone-rust-repos.sh" "$temporary_directory/duplicate/clone-rust-repos.sh"
if DUPLICATE_RESULTS=1 bash "$temporary_directory/duplicate/clone-rust-repos.sh" --limit 3 > "$temporary_directory/duplicate.log" 2>&1; then
    printf '%s\n' 'Expected duplicate search results to be rejected' >&2
    exit 1
fi
if bash "$script" --jobs 0 > /dev/null 2>&1; then exit 1; fi
if bash "$script" --limit 1001 > /dev/null 2>&1; then exit 1; fi
[[ -z "$(find "$temporary_directory/workspace/data" -maxdepth 1 -name '.search.*' -print)" ]]
printf '%s\n' 'Clone script checks passed: ranking, progress flags, cache reuse, resume, failure cleanup, collisions and validation'
