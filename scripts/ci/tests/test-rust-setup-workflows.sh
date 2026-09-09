#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
python3 - "$root" <<'PY'
import pathlib
import re
import sys

workflows = pathlib.Path(sys.argv[1]) / '.github/workflows'
expected = {
    'build-release-binaries.yml': 1, 'check-code.yml': 1,
    'check-fmt.yml': 1, 'check-lint.yml': 1, 'ci.yml': 1,
    'dependency-security.yml': 1, 'release-readiness.yml': 1,
    'release.yml': 2, 'test.yml': 1,
}
command = 'run: bash scripts/shared/setup-rust-toolchain.sh'
for name, count in expected.items():
    text = (workflows / name).read_text()
    steps = re.split(r'(?m)^      - ', text)[1:]
    setups = [step for step in steps if step.startswith('name: Set up ') and 'Rust toolchain' in step.splitlines()[0]]
    assert len(setups) == count, (name, 'missing setup blocks')
    for step in setups:
        assert '        shell: bash' in step, (name, 'setup must use Bash')
        run = next(line.strip() for line in step.splitlines() if line.strip().startswith('run:'))
        suffix = ' --target "$TARGET"' if name == 'build-release-binaries.yml' else ''
        assert run == command + suffix, (name, run)
    if name == 'build-release-binaries.yml':
        assert 'TARGET: ${{ matrix.target }}' in setups[0]
for path in workflows.glob('*.yml'):
    assert not re.search(r'\b(?:ubuntu|macos|windows)-latest\b', path.read_text()), path
    assert not re.search(r'rustup\s+(toolchain\s+install|target\s+add|show)', path.read_text()), path
print('Rust setup workflow contracts passed.')
PY
