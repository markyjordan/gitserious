# Workspace dependency baseline

This inventory records the six-crate workspace at `origin/dev`, `b8c03d1a46a56cced76bbdc760e76eb9437f52ea`. It is a declared dependency inventory, not a resolved target graph or a proof of semantic ownership.

## Workspace members

The workspace contains `gitserious`, `gitserious-app`, `gitserious-cli`, `gitserious-core`, `gitserious-fs`, and `gitserious-tui`. The executable is the default workspace member.

## Declared workspace edges

| From | To | Kind | Target | Optional |
|---|---|---|---|---|
| `gitserious` | `gitserious-app` | normal | all targets | false |
| `gitserious` | `gitserious-cli` | normal | all targets | false |
| `gitserious` | `gitserious-core` | normal | all targets | false |
| `gitserious` | `gitserious-fs` | normal | all targets | false |
| `gitserious` | `gitserious-tui` | normal | all targets | false |
| `gitserious-app` | `gitserious-core` | normal | all targets | false |
| `gitserious-cli` | `gitserious-app` | normal | all targets | false |
| `gitserious-cli` | `gitserious-core` | normal | all targets | false |
| `gitserious-fs` | `gitserious-app` | normal | all targets | false |
| `gitserious-fs` | `gitserious-core` | normal | all targets | false |
| `gitserious-tui` | `gitserious-app` | normal | all targets | false |
| `gitserious-tui` | `gitserious-core` | normal | all targets | false |

There are twelve workspace-to-workspace edges. All are normal, unconditional, nonoptional dependencies. No workspace member declares a development, build, conditional, or optional dependency on another workspace member at this baseline. External dependencies, including external development dependencies, are outside this table.

The table and the statement above also hold at `f6a082c`. Between the two commits, App's `test-support` feature added development dependencies on App from FS, TUI, and App itself. #469, #470, and #471 removed all three. `scripts/ci/check-crate-dependencies.py` checks each workspace edge against its `PERMITTED` table whatever the edge's kind. It rejects an App self edge, so the feature's self dev-dependency cannot return. It still allows a development dependency along a listed edge, such as FS on App.

Core has no dependency on another workspace member. App depends on Core and no adapter implementation. CLI and TUI do not depend on one another. FS depends on application contracts and Core. The executable depends on all five constituent crates for composition.

## Reproduce the table

Run this from a checkout at the recorded baseline with the pinned Rust toolchain and Python 3. Offline mode needs Cargo's already cached dependency metadata. The command does not update the lockfile.

```sh
python3 - <<'PYTHON'
import json, subprocess
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--offline", "--no-deps",
    "--format-version", "1",
]))
names = {package["name"] for package in metadata["packages"]}
print("| From | To | Kind | Target | Optional |")
print("|---|---|---|---|---|")
for package in sorted(metadata["packages"], key=lambda package: package["name"]):
    for dep in sorted(package["dependencies"], key=lambda dep: dep["name"]):
        if dep["name"] in names:
            print(f'| `{package["name"]}` | `{dep["name"]}` | '
                  f'{dep["kind"] or "normal"} | {dep["target"] or "all targets"} | '
                  f'{str(dep["optional"]).lower()} |')
PYTHON
```

`cargo metadata --no-deps` reports dependency declarations from package manifests. The command retains each declaration's kind, target condition, and optionality before formatting. The table does not imply that every declaration is compiled in every target. Use resolved metadata or Cargo's target-specific graph when investigating compiled dependencies.

## Ownership limits

Cargo edges preserve the intended hexagonal direction. They do not prove that each type belongs to its current layer. App currently defines Help layout and UI-launch contracts despite having no adapter dependency. Relative source inclusions do not appear as Cargo edges either. At `f6a082c`, the only two stay inside the executable crate. `tests/help.rs` includes `src/help.rs`, and `tests/terminal_help.rs` includes `tests/support/terminal.rs`. [Testing](testing.md) records which crate owns each test.

`scripts/ci/check-crate-dependencies.py` now enforces the permitted workspace edges, keeps presentation crates out of Core and App, and rejects a source inclusion that leaves its crate. `run-rust-quality.sh check` runs it on the Linux and macOS legs.

[TUI module ownership](tui-modules.md) records how `gitserious-tui` divides its own modules.
