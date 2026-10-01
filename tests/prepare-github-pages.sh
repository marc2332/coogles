#!/usr/bin/env bash
set -euo pipefail

project_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf -- "$temporary_directory"' EXIT
repository="$temporary_directory/repository"
mkdir -p "$repository/reports/owner/first" "$repository/reports/owner/with space" "$repository/data"
cp "$project_directory/prepare-github-pages.sh" "$repository/prepare-github-pages.sh"
cp "$project_directory/generate-index.sh" "$repository/generate-index.sh"
chmod +x "$repository/prepare-github-pages.sh" "$repository/generate-index.sh"
printf '<html>first</html>\n' > "$repository/reports/owner/first/report.html"
printf '<html>space</html>\n' > "$repository/reports/owner/with space/report.html"
printf '{}\n' > "$repository/reports/owner/first/report.json"
printf '<html>development artifact</html>\n' > "$repository/reports/audit-before.html"
printf '%s\n' '[{"fullName":"owner/first","stargazersCount":10},{"fullName":"owner/with space","stargazersCount":5}]' > "$repository/data/repositories.json"
git -C "$repository" init -q
git -C "$repository" config user.email test@example.com
git -C "$repository" config user.name Test
git -C "$repository" config commit.gpgsign false
git -C "$repository" add prepare-github-pages.sh generate-index.sh
git -C "$repository" commit -qm initial

bash "$repository/prepare-github-pages.sh" > "$temporary_directory/first.log"
pages="$repository/.gh-pages"
[[ "$(git -C "$pages" branch --show-current)" == gh-pages ]]
[[ -f "$pages/index.html" && -f "$pages/.nojekyll" ]]
[[ -f "$pages/owner/first/report.html" && -f "$pages/owner/with space/report.html" ]]
[[ ! -e "$pages/owner/first/report.json" ]]
[[ ! -e "$pages/audit-before.html" ]]
grep -q 'href="owner/first/report.html"' "$pages/index.html"
grep -q 'href="owner/with%20space/report.html"' "$pages/index.html"
grep -q 'href="https://github.com/marc2332/coogles"' "$pages/index.html"
[[ -z "$(git -C "$repository" log --all --format=%s | grep -v '^initial$' || true)" ]]

rm "$repository/reports/owner/first/report.html"
mkdir -p "$repository/reports/owner/second"
printf '<html>second</html>\n' > "$repository/reports/owner/second/report.html"
bash "$repository/prepare-github-pages.sh" > "$temporary_directory/second.log"
[[ ! -e "$pages/owner/first/report.html" ]]
[[ -f "$pages/owner/second/report.html" ]]
[[ "$(git -C "$pages" branch --show-current)" == gh-pages ]]

mkdir -p "$temporary_directory/collision"
if bash "$repository/prepare-github-pages.sh" --worktree "$temporary_directory/collision" > "$temporary_directory/collision.log" 2>&1; then
    printf '%s\n' 'Expected a non-worktree collision to fail' >&2
    exit 1
fi
printf '%s\n' 'Pages script checks passed: orphan worktree, HTML-only copy, URL mapping, stale deletion, reuse and no commits'
