//! How a failure reads: the property, the seed, and the keys that reproduce it.

use std::collections::HashSet;

use super::engine::INPUTS;
use super::properties::PROPERTIES;
use super::seeds::Seed;
use super::snapshot::Snapshot;

pub(super) fn labels(keys: &[usize]) -> String {
    keys.iter()
        .map(|&key| INPUTS[key].0)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Judges the last of `keys`, of which the first `seed_keys` reached the seed's start.
pub(super) fn check(
    seed: &Seed,
    seed_keys: usize,
    keys: &[usize],
    before: &Snapshot,
    after: &Result<Option<Snapshot>, String>,
) -> Result<(), String> {
    let report = |name: &str, reason: &str, after: &str| {
        format!(
            "{name} failed from seed \"{}\".\nSeed keys: {}\nKeys after the seed: {}\n\
             Screen before the last key: {:?}\nScreen after the last key: {after}\n{reason}",
            seed.name,
            labels(&keys[..seed_keys]),
            labels(&keys[seed_keys..]),
            before.screen,
        )
    };
    let after = match after {
        Ok(after) => after.as_ref(),
        Err(error) => return Err(report("Replay", error, "none")),
    };
    let after_screen = after.map_or_else(|| "quit".into(), |after| format!("{:?}", after.screen));
    for (name, holds) in PROPERTIES {
        holds(before, &INPUTS[keys[keys.len() - 1]].1, after)
            .map_err(|reason| report(name, &reason, &after_screen))?;
    }
    Ok(())
}

/// Fails on the first of the seed's reach goals that no explored state met.
pub(super) fn check_reaches(
    seed: &Seed,
    observed: &[HashSet<String>],
    start_keys: &str,
    alphabet: &[usize],
) -> Result<(), String> {
    for (reach, values) in seed.reaches.iter().zip(observed) {
        let missing = reach
            .expected
            .iter()
            .filter(|value| !values.contains(**value))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let mut values = values.iter().map(String::as_str).collect::<Vec<_>>();
            values.sort_unstable();
            return Err(format!(
                "{} failed from seed \"{}\".\nSeed keys: {}\nExplored to depth {} with: {}\n\
                 Never reached: {}\nReached: {}",
                reach.name,
                seed.name,
                start_keys,
                seed.depth,
                labels(alphabet),
                missing.join(", "),
                values.join(", "),
            ));
        }
    }
    Ok(())
}
