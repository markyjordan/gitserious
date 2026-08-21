use std::error::Error;

use gitserious_core::{
    ChangeTypeDefinition, ChangeTypeId, ChangeTypeSchema, ChangeTypeSchemaError,
    ConditionalApplicability, Description, DescriptionError, PropertyCondition, PropertyDefinition,
    PropertyKey, PropertyMultiplicity, PropertyRequirement, PropertyResponse,
    PropertyValidationIssueKind, PropertyValue, PropertyValues, ResolveTaxonomyError,
    ResolvedTaxonomy, TaxonomyDefinition, TaxonomyDefinitionError, TaxonomyId, TaxonomyVersion,
    TemplateDefinition, TemplateId, TemplateVersion, TypesetDefinition, TypesetDefinitionError,
    TypesetId, TypesetVersion, ValidationSeverity, built_in_commit_types, built_in_configuration,
    validate_property_responses,
};

fn description(value: &str) -> Result<Description, Box<dyn Error>> {
    Ok(Description::new(value)?)
}

fn change_type(id: &str) -> Result<ChangeTypeDefinition, Box<dyn Error>> {
    Ok(ChangeTypeDefinition::new(
        ChangeTypeId::new(id)?,
        description(&format!("Meaning of {id}."))?,
    ))
}

fn property(
    key: &str,
    requirement: PropertyRequirement,
    multiplicity: PropertyMultiplicity,
) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        format!("Meaning of {key}."),
        requirement,
        multiplicity,
    )?)
}

fn custom_model()
-> Result<(TaxonomyDefinition, TypesetDefinition, TemplateDefinition), Box<dyn Error>> {
    let taxonomy_id = TaxonomyId::new("custom")?;
    let taxonomy = TaxonomyDefinition::new(
        taxonomy_id.clone(),
        TaxonomyVersion::new(2)?,
        description("A custom taxonomy.")?,
        vec![change_type("alpha")?, change_type("beta")?],
    )?;
    let typeset = TypesetDefinition::new(
        taxonomy_id.clone(),
        TypesetId::new("strict")?,
        TypesetVersion::new(3)?,
        description("Strict durable context.")?,
        vec![
            ChangeTypeSchema::new(ChangeTypeId::new("beta")?, Vec::new())?,
            ChangeTypeSchema::new(
                ChangeTypeId::new("alpha")?,
                vec![property(
                    "intent",
                    PropertyRequirement::Required,
                    PropertyMultiplicity::Single,
                )?],
            )?,
        ],
    )?;
    let template = TemplateDefinition::new(
        TemplateId::new("custom-template")?,
        TemplateVersion::new(4)?,
        description("A reusable custom configuration.")?,
        taxonomy_id,
        TypesetId::new("strict")?,
    );
    Ok((taxonomy, typeset, template))
}

#[test]
fn descriptions_preserve_text_and_reject_blank_values() -> Result<(), Box<dyn Error>> {
    let value = Description::new("  Durable meaning 🦀  ")?;
    assert_eq!(value.as_str(), "  Durable meaning 🦀  ");
    assert_eq!(value.to_string(), "  Durable meaning 🦀  ");
    assert_eq!(Description::new(" \n"), Err(DescriptionError));
    assert!(!DescriptionError.to_string().is_empty());
    Ok(())
}

#[test]
fn taxonomy_preserves_identity_version_description_and_order() -> Result<(), Box<dyn Error>> {
    let (taxonomy, _, _) = custom_model()?;
    assert_eq!(taxonomy.id().as_str(), "custom");
    assert_eq!(taxonomy.version().get(), 2);
    assert_eq!(taxonomy.description().as_str(), "A custom taxonomy.");
    assert_eq!(
        taxonomy
            .change_types()
            .iter()
            .map(|change_type| change_type.id().as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    assert_eq!(taxonomy.clone(), taxonomy);
    Ok(())
}

#[test]
fn taxonomy_rejects_empty_and_duplicate_change_types() -> Result<(), Box<dyn Error>> {
    let id = TaxonomyId::new("custom")?;
    assert_eq!(
        TaxonomyDefinition::new(
            id.clone(),
            TaxonomyVersion::V1,
            description("Custom.")?,
            Vec::new(),
        ),
        Err(TaxonomyDefinitionError::EmptyChangeTypes)
    );
    assert_eq!(
        TaxonomyDefinition::new(
            id,
            TaxonomyVersion::V1,
            description("Custom.")?,
            vec![change_type("alpha")?, change_type("alpha")?],
        ),
        Err(TaxonomyDefinitionError::DuplicateChangeType(
            ChangeTypeId::new("alpha")?
        ))
    );
    Ok(())
}

#[test]
fn change_type_schemas_allow_explicitly_empty_properties_and_reject_duplicates()
-> Result<(), Box<dyn Error>> {
    let empty = ChangeTypeSchema::new(ChangeTypeId::new("alpha")?, Vec::new())?;
    assert!(empty.properties().is_empty());
    let duplicate = property(
        "intent",
        PropertyRequirement::Required,
        PropertyMultiplicity::Single,
    )?;
    assert_eq!(
        ChangeTypeSchema::new(
            ChangeTypeId::new("alpha")?,
            vec![duplicate.clone(), duplicate],
        ),
        Err(ChangeTypeSchemaError::DuplicateProperty(PropertyKey::new(
            "intent"
        )?))
    );
    Ok(())
}

#[test]
fn typesets_preserve_qualified_identity_schema_and_property_order() -> Result<(), Box<dyn Error>> {
    let (_, typeset, _) = custom_model()?;
    assert_eq!(typeset.taxonomy().as_str(), "custom");
    assert_eq!(typeset.id().as_str(), "strict");
    assert_eq!(typeset.version().get(), 3);
    assert_eq!(typeset.schemas()[0].change_type().as_str(), "beta");
    assert_eq!(
        typeset.schemas()[1].properties()[0].key().as_str(),
        "intent"
    );
    Ok(())
}

#[test]
fn typesets_reject_empty_and_duplicate_type_coverage() -> Result<(), Box<dyn Error>> {
    let taxonomy = TaxonomyId::new("custom")?;
    let id = TypesetId::new("default")?;
    assert_eq!(
        TypesetDefinition::new(
            taxonomy.clone(),
            id.clone(),
            TypesetVersion::V1,
            description("Custom.")?,
            Vec::new(),
        ),
        Err(TypesetDefinitionError::EmptySchemas)
    );
    assert_eq!(
        TypesetDefinition::new(
            taxonomy,
            id,
            TypesetVersion::V1,
            description("Custom.")?,
            vec![
                ChangeTypeSchema::new(ChangeTypeId::new("alpha")?, Vec::new())?,
                ChangeTypeSchema::new(ChangeTypeId::new("alpha")?, Vec::new())?,
            ],
        ),
        Err(TypesetDefinitionError::DuplicateChangeType(
            ChangeTypeId::new("alpha")?
        ))
    );
    Ok(())
}

#[test]
fn template_resolution_joins_in_taxonomy_order_not_typeset_entry_order()
-> Result<(), Box<dyn Error>> {
    let (taxonomy, typeset, template) = custom_model()?;
    let resolved = ResolvedTaxonomy::resolve(&template, &taxonomy, &typeset)?;
    assert_eq!(resolved.template_id().as_str(), "custom-template");
    assert_eq!(resolved.template_version().get(), 4);
    assert_eq!(resolved.taxonomy_id().as_str(), "custom");
    assert_eq!(resolved.typeset_id().as_str(), "strict");
    assert_eq!(
        resolved
            .change_types()
            .iter()
            .map(|change_type| change_type.id().as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    assert_eq!(resolved.change_types()[0].properties().len(), 1);
    assert!(resolved.change_types()[1].properties().is_empty());
    Ok(())
}

#[test]
fn resolution_rejects_reference_mismatches_missing_coverage_and_unknown_entries()
-> Result<(), Box<dyn Error>> {
    let (taxonomy, typeset, _template) = custom_model()?;
    let wrong_template = TemplateDefinition::new(
        TemplateId::new("wrong")?,
        TemplateVersion::V1,
        description("Wrong.")?,
        TaxonomyId::new("other")?,
        typeset.id().clone(),
    );
    assert!(matches!(
        ResolvedTaxonomy::resolve(&wrong_template, &taxonomy, &typeset),
        Err(ResolveTaxonomyError::TemplateTaxonomyMismatch { .. })
    ));

    let missing = TypesetDefinition::new(
        taxonomy.id().clone(),
        TypesetId::new("missing")?,
        TypesetVersion::V1,
        description("Missing.")?,
        vec![ChangeTypeSchema::new(
            ChangeTypeId::new("alpha")?,
            Vec::new(),
        )?],
    )?;
    let missing_template = TemplateDefinition::new(
        TemplateId::new("missing")?,
        TemplateVersion::V1,
        description("Missing.")?,
        taxonomy.id().clone(),
        missing.id().clone(),
    );
    assert_eq!(
        ResolvedTaxonomy::resolve(&missing_template, &taxonomy, &missing),
        Err(ResolveTaxonomyError::MissingTypesetChangeType(
            ChangeTypeId::new("beta")?
        ))
    );

    let unknown = TypesetDefinition::new(
        taxonomy.id().clone(),
        TypesetId::new("unknown")?,
        TypesetVersion::V1,
        description("Unknown.")?,
        vec![
            ChangeTypeSchema::new(ChangeTypeId::new("alpha")?, Vec::new())?,
            ChangeTypeSchema::new(ChangeTypeId::new("beta")?, Vec::new())?,
            ChangeTypeSchema::new(ChangeTypeId::new("gamma")?, Vec::new())?,
        ],
    )?;
    let unknown_template = TemplateDefinition::new(
        TemplateId::new("unknown")?,
        TemplateVersion::V1,
        description("Unknown.")?,
        taxonomy.id().clone(),
        unknown.id().clone(),
    );
    assert_eq!(
        ResolvedTaxonomy::resolve(&unknown_template, &taxonomy, &unknown),
        Err(ResolveTaxonomyError::UnknownTypesetChangeType(
            ChangeTypeId::new("gamma")?
        ))
    );
    Ok(())
}

#[test]
fn built_in_baseline_resolves_through_the_public_generic_model() -> Result<(), Box<dyn Error>> {
    let built_in = built_in_configuration();
    let resolved =
        ResolvedTaxonomy::resolve(built_in.template(), built_in.taxonomy(), built_in.typeset())?;
    assert_eq!(resolved.template_id().as_str(), "default");
    assert_eq!(resolved.taxonomy_id().as_str(), "conventional");
    assert_eq!(resolved.typeset_id().as_str(), "default");
    assert_eq!(resolved.change_types().len(), 11);
    for (resolved, legacy) in resolved.change_types().iter().zip(built_in_commit_types()) {
        assert_eq!(resolved.id(), legacy.id());
        assert_eq!(resolved.description().as_str(), legacy.description());
        assert_eq!(resolved.properties(), legacy.properties());
    }
    Ok(())
}

#[test]
fn requirement_validation_reports_errors_and_recommendations_from_the_typeset()
-> Result<(), Box<dyn Error>> {
    let condition = PropertyCondition::new(
        gitserious_core::ConditionId::new("known-cost")?,
        "Required when a known cost exists.",
    )?;
    let taxonomy_id = TaxonomyId::new("validation")?;
    let taxonomy = TaxonomyDefinition::new(
        taxonomy_id.clone(),
        TaxonomyVersion::V1,
        description("Validation.")?,
        vec![change_type("change")?],
    )?;
    let typeset = TypesetDefinition::new(
        taxonomy_id.clone(),
        TypesetId::new("all-levels")?,
        TypesetVersion::V1,
        description("All levels.")?,
        vec![ChangeTypeSchema::new(
            ChangeTypeId::new("change")?,
            vec![
                property(
                    "required",
                    PropertyRequirement::Required,
                    PropertyMultiplicity::Single,
                )?,
                property(
                    "recommended",
                    PropertyRequirement::Recommended,
                    PropertyMultiplicity::Single,
                )?,
                property(
                    "optional",
                    PropertyRequirement::Optional,
                    PropertyMultiplicity::Multiple,
                )?,
                property(
                    "conditional",
                    PropertyRequirement::Conditional(condition),
                    PropertyMultiplicity::Single,
                )?,
            ],
        )?],
    )?;
    let template = TemplateDefinition::new(
        TemplateId::new("validation")?,
        TemplateVersion::V1,
        description("Validation.")?,
        taxonomy_id,
        typeset.id().clone(),
    );
    let resolved = ResolvedTaxonomy::resolve(&template, &taxonomy, &typeset)?;
    let report = validate_property_responses(&resolved.change_types()[0], &[]);
    assert!(report.has_errors());
    assert_eq!(report.issues().len(), 3);
    assert_eq!(
        report.issues()[0].kind(),
        &PropertyValidationIssueKind::MissingRequired(PropertyKey::new("required")?)
    );
    assert_eq!(report.issues()[1].severity(), ValidationSeverity::Warning);
    assert_eq!(
        report.issues()[2].kind(),
        &PropertyValidationIssueKind::MissingConditionalDecision(PropertyKey::new("conditional")?)
    );

    let responses = vec![
        PropertyResponse::new(
            PropertyKey::new("required")?,
            Some(PropertyValues::single(PropertyValue::new("value")?)),
            None,
        ),
        PropertyResponse::new(
            PropertyKey::new("conditional")?,
            None,
            Some(ConditionalApplicability::DoesNotApply),
        ),
    ];
    let report = validate_property_responses(&resolved.change_types()[0], &responses);
    assert!(!report.has_errors());
    assert_eq!(report.issues().len(), 1);
    assert_eq!(report.issues()[0].severity(), ValidationSeverity::Warning);
    Ok(())
}

#[test]
fn conditional_and_multiplicity_contradictions_are_blocking() -> Result<(), Box<dyn Error>> {
    let built_in = built_in_configuration();
    let resolved =
        ResolvedTaxonomy::resolve(built_in.template(), built_in.taxonomy(), built_in.typeset())?;
    let feat = &resolved.change_types()[0];
    let responses = vec![
        PropertyResponse::new(
            PropertyKey::new("intent")?,
            Some(PropertyValues::multiple([PropertyValue::new("one")?])?),
            Some(ConditionalApplicability::Applies),
        ),
        PropertyResponse::new(
            PropertyKey::new("constraints")?,
            Some(PropertyValues::single(PropertyValue::new("constraint")?)),
            Some(ConditionalApplicability::DoesNotApply),
        ),
        PropertyResponse::new(PropertyKey::new("unknown")?, None, None),
    ];
    let report = validate_property_responses(feat, &responses);
    assert!(report.has_errors());
    assert!(report.issues().iter().any(|issue| matches!(
        issue.kind(),
        PropertyValidationIssueKind::Multiplicity { .. }
    )));
    assert!(report.issues().iter().any(|issue| matches!(
        issue.kind(),
        PropertyValidationIssueKind::UnexpectedConditionalDecision(_)
    )));
    assert!(report.issues().iter().any(|issue| matches!(
        issue.kind(),
        PropertyValidationIssueKind::ValueForNonApplicableProperty(_)
    )));
    assert!(report.issues().iter().any(|issue| matches!(
        issue.kind(),
        PropertyValidationIssueKind::UnknownProperty(_)
    )));
    Ok(())
}
