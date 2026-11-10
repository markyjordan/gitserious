#![allow(clippy::expect_used)]

#[path = "support/library_lookup.rs"]
mod library_lookup;
use library_lookup::LibraryLookup;

use std::error::Error;

use gitserious_app::{
    DraftEdit, DraftField, GlobalConfiguration, LibraryDraft, ProblemKind,
    initial_project_configuration,
};
use gitserious_core::TaxonomyId;

fn built_in_project() -> LibraryDraft {
    LibraryDraft::from_project(
        initial_project_configuration(&GlobalConfiguration::default())
            .expect("built-ins form a project"),
    )
}

#[test]
fn a_project_owned_built_in_is_editable_without_a_revision_counter() -> Result<(), Box<dyn Error>> {
    let mut draft = built_in_project();
    let fix = draft
        .find_path(&["conventional", "fix"])
        .ok_or("conventional has fix")?;

    draft.edit(
        &fix,
        DraftEdit::Text(DraftField::Description, "Repairs a defect."),
    )?;
    let built = draft
        .build_project()
        .map_err(|problems| format!("{problems:?}"))?;

    let conventional = built
        .find(&TaxonomyId::new("conventional")?)
        .ok_or("conventional stays")?;
    assert_eq!(
        conventional
            .version()
            .map(gitserious_core::TaxonomyVersion::get),
        Some(1)
    );
    assert!(
        conventional
            .commit_types()
            .iter()
            .any(|definition| definition.description() == "Repairs a defect.")
    );
    assert!(draft.is_changed());
    assert!(
        draft.build().is_err(),
        "a project draft never builds a Library"
    );
    Ok(())
}

#[test]
fn forks_and_new_taxonomies_join_the_project_once_they_have_a_type() -> Result<(), Box<dyn Error>> {
    let mut draft = built_in_project();
    let conventional = draft.find_path(&["conventional"]).ok_or("conventional")?;
    draft.fork(&conventional)?;
    let blank = draft.create_taxonomy();
    draft.edit(&blank, DraftEdit::Text(DraftField::Name, "notes"))?;
    draft.edit(&blank, DraftEdit::Text(DraftField::Description, "Notes."))?;

    let problems = draft
        .build_project()
        .expect_err("an empty taxonomy applied");
    assert!(
        problems
            .iter()
            .any(|problem| problem.kind == ProblemKind::NoTypes && problem.node() == Some(&blank))
    );

    draft.edit(&blank, DraftEdit::Remove)?;
    let built = draft
        .build_project()
        .map_err(|problems| format!("{problems:?}"))?;
    let fork = built.taxonomies().last().ok_or("the fork is last")?;
    assert_eq!(fork.id().as_str(), "conventional-fork");
    assert_eq!(
        fork.derived_from().map(|lineage| lineage.id().as_str()),
        Some("conventional")
    );
    assert!(draft.removal_blocker(&conventional).is_some());
    let research = draft.find_path(&["research"]).ok_or("research")?;
    assert!(draft.removal_blocker(&research).is_none());
    Ok(())
}
