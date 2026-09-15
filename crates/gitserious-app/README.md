# gitserious-app

This crate owns taxonomy policy, application workflows, persistence ports, and
semantic fingerprinting. Its Rust API is internal and unstable; the supported
interface is the `gitserious` CLI.

## Taxonomy policy

A taxonomy is the complete selectable authoring aggregate: identity, semantic
version, description, optional fork lineage, ordered commit types, and each
type's ordered durable properties. Built-ins are immutable. Custom taxonomies
live only in the global user library. A fork is a version-one snapshot with
informational source identity and version.

Global configuration contains the default taxonomy, ordered available set, and
custom library. Project configuration contains optional default and available
overrides. Each project field falls back independently to the corresponding
global field, and the effective default must be available.

## Portable project locks

Project locks contain the complete ordered taxonomy definitions used for commit
authoring. Commit creation validates only the authored project fingerprint and
then uses the pinned lock, so another machine does not need the originating
global custom taxonomy. Later global edits appear as an available project-policy
update and require reviewed refresh; they do not silently alter commits.

Configuration sessions stage one persistence destination at a time. Global
changes use whole-snapshot compare-and-swap. Project apply and initialization
write `gitserious.toml` and `gitserious.lock` through the project-state port.
Review is required before either adapter is called, and save failures retain the
draft.
