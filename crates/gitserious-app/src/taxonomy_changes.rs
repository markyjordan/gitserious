use std::fmt::{self, Display, Formatter};

use gitserious_core::{CommitTypeDefinition, CommitTypeId, Taxonomy};

use crate::GlobalConfiguration;

/// One custom taxonomy whose content differs between two global configurations.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TaxonomyChange<'a> {
    Created(&'a Taxonomy),
    Edited {
        before: &'a Taxonomy,
        after: &'a Taxonomy,
        parts: Vec<EditedPart<'a>>,
    },
    Deleted(&'a Taxonomy),
}

/// One part of a custom taxonomy that an edit changed.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum EditedPart<'a> {
    Description,
    TypeAdded(&'a CommitTypeId),
    TypeChanged(&'a CommitTypeId),
    TypeRemoved(&'a CommitTypeId),
    TypesReordered,
}

/// Lists created and edited taxonomies in `working` order, then deleted ones in `original` order.
pub(crate) fn taxonomy_changes<'a>(
    original: &'a GlobalConfiguration,
    working: &'a GlobalConfiguration,
) -> Vec<TaxonomyChange<'a>> {
    let find = |library: &'a GlobalConfiguration, taxonomy: &Taxonomy| {
        library
            .custom_taxonomies()
            .iter()
            .find(|candidate| candidate.id() == taxonomy.id())
    };
    let created_or_edited =
        working
            .custom_taxonomies()
            .iter()
            .filter_map(|after| match find(original, after) {
                None => Some(TaxonomyChange::Created(after)),
                Some(before) if before != after => Some(TaxonomyChange::Edited {
                    before,
                    after,
                    parts: edited_parts(before, after),
                }),
                Some(_) => None,
            });
    let deleted = original
        .custom_taxonomies()
        .iter()
        .filter(|before| find(working, before).is_none())
        .map(TaxonomyChange::Deleted);
    created_or_edited.chain(deleted).collect()
}

fn edited_parts<'a>(before: &'a Taxonomy, after: &'a Taxonomy) -> Vec<EditedPart<'a>> {
    let find = |types: &'a [CommitTypeDefinition], id: &CommitTypeId| {
        types.iter().find(|definition| definition.id() == id)
    };
    let kept_order = |types: &'a [CommitTypeDefinition], other: &'a [CommitTypeDefinition]| {
        types
            .iter()
            .map(CommitTypeDefinition::id)
            .filter(|id| find(other, id).is_some())
            .collect::<Vec<_>>()
    };
    let mut parts = Vec::new();
    if before.description() != after.description() {
        parts.push(EditedPart::Description);
    }
    for definition in after.commit_types() {
        match find(before.commit_types(), definition.id()) {
            None => parts.push(EditedPart::TypeAdded(definition.id())),
            Some(previous) if previous != definition => {
                parts.push(EditedPart::TypeChanged(definition.id()));
            }
            Some(_) => {}
        }
    }
    parts.extend(
        before
            .commit_types()
            .iter()
            .filter(|definition| find(after.commit_types(), definition.id()).is_none())
            .map(|definition| EditedPart::TypeRemoved(definition.id())),
    );
    if kept_order(before.commit_types(), after.commit_types())
        != kept_order(after.commit_types(), before.commit_types())
    {
        parts.push(EditedPart::TypesReordered);
    }
    parts
}

impl Display for TaxonomyChange<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(taxonomy) => {
                let count = taxonomy.commit_types().len();
                let noun = if count == 1 { "type" } else { "types" };
                write!(
                    formatter,
                    "created  {} v{}, {count} {noun}",
                    taxonomy.id(),
                    taxonomy.version()
                )
            }
            Self::Edited {
                before,
                after,
                parts,
            } => {
                write!(formatter, "edited   {} v{}", after.id(), before.version())?;
                if after.version() != before.version() {
                    write!(formatter, " -> v{}", after.version())?;
                }
                for (index, part) in parts.iter().enumerate() {
                    let separator = if index == 0 { ": " } else { "; " };
                    write!(formatter, "{separator}{part}")?;
                }
                Ok(())
            }
            Self::Deleted(taxonomy) => {
                write!(
                    formatter,
                    "deleted  {} v{}",
                    taxonomy.id(),
                    taxonomy.version()
                )
            }
        }
    }
}

impl Display for EditedPart<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Description => formatter.write_str("description"),
            Self::TypeAdded(id) => write!(formatter, "type {id} added"),
            Self::TypeChanged(id) => write!(formatter, "type {id} changed"),
            Self::TypeRemoved(id) => write!(formatter, "type {id} removed"),
            Self::TypesReordered => formatter.write_str("types reordered"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use gitserious_core::{
        Description, PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion,
        TaxonomyId, TaxonomyVersion,
    };

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    fn commit_type(id: &str, description: &str) -> Result<CommitTypeDefinition, Box<dyn Error>> {
        Ok(CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new(id)?,
            description,
            Vec::new(),
        )?)
    }

    fn taxonomy(
        id: &str,
        version: u16,
        description: &str,
        types: Vec<CommitTypeDefinition>,
    ) -> Result<Taxonomy, Box<dyn Error>> {
        Ok(Taxonomy::new(
            TaxonomyId::new(id)?,
            TaxonomyVersion::new(version)?,
            Description::new(description)?,
            None,
            types,
        )?)
    }

    fn types(ids: &[&str]) -> Result<Vec<CommitTypeDefinition>, Box<dyn Error>> {
        ids.iter().map(|id| commit_type(id, "A change.")).collect()
    }

    fn library(custom: Vec<Taxonomy>) -> Result<GlobalConfiguration, Box<dyn Error>> {
        let conventional = TaxonomyId::new("conventional")?;
        Ok(GlobalConfiguration::new(
            1,
            conventional.clone(),
            vec![conventional],
            custom,
        )?)
    }

    fn id(value: &str) -> Result<CommitTypeId, Box<dyn Error>> {
        Ok(CommitTypeId::new(value)?)
    }

    #[test]
    fn an_unchanged_library_has_no_changes() -> TestResult {
        let original = library(vec![taxonomy("team", 1, "Team.", types(&["release"])?)?])?;

        assert!(taxonomy_changes(&original, &original.clone()).is_empty());
        Ok(())
    }

    #[test]
    fn changes_list_working_order_then_deleted_in_original_order() -> TestResult {
        let kept = taxonomy("kept", 1, "Kept.", types(&["release"])?)?;
        let gone = taxonomy("gone", 2, "Gone.", types(&["release"])?)?;
        let before = taxonomy("edited", 3, "Before.", types(&["release"])?)?;
        let old = taxonomy("old", 1, "Old.", types(&["release"])?)?;
        let after = taxonomy("edited", 4, "After.", types(&["release"])?)?;
        let new = taxonomy("new", 1, "New.", types(&["release"])?)?;
        let original = library(vec![
            kept.clone(),
            gone.clone(),
            before.clone(),
            old.clone(),
        ])?;
        let working = library(vec![after.clone(), kept, new.clone()])?;

        assert_eq!(
            taxonomy_changes(&original, &working),
            [
                TaxonomyChange::Edited {
                    before: &before,
                    after: &after,
                    parts: vec![EditedPart::Description],
                },
                TaxonomyChange::Created(&new),
                TaxonomyChange::Deleted(&gone),
                TaxonomyChange::Deleted(&old),
            ]
        );
        Ok(())
    }

    #[test]
    fn an_edit_names_each_changed_part() -> TestResult {
        let before = taxonomy("team", 1, "Before.", types(&["release", "fix", "chore"])?)?;
        let after = taxonomy(
            "team",
            2,
            "After.",
            vec![
                commit_type("release", "A change.")?,
                commit_type("fix", "A repair.")?,
                commit_type("hotfix", "A change.")?,
            ],
        )?;
        let (original, working) = (library(vec![before])?, library(vec![after])?);

        let changes = taxonomy_changes(&original, &working);

        let [TaxonomyChange::Edited { parts, .. }] = changes.as_slice() else {
            return Err(format!("expected one edit, got {changes:?}").into());
        };
        assert_eq!(
            parts,
            &[
                EditedPart::Description,
                EditedPart::TypeChanged(&id("fix")?),
                EditedPart::TypeAdded(&id("hotfix")?),
                EditedPart::TypeRemoved(&id("chore")?),
            ]
        );
        Ok(())
    }

    #[test]
    fn a_new_property_changes_its_type() -> TestResult {
        let before = taxonomy("team", 1, "Team.", types(&["release"])?)?;
        let property = PropertyDefinition::new(
            PropertyKey::new("summary")?,
            "What shipped",
            PropertyRequirement::Required,
        )?;
        let release = CommitTypeDefinition::new(
            SchemaVersion::V1,
            id("release")?,
            "A change.",
            vec![property],
        )?;
        let after = taxonomy("team", 2, "Team.", vec![release])?;
        let (original, working) = (library(vec![before])?, library(vec![after])?);

        let changes = taxonomy_changes(&original, &working);

        let [TaxonomyChange::Edited { parts, .. }] = changes.as_slice() else {
            return Err(format!("expected one edit, got {changes:?}").into());
        };
        assert_eq!(parts, &[EditedPart::TypeChanged(&id("release")?)]);
        Ok(())
    }

    #[test]
    fn types_reorder_only_when_the_kept_types_change_order() -> TestResult {
        let before = taxonomy("team", 1, "Team.", types(&["a", "b", "c"])?)?;
        let shifted = taxonomy("team", 2, "Team.", types(&["new", "a", "c"])?)?;
        let swapped = taxonomy("team", 2, "Team.", types(&["b", "a", "c"])?)?;
        let original = library(vec![before])?;
        let parts = |after: &Taxonomy| -> Result<String, Box<dyn Error>> {
            let working = library(vec![after.clone()])?;
            Ok(taxonomy_changes(&original, &working)
                .iter()
                .map(ToString::to_string)
                .collect())
        };

        assert_eq!(
            parts(&shifted)?,
            "edited   team v1 -> v2: type new added; type b removed"
        );
        assert_eq!(parts(&swapped)?, "edited   team v1 -> v2: types reordered");
        Ok(())
    }

    #[test]
    fn changes_render_as_words() -> TestResult {
        let one = taxonomy("one", 1, "One.", types(&["release"])?)?;
        let two = taxonomy("two", 3, "Two.", types(&["release", "fix"])?)?;
        let fix = id("fix")?;
        let hotfix = id("hotfix")?;
        let release = id("release")?;
        let lines = [
            TaxonomyChange::Created(&one),
            TaxonomyChange::Created(&two),
            TaxonomyChange::Edited {
                before: &one,
                after: &two,
                parts: vec![
                    EditedPart::Description,
                    EditedPart::TypeChanged(&fix),
                    EditedPart::TypeAdded(&hotfix),
                    EditedPart::TypeRemoved(&release),
                    EditedPart::TypesReordered,
                ],
            },
            TaxonomyChange::Edited {
                before: &two,
                after: &two,
                parts: vec![EditedPart::Description],
            },
            TaxonomyChange::Deleted(&two),
        ]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

        assert_eq!(
            lines,
            [
                "created  one v1, 1 type",
                "created  two v3, 2 types",
                "edited   two v1 -> v3: description; type fix changed; type hotfix added; \
                 type release removed; types reordered",
                "edited   two v3: description",
                "deleted  two v3",
            ]
        );
        Ok(())
    }
}
