#!/usr/bin/env python3
import json
import sys


with open(sys.argv[1], encoding="utf-8") as handle:
    metadata = json.load(handle)

workspace = set(metadata.get("workspace_members", []))
packages = {}
versions = {}
for package in metadata.get("packages", []):
    package_id = package["id"]
    if package_id not in workspace or package.get("source") is not None:
        continue
    if package.get("publish") == []:
        continue
    packages[package_id] = package["name"]
    versions[package_id] = package["version"]

dependencies = {package_id: set() for package_id in packages}
for node in metadata.get("resolve", {}).get("nodes", []):
    node_id = node["id"]
    if node_id not in packages:
        continue
    for dependency in node.get("dependencies", []):
        if dependency in packages:
            dependencies[node_id].add(dependency)
    for dependency in node.get("deps", []):
        dependency_id = dependency.get("pkg")
        if dependency_id in packages:
            dependencies[node_id].add(dependency_id)

ordered = []
temporary = set()
permanent = set()


def visit(package_id):
    if package_id in permanent:
        return
    if package_id in temporary:
        raise SystemExit("dependency cycle in publishable workspace packages")
    temporary.add(package_id)
    for dependency_id in sorted(
        dependencies[package_id], key=lambda item: packages[item]
    ):
        visit(dependency_id)
    temporary.remove(package_id)
    permanent.add(package_id)
    ordered.append(package_id)


for package_id in sorted(packages, key=lambda item: packages[item]):
    visit(package_id)

for package_id in ordered:
    print(f"{packages[package_id]}\t{versions[package_id]}")
