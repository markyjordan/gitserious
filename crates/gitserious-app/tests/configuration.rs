use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_app::{
    ConfigurationCatalog, ConfigurationCatalogError, ConfigurationEdit, ConfigurationEntity,
    ConfigurationMutationError, UserConfiguration, UserConfigurationError, UserConfigurationStore,
    apply_configuration_edits, create_taxonomy, create_template, create_typeset, delete_taxonomy,
    delete_template, delete_typeset, find_taxonomy, find_template, find_typeset,
    fingerprint_resolved_taxonomy, list_taxonomies, list_templates, list_typesets, update_taxonomy,
};
use gitserious_core::{
    ChangeTypeDefinition, ChangeTypeId, ChangeTypeSchema, Description, PropertyDefinition,
    PropertyKey, PropertyMultiplicity, PropertyRequirement, ResolvedTaxonomy, TaxonomyDefinition,
    TaxonomyId, TaxonomyVersion, TemplateDefinition, TemplateId, TemplateVersion,
    TypesetDefinition, TypesetId, TypesetVersion, built_in_configuration,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeError {
    Unavailable,
    Concurrent,
}

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "configuration store unavailable",
            Self::Concurrent => "configuration changed concurrently",
        })
    }
}

impl Error for FakeError {}

struct RecordingStore {
    current: RefCell<UserConfiguration>,
    load_error: Cell<Option<FakeError>>,
    swap_error: Cell<Option<FakeError>>,
    loads: Cell<usize>,
    swaps: Cell<usize>,
}

impl RecordingStore {
    fn new(current: UserConfiguration) -> Self {
        Self {
            current: RefCell::new(current),
            load_error: Cell::new(None),
            swap_error: Cell::new(None),
            loads: Cell::new(0),
            swaps: Cell::new(0),
        }
    }

    fn snapshot(&self) -> UserConfiguration {
        self.current.borrow().clone()
    }
}

impl UserConfigurationStore for RecordingStore {
    type Error = FakeError;

    fn load(&self) -> Result<UserConfiguration, Self::Error> {
        self.loads.set(self.loads.get() + 1);
        match self.load_error.get() {
            Some(error) => Err(error),
            None => Ok(self.snapshot()),
        }
    }

    fn compare_and_swap(
        &self,
        expected: &UserConfiguration,
        replacement: &UserConfiguration,
    ) -> Result<(), Self::Error> {
        self.swaps.set(self.swaps.get() + 1);
        if let Some(error) = self.swap_error.get() {
            return Err(error);
        }
        if *self.current.borrow() != *expected {
            return Err(FakeError::Concurrent);
        }
        *self.current.borrow_mut() = replacement.clone();
        Ok(())
    }
}

fn description(value: &str) -> Result<Description, Box<dyn Error>> {
    Ok(Description::new(value)?)
}

fn change_type(id: &str) -> Result<ChangeTypeDefinition, Box<dyn Error>> {
    Ok(ChangeTypeDefinition::new(
        ChangeTypeId::new(id)?,
        description(&format!("Meaning of {id}."))?,
    ))
}

fn property(key: &str) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        format!("Meaning of {key}."),
        PropertyRequirement::Required,
        PropertyMultiplicity::Single,
    )?)
}

fn taxonomy(version: u16, include_beta: bool) -> Result<TaxonomyDefinition, Box<dyn Error>> {
    let mut change_types = vec![change_type("alpha")?];
    if include_beta {
        change_types.push(change_type("beta")?);
    }
    Ok(TaxonomyDefinition::new(
        TaxonomyId::new("custom")?,
        TaxonomyVersion::new(version)?,
        description("Custom taxonomy.")?,
        change_types,
    )?)
}

fn typeset(version: u16, include_beta: bool) -> Result<TypesetDefinition, Box<dyn Error>> {
    let mut schemas = vec![ChangeTypeSchema::new(
        ChangeTypeId::new("alpha")?,
        vec![property("intent")?],
    )?];
    if include_beta {
        schemas.push(ChangeTypeSchema::new(
            ChangeTypeId::new("beta")?,
            Vec::new(),
        )?);
    }
    Ok(TypesetDefinition::new(
        TaxonomyId::new("custom")?,
        TypesetId::new("strict")?,
        TypesetVersion::new(version)?,
        description("Strict typeset.")?,
        schemas,
    )?)
}

fn template(version: u16) -> Result<TemplateDefinition, Box<dyn Error>> {
    Ok(TemplateDefinition::new(
        TemplateId::new("custom-template")?,
        TemplateVersion::new(version)?,
        description("Custom template.")?,
        TaxonomyId::new("custom")?,
        TypesetId::new("strict")?,
    ))
}

fn full_configuration() -> Result<UserConfiguration, Box<dyn Error>> {
    Ok(UserConfiguration::new(
        vec![taxonomy(1, false)?],
        vec![typeset(1, false)?],
        vec![template(1)?],
    )?)
}

#[test]
fn empty_user_catalog_resolves_the_built_in_template_generically() -> Result<(), Box<dyn Error>> {
    let catalog = ConfigurationCatalog::new(&UserConfiguration::default())?;
    let resolved = catalog.resolve(&TemplateId::new("default")?)?;
    assert_eq!(resolved.template_id().as_str(), "default");
    assert_eq!(resolved.taxonomy_id().as_str(), "conventional");
    assert_eq!(resolved.typeset_id().as_str(), "default");
    assert_eq!(resolved.change_types().len(), 11);
    assert_eq!(catalog.taxonomies().len(), 1);
    assert_eq!(catalog.typesets().len(), 1);
    assert_eq!(catalog.templates().len(), 1);
    Ok(())
}

#[test]
fn custom_template_resolves_the_joined_taxonomy_and_typeset() -> Result<(), Box<dyn Error>> {
    let configuration = full_configuration()?;
    let catalog = ConfigurationCatalog::new(&configuration)?;
    let resolved = catalog.resolve(&TemplateId::new("custom-template")?)?;
    assert_eq!(resolved.taxonomy_id().as_str(), "custom");
    assert_eq!(resolved.typeset_id().as_str(), "strict");
    assert_eq!(resolved.change_types()[0].id().as_str(), "alpha");
    assert_eq!(
        resolved.change_types()[0].properties()[0].key().as_str(),
        "intent"
    );
    Ok(())
}

#[test]
fn effective_catalog_rejects_every_built_in_shadowing_form() -> Result<(), Box<dyn Error>> {
    let built_in = built_in_configuration();
    let taxonomy_collision =
        UserConfiguration::new(vec![built_in.taxonomy().clone()], Vec::new(), Vec::new())?;
    assert!(matches!(
        ConfigurationCatalog::new(&taxonomy_collision),
        Err(ConfigurationCatalogError::ReservedTaxonomy(_))
    ));
    let typeset_collision =
        UserConfiguration::new(Vec::new(), vec![built_in.typeset().clone()], Vec::new())?;
    assert!(matches!(
        ConfigurationCatalog::new(&typeset_collision),
        Err(ConfigurationCatalogError::ReservedTypeset { .. })
    ));
    let template_collision =
        UserConfiguration::new(Vec::new(), Vec::new(), vec![built_in.template().clone()])?;
    assert!(matches!(
        ConfigurationCatalog::new(&template_collision),
        Err(ConfigurationCatalogError::ReservedTemplate(_))
    ));
    Ok(())
}

#[test]
fn catalog_rejects_dangling_and_incomplete_definitions() -> Result<(), Box<dyn Error>> {
    let dangling_typeset =
        UserConfiguration::new(Vec::new(), vec![typeset(1, false)?], Vec::new())?;
    assert!(matches!(
        ConfigurationCatalog::new(&dangling_typeset),
        Err(ConfigurationCatalogError::UnknownTypesetTaxonomy { .. })
    ));

    let incomplete = UserConfiguration::new(
        vec![taxonomy(1, true)?],
        vec![typeset(1, false)?],
        Vec::new(),
    )?;
    assert!(matches!(
        ConfigurationCatalog::new(&incomplete),
        Err(ConfigurationCatalogError::Resolution(_))
    ));

    let dangling_template =
        UserConfiguration::new(vec![taxonomy(1, false)?], Vec::new(), vec![template(1)?])?;
    assert!(matches!(
        ConfigurationCatalog::new(&dangling_template),
        Err(ConfigurationCatalogError::UnknownTemplateTypeset { .. })
    ));
    Ok(())
}

#[test]
fn snapshots_reject_duplicates_and_canonicalize_top_level_order() -> Result<(), Box<dyn Error>> {
    let duplicate = taxonomy(1, false)?;
    assert_eq!(
        UserConfiguration::new(vec![duplicate.clone(), duplicate], Vec::new(), Vec::new(),),
        Err(UserConfigurationError::DuplicateTaxonomy(TaxonomyId::new(
            "custom"
        )?))
    );

    let second = TaxonomyDefinition::new(
        TaxonomyId::new("aaa")?,
        TaxonomyVersion::V1,
        description("First alphabetically.")?,
        vec![change_type("alpha")?],
    )?;
    let configuration =
        UserConfiguration::new(vec![taxonomy(1, false)?, second], Vec::new(), Vec::new())?;
    assert_eq!(configuration.taxonomies()[0].id().as_str(), "aaa");
    assert_eq!(configuration.taxonomies()[1].id().as_str(), "custom");
    Ok(())
}

#[test]
fn item_crud_builds_and_queries_a_reusable_configuration() -> Result<(), Box<dyn Error>> {
    let store = RecordingStore::new(UserConfiguration::default());
    create_taxonomy(&store, taxonomy(1, false)?)?;
    create_typeset(&store, typeset(1, false)?)?;
    create_template(&store, template(1)?)?;

    assert!(find_taxonomy(&store, &TaxonomyId::new("custom")?)?.is_some());
    assert!(
        find_typeset(
            &store,
            &TaxonomyId::new("custom")?,
            &TypesetId::new("strict")?
        )?
        .is_some()
    );
    assert!(find_template(&store, &TemplateId::new("custom-template")?)?.is_some());
    assert_eq!(list_taxonomies(&store)?.len(), 2);
    assert_eq!(list_typesets(&store)?.len(), 2);
    assert_eq!(list_templates(&store)?.len(), 2);
    assert_eq!(store.swaps.get(), 3);
    Ok(())
}

#[test]
fn failed_create_and_update_leave_the_snapshot_unchanged() -> Result<(), Box<dyn Error>> {
    let initial = full_configuration()?;
    let store = RecordingStore::new(initial.clone());
    let duplicate = create_taxonomy(&store, taxonomy(1, false)?);
    assert!(matches!(
        duplicate,
        Err(ConfigurationMutationError::AlreadyExists(
            ConfigurationEntity::Taxonomy(_)
        ))
    ));
    let stale = update_taxonomy(&store, taxonomy(1, false)?);
    assert!(matches!(
        stale,
        Err(ConfigurationMutationError::VersionNotAdvanced { .. })
    ));
    let incompatible = update_taxonomy(&store, taxonomy(2, true)?);
    assert!(matches!(
        incompatible,
        Err(ConfigurationMutationError::Catalog(_))
    ));
    assert_eq!(store.snapshot(), initial);
    assert_eq!(store.swaps.get(), 0);
    Ok(())
}

#[test]
fn atomic_edits_can_advance_a_taxonomy_and_all_covering_typesets() -> Result<(), Box<dyn Error>> {
    let store = RecordingStore::new(full_configuration()?);
    apply_configuration_edits(
        &store,
        [
            ConfigurationEdit::UpdateTaxonomy(taxonomy(2, true)?),
            ConfigurationEdit::UpdateTypeset(typeset(2, true)?),
        ],
    )?;
    let catalog = ConfigurationCatalog::new(&store.snapshot())?;
    let resolved = catalog.resolve(&TemplateId::new("custom-template")?)?;
    assert_eq!(resolved.taxonomy_version().get(), 2);
    assert_eq!(resolved.typeset_version().get(), 2);
    assert_eq!(resolved.change_types().len(), 2);
    assert_eq!(store.swaps.get(), 1);
    Ok(())
}

#[test]
fn deletes_protect_dependents_then_succeed_in_dependency_order() -> Result<(), Box<dyn Error>> {
    let store = RecordingStore::new(full_configuration()?);
    let taxonomy_result = delete_taxonomy(&store, &TaxonomyId::new("custom")?);
    assert!(matches!(
        taxonomy_result,
        Err(ConfigurationMutationError::Referenced { .. })
    ));
    let typeset_result = delete_typeset(
        &store,
        &TaxonomyId::new("custom")?,
        &TypesetId::new("strict")?,
    );
    assert!(matches!(
        typeset_result,
        Err(ConfigurationMutationError::Referenced { .. })
    ));

    delete_template(&store, &TemplateId::new("custom-template")?)?;
    delete_typeset(
        &store,
        &TaxonomyId::new("custom")?,
        &TypesetId::new("strict")?,
    )?;
    delete_taxonomy(&store, &TaxonomyId::new("custom")?)?;
    assert_eq!(store.snapshot(), UserConfiguration::default());
    Ok(())
}

#[test]
fn reserved_mutations_and_store_failures_remain_precise() -> Result<(), Box<dyn Error>> {
    let store = RecordingStore::new(UserConfiguration::default());
    let reserved = create_template(&store, built_in_configuration().template().clone());
    assert!(matches!(
        reserved,
        Err(ConfigurationMutationError::Reserved(
            ConfigurationEntity::Template(_)
        ))
    ));
    store.load_error.set(Some(FakeError::Unavailable));
    assert!(matches!(
        list_templates(&store),
        Err(ConfigurationMutationError::Store(FakeError::Unavailable))
    ));
    store.load_error.set(None);
    store.swap_error.set(Some(FakeError::Concurrent));
    assert!(matches!(
        create_taxonomy(&store, taxonomy(1, false)?),
        Err(ConfigurationMutationError::Store(FakeError::Concurrent))
    ));
    Ok(())
}

#[test]
fn resolved_fingerprint_is_stable_and_covers_semantic_identity() -> Result<(), Box<dyn Error>> {
    let catalog = ConfigurationCatalog::new(&full_configuration()?)?;
    let resolved = catalog.resolve(&TemplateId::new("custom-template")?)?;
    let first = fingerprint_resolved_taxonomy(&resolved);
    let second = fingerprint_resolved_taxonomy(&resolved.clone());
    assert_eq!(first, second);

    let built_in = built_in_configuration();
    let conventional =
        ResolvedTaxonomy::resolve(built_in.template(), built_in.taxonomy(), built_in.typeset())?;
    assert_ne!(first, fingerprint_resolved_taxonomy(&conventional));
    assert!(first.to_string().starts_with("sha256:"));
    Ok(())
}
