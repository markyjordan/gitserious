#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
python3 - "$root" <<'PY'
import pathlib
import re
import sys
for path in (pathlib.Path(sys.argv[1]) / '.github/workflows').glob('*.yml'):
    text = path.read_text()
    if re.search(r'^        run:', text, re.M):
        assert 'defaults:\n  run:\n    shell: bash\n' in text, path
    for shell in re.findall(r'^        shell: (.+)$', text, re.M):
        assert shell == 'bash', (path, shell)
    for match in re.finditer(r'^        run: (.*)\n(.*)', text, re.M):
        assert match[1] == '|' and match[2] == '          set -euo pipefail', path
    for step in re.split(r'(?m)^      - ', text)[1:]:
        if 'uses: actions/checkout@' in step:
            assert re.search(r'^          fetch-depth: [012]$', step, re.M), path
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if re.match(r'^\s+(contents|issues|pull-requests|statuses|attestations|id-token): (read|write)\b', line):
            assert '# why:' in line, (path, line)
    if path.name == 'trusted-automation-review.yml':
        assert 'statuses: write' not in text.split('jobs:', 1)[0]
        assert '      statuses: write' in text
PY
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin"
cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$LOG"
if [[ "$SCENARIO" == manifest && "$3" == */release-manifest.json ]]; then exit 31; fi
if [[ "$SCENARIO" == artifact && "$3" == */first ]]; then exit 32; fi
SH
cat >"$fixture/bin/jq" <<'SH'
#!/usr/bin/env bash
[[ "$SCENARIO" != jq ]] || exit 33
printf 'first\nsecond\n'
SH
chmod +x "$fixture/bin/"*
body="$(python3 - "$root/.github/workflows/update-homebrew-tap.yml" <<'PY'
import pathlib
import sys
text = pathlib.Path(sys.argv[1]).read_text()
step = text.split('      - name: Verify GitHub provenance\n', 1)[1].split('      - name:', 1)[0]
print('\n'.join(line[10:] for line in step.split('        run: |\n', 1)[1].splitlines() if line.startswith('          ')))
PY
)"
for scenario in manifest jq artifact success; do
  : >"$fixture/log"
  if env PATH="$fixture/bin:$PATH" SOURCE_REPOSITORY=fixture/repo \
    SCENARIO="$scenario" LOG="$fixture/log" bash -c "$body"$'\necho downstream >>"$LOG"'; then
    [[ "$scenario" == success ]]
    grep -q downstream "$fixture/log"
  else
    [[ "$scenario" != success ]]
    if grep -q downstream "$fixture/log"; then exit 1; fi
  fi
  if [[ "$scenario" == artifact ]] && grep -q '/second' "$fixture/log"; then exit 1; fi
done
echo 'Strict workflow and provenance failure fixtures passed.'
