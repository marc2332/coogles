#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf '%s\n' \
        'Prepare HTML reports in a local gh-pages worktree.' \
        'Usage: ./prepare-github-pages.sh [--worktree PATH]' \
        'The default worktree is ./.gh-pages.' \
        'This script does not commit or push.'
}

PAGES_WORKTREE=''
while (($# > 0)); do
    case "$1" in
        --worktree)
            if (($# < 2)); then printf '%s\n' 'Missing value for --worktree' >&2; exit 1; fi
            PAGES_WORKTREE="$2"
            shift 2
            ;;
        --help|-h) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; usage >&2; exit 1 ;;
    esac
done

for dependency in git rsync python3; do
    if ! command -v "$dependency" >/dev/null 2>&1; then
        printf 'Missing dependency: %s\n' "$dependency" >&2
        exit 1
    fi
done

PROJECT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPORTS_DIRECTORY="$PROJECT_DIRECTORY/reports"
if [[ -z "$PAGES_WORKTREE" ]]; then PAGES_WORKTREE="$PROJECT_DIRECTORY/.gh-pages"; fi
if [[ "$PAGES_WORKTREE" != /* ]]; then PAGES_WORKTREE="$PROJECT_DIRECTORY/$PAGES_WORKTREE"; fi
if [[ "$PAGES_WORKTREE" == "$PROJECT_DIRECTORY" || "$PAGES_WORKTREE" == "$REPORTS_DIRECTORY" ]]; then
    printf 'Unsafe Pages worktree path: %s\n' "$PAGES_WORKTREE" >&2
    exit 1
fi
if [[ ! -x "$PROJECT_DIRECTORY/generate-index.sh" ]]; then
    printf '%s\n' 'generate-index.sh is missing or not executable' >&2
    exit 1
fi

"$PROJECT_DIRECTORY/generate-index.sh"
if [[ ! -s "$REPORTS_DIRECTORY/index.html" ]]; then
    printf '%s\n' 'reports/index.html was not generated' >&2
    exit 1
fi

staging_directory="$(mktemp -d)"
trap 'rm -rf -- "$staging_directory"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
rsync -a --prune-empty-dirs \
    --include='*/' --include='index.html' --include='report.html' --exclude='*' \
    "$REPORTS_DIRECTORY/" "$staging_directory/"
touch "$staging_directory/.nojekyll"

python3 - "$staging_directory" <<'PYTHON'
import html.parser
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse

root = Path(sys.argv[1])
class Links(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = []
    def handle_starttag(self, tag, attributes):
        if tag == 'a':
            url = dict(attributes).get('href')
            if url:
                self.urls.append(url)

parser = Links()
parser.feed((root / 'index.html').read_text(encoding='utf-8'))
missing = []
for url in parser.urls:
    parsed = urlparse(url)
    if parsed.scheme or parsed.netloc or url.startswith('#'):
        continue
    target = root / unquote(parsed.path)
    if not target.is_file():
        missing.append(url)
if missing:
    raise SystemExit('Index contains missing report links:\n' + '\n'.join(missing[:20]))
PYTHON

if [[ -e "$PAGES_WORKTREE" || -L "$PAGES_WORKTREE" ]]; then
    if [[ ! -d "$PAGES_WORKTREE" || "$(git -C "$PAGES_WORKTREE" branch --show-current 2>/dev/null || true)" != gh-pages ]]; then
        printf 'Refusing existing path that is not a gh-pages worktree: %s\n' "$PAGES_WORKTREE" >&2
        exit 1
    fi
else
    if git -C "$PROJECT_DIRECTORY" show-ref --verify --quiet refs/heads/gh-pages; then
        git -C "$PROJECT_DIRECTORY" worktree add "$PAGES_WORKTREE" gh-pages
    elif git -C "$PROJECT_DIRECTORY" show-ref --verify --quiet refs/remotes/origin/gh-pages; then
        git -C "$PROJECT_DIRECTORY" worktree add -b gh-pages "$PAGES_WORKTREE" origin/gh-pages
    else
        git -C "$PROJECT_DIRECTORY" worktree add --orphan -b gh-pages "$PAGES_WORKTREE"
    fi
fi

rsync -a --delete --exclude='.git' "$staging_directory/" "$PAGES_WORKTREE/"
report_count="$(find "$PAGES_WORKTREE" -type f -name report.html | wc -l)"
printf 'Prepared %s report pages in %s\n' "$report_count" "$PAGES_WORKTREE"
printf '%s\n' 'Review the worktree, then commit and push the gh-pages branch manually.'
printf '  git -C %q status --short\n' "$PAGES_WORKTREE"
printf '  git -C %q add -A\n' "$PAGES_WORKTREE"
