set positional-arguments

# DEFAULT ENTRYPOINTS
# Lists all recipes so you can quickly discover available workflows.
default:
    @just --list

# PROJECT SETUP
# Prepares a contributor checkout for local verification.
bootstrap:
    bash scripts/dev/justfile/bootstrap.sh

# DOCS NAVIGATION
# Displays a subtree with repo-aware path resolution for quick structure checks. Use `just tree -h` for usage.
tree *args:
    @bash scripts/dev/justfile/tree.sh "$@"
