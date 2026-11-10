#!/usr/bin/env python3
"""Fail when a workspace crate depends on what the architecture forbids.

Three rules, from the dependency invariants in
docs/eng/architecture/workspace-baseline.md:

- A workspace crate depends on another workspace crate, in any kind or target,
  only along an edge in PERMITTED.
- Core and App depend on no terminal or command-line presentation crate.
- No source file compiles a file outside its own crate through a string-literal
  #[path], include!, include_str!, or include_bytes!. Cargo cannot see that
  edge, so the first rule would miss it.

Reads `cargo metadata` from the repository root. Prints one error per breach
and exits 1, or prints a summary and exits 0.
"""

import json
import os
import pathlib
import re
import subprocess
import sys

PERMITTED = {
    "gitserious": {
        "gitserious-app", "gitserious-cli", "gitserious-core", "gitserious-fs", "gitserious-tui",
    },
    # The self edge lets the App's integration tests enable its test-support feature.
    "gitserious-app": {"gitserious-app", "gitserious-core"},
    "gitserious-cli": {"gitserious-app", "gitserious-core"},
    "gitserious-core": set(),
    "gitserious-fs": {"gitserious-app", "gitserious-core"},
    "gitserious-tui": {"gitserious-app", "gitserious-core"},
}
PRESENTATION = {"clap", "crossterm", "ratatui", "tui-textarea"}
PRESENTATION_FREE = {"gitserious-app", "gitserious-core"}
INCLUSION = re.compile(
    r'#\[path\s*=\s*"([^"]+)"\]|\binclude(?:_str|_bytes)?!\(\s*"([^"]+)"\s*\)')


def metadata(cargo):
    return json.loads(subprocess.run(
        [cargo, "metadata", "--locked", "--no-deps", "--format-version", "1"],
        check=True, capture_output=True, text=True,
    ).stdout)


def dependency_errors(packages):
    names = {package["name"] for package in packages}
    errors = []
    for package in packages:
        name = package["name"]
        if name not in PERMITTED:
            errors.append(f"{name} is a workspace crate with no entry in PERMITTED; "
                          "add the edges the architecture allows it")
            continue
        for dependency in package["dependencies"]:
            target = dependency["name"]
            kind = dependency["kind"] or "normal"
            if target in names and target not in PERMITTED[name]:
                errors.append(f"{name} depends on {target} ({kind}), an edge the "
                              "architecture forbids; remove it or justify it in PERMITTED")
            if name in PRESENTATION_FREE and target in PRESENTATION:
                errors.append(f"{name} depends on {target} ({kind}), but core and app "
                              "own no presentation; move the code to an adapter")
    return errors


def inclusion_errors(root, packages):
    errors = []
    for package in packages:
        crate = pathlib.Path(package["manifest_path"]).parent.resolve()
        for source in sorted(crate.rglob("*.rs")):
            if "target" in source.relative_to(crate).parts:
                continue
            text = source.read_text(encoding="utf-8")
            for match in INCLUSION.finditer(text):
                included = (source.parent / (match.group(1) or match.group(2))).resolve()
                if crate not in included.parents:
                    line = text.count("\n", 0, match.start()) + 1
                    errors.append(
                        f"{source.relative_to(root).as_posix()}:{line} compiles "
                        f"{os.path.relpath(included, root)}, outside {package['name']}; "
                        "depend on the owning crate instead")
    return errors


def main():
    root = pathlib.Path(subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], check=True, capture_output=True, text=True,
    ).stdout.strip()).resolve()
    os.chdir(root)
    packages = metadata(os.environ.get("CARGO", "cargo"))["packages"]
    errors = dependency_errors(packages) + inclusion_errors(root, packages)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        sys.exit(1)
    print(f"Crate dependencies of {len(packages)} workspace crates follow the architecture.")


if __name__ == "__main__":
    main()
