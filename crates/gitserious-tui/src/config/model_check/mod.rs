//! Bounded exhaustive exploration of real Config events and rendered frames.
//!
//! Seeds cover blank creation, forks, existing fields, conditional policy, limits and copies.
//! Every transition checks modal editing, direct persistence, deletion confirmation, saved
//! eligibility badges, the new taxonomy row that ends the Library, name locking, field limits,
//! inline messages and copy independence. Esc restores the checkpoint taken before the
//! active edit and writes nothing. Ctrl+W finishes that edit only after storage succeeds.
//! At most one taxonomy root is expanded, while type expansion can be remembered independently.
//! The existing breadth-first runner deduplicates snapshots and reports shortest counterexamples.
//! Keys the root takes before Config still reach it here. Mouse, resize, write failures and the root are covered by interaction
//! tests.

mod diagnostics;
mod engine;
mod properties;
mod seeds;
mod snapshot;

use engine::explore;
use seeds::SEEDS;

#[test]
#[expect(clippy::print_stderr, reason = "coverage shows under --nocapture")]
fn every_bounded_key_sequence_keeps_the_config_editor_properties() {
    let mut failures = Vec::new();
    for seed in SEEDS {
        match explore(seed) {
            Ok(coverage) => eprintln!(
                "{}: depth {}, {} states, {} transitions, {:.1?}",
                seed.name, seed.depth, coverage.states, coverage.transitions, coverage.elapsed
            ),
            Err(failure) => failures.push(failure),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
