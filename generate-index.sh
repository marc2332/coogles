#!/usr/bin/env bash
set -euo pipefail

project_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$project_directory/reports" "$project_directory/data/repositories.json" <<'PYTHON'
import html
import json
import sys
from pathlib import Path
from urllib.parse import quote

reports = Path(sys.argv[1])
reports.mkdir(parents=True, exist_ok=True)
catalog = Path(sys.argv[2])
stars = {}
if catalog.exists():
    stars = {repository['fullName'].casefold(): repository['stargazersCount'] for repository in json.loads(catalog.read_text(encoding='utf-8'))}
entries = []
for report in reports.rglob('report.html'):
    relative = report.relative_to(reports)
    if any(part.startswith('.') for part in relative.parts):
        continue
    name = relative.parent.as_posix()
    entries.append((name, quote(relative.as_posix(), safe='/')))
entries.sort(key=lambda entry: (-stars.get(entry[0].casefold(), -1), entry[0].casefold()))
links = '\n'.join(
    f'<li><a href="{html.escape(url, quote=True)}">{html.escape(name)}</a> <span>{stars[name.casefold()]:,} stars</span></li>'
    if name.casefold() in stars else
    f'<li><a href="{html.escape(url, quote=True)}">{html.escape(name)}</a> <span>Stars unavailable</span></li>'
    for name, url in entries
)
page = f'''<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Coogles reports</title>
<style>
:root {{ color-scheme: dark; font-family: system-ui, sans-serif; background: #11151d; color: #e8edf5 }}
body {{ max-width: 900px; margin: 32px auto; padding: 0 20px }}
a {{ color: #69c7ff }}
li {{ margin: 12px 0 }}
span {{ color: #aebcd0; margin-left: 8px }}
</style>
<h1><a href="https://github.com/marc2332/coogles">Coogles</a> reports</h1>
<p>{len(entries)} repositories</p>
<ol>
{links}
</ol>
</html>
'''
destination = reports / 'index.html'
destination.write_text(page, encoding='utf-8')
print(f'Generated {destination} with {len(entries)} repository links')
PYTHON
