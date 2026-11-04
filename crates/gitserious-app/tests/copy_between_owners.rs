#![allow(clippy::expect_used)]

#[path = "support/library_lookup.rs"]
mod library_lookup;
use library_lookup::LibraryLookup;

use std::error::Error;

use gitserious_app::{
    Collision, CopyIn, DraftEdit, DraftField, GlobalConfiguration, LibraryDraft,
    initial_project_configuration,
};
use gitserious_core::{
    Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};

fn project() -> LibraryDraft {
    LibraryDraft::from_project(
        initial_project_configuration(&GlobalConfiguration::default(), None)
            .expect("built-ins form a project"),
    )
}

fn team() -> Result<Taxonomy, Box<dyn Error>> {
    let source = &built_in_taxonomies()[0];
    Ok(Taxonomy::new(
        TaxonomyId::new("team")?,
        TaxonomyVersion::V1,
        source.description().clone(),
        Some(TaxonomyLineage::new(source.id().clone(), source.version())),
        source.commit_types()[..2].to_vec(),
    )?)
}

fn built(draft: &LibraryDraft, id: &str) -> Result<Taxonomy, Box<dyn Error>> {
    Ok(draft
        .build_project()
        .map_err(|problems| format!("{problems:?}"))?
        .find(&TaxonomyId::new(id)?)
        .cloned()
        .ok_or("taxonomy missing")?)
}

#[test]
fn importing_into_a_project_follows_the_collision_rules() -> Result<(), Box<dyn Error>> {
    let mut draft = project();
    let team = team()?;
    let conventional = built_in_taxonomies()[0].clone();

    assert_eq!(draft.collision(&team), Collision::Absent);
    draft.copy_in(&team, CopyIn::New)?;
    assert_eq!(built(&draft, "team")?, team);
    assert_eq!(draft.collision(&team), Collision::Identical);
    assert_eq!(draft.collision(&conventional), Collision::Identical);

    let fix = draft
        .find_path(&["conventional", "fix"])
        .ok_or("conventional has fix")?;
    draft.edit(&fix, DraftEdit::Text(DraftField::Name, "repair"))?;
    assert_eq!(draft.collision(&conventional), Collision::Differs);
    let refused = draft
        .copy_in(&conventional, CopyIn::New)
        .expect_err("a different taxonomy was overwritten silently");
    assert!(
        refused.contains("replace it or copy under a new name"),
        "{refused}"
    );

    draft.copy_in(&conventional, CopyIn::Renamed)?;
    let copy = built(&draft, "conventional-copy")?;
    assert_eq!(copy.commit_types(), conventional.commit_types());
    assert_eq!(copy.version(), TaxonomyVersion::V1);
    assert_eq!(
        copy.derived_from().map(|lineage| lineage.id().as_str()),
        Some("conventional")
    );

    draft.copy_in(&conventional, CopyIn::Replace)?;
    let replaced = built(&draft, "conventional")?;
    assert_eq!(replaced.commit_types(), conventional.commit_types());
    assert_eq!(replaced.version(), conventional.version());
    assert_eq!(
        draft
            .build_project()
            .map_err(|problems| format!("{problems:?}"))?
            .taxonomies()[0]
            .id()
            .as_str(),
        "conventional",
        "replace keeps the taxonomy's place"
    );
    Ok(())
}

#[test]
fn copying_to_the_library_never_shadows_a_built_in() -> Result<(), Box<dyn Error>> {
    let mut project = project();
    let fix = project
        .find_path(&["conventional", "fix"])
        .ok_or("conventional has fix")?;
    project.edit(&fix, DraftEdit::Text(DraftField::Name, "repair"))?;
    let customized = built(&project, "conventional")?;
    let mut library = LibraryDraft::from_configuration(GlobalConfiguration::default());

    assert_eq!(library.collision(&customized), Collision::Reserved);
    assert!(library.copy_in(&customized, CopyIn::New).is_err());
    assert!(library.copy_in(&customized, CopyIn::Replace).is_err());
    library.copy_in(&customized, CopyIn::Renamed)?;

    let saved = library
        .build()
        .map_err(|problems| format!("{problems:?}"))?;
    let copy = saved
        .custom_taxonomies()
        .iter()
        .find(|record| record.id().as_str() == "conventional-copy")
        .ok_or("the copy is in the Library")?;
    assert!(
        copy.commit_types()
            .iter()
            .any(|definition| definition.id().as_str() == "repair")
    );
    assert_eq!(
        built(&project, "conventional")?,
        customized,
        "the project is unchanged"
    );
    Ok(())
}
