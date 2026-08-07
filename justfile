set positional-arguments

# DEFAULT ENTRYPOINTS
# Lists all recipes so you can quickly discover available workflows.
default:
    @just --list

# PROJECT SETUP
# Prepares a contributor checkout for local verification.
bootstrap:
    bash scripts/dev/justfile/bootstrap.sh

# LOCAL DEVELOPMENT
# Builds the workspace with Cargo's debug profile for local testing.
build:
    bash scripts/dev/justfile/cargo.sh build

# Runs the gitserious binary with Cargo's debug profile. Arguments are forwarded to the binary.
run *args:
    bash scripts/dev/justfile/cargo.sh run "$@"

# LOCAL CI
# Runs the same language-neutral quality categories used by hosted CI.
ci-check:
    bash scripts/ci/run-quality.sh check

ci-fmt:
    bash scripts/ci/run-quality.sh fmt

ci-lint:
    bash scripts/ci/run-quality.sh lint

ci-test:
    bash scripts/ci/run-quality.sh test

ci:
    bash scripts/ci/check-merge-into-dev.sh

# Exercises portable CI policy and helper fixtures without running hosted-only jobs.
ci-fixtures:
    bash scripts/ci/tests/run.sh

# DOCS NAVIGATION
# Displays a subtree with repo-aware path resolution for quick structure checks. Use `just tree -h` for usage.
tree *args:
    @bash scripts/dev/justfile/tree.sh "$@"
