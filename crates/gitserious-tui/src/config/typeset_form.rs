use std::collections::BTreeMap;

use gitserious_app::ConfigurationEdit;
use gitserious_core::{
    ChangeTypeId, ChangeTypeSchema, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, TaxonomyDefinition,
    TaxonomyId, TypesetDefinition, TypesetId, TypesetVersion,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::form::{Field, Form, FormAction, Group};

#[derive(Clone)]
struct PropertyDraft {
    values: [String; 6],
}

impl PropertyDraft {
    fn empty() -> Self {
        Self {
            values: [
                String::new(),
                String::new(),
                "required".into(),
                "single".into(),
                String::new(),
                String::new(),
            ],
        }
    }

    fn from_definition(property: &PropertyDefinition) -> Self {
        let (requirement, condition, rationale) = match property.requirement() {
            PropertyRequirement::Required => ("required", "", ""),
            PropertyRequirement::Recommended => ("recommended", "", ""),
            PropertyRequirement::Optional => ("optional", "", ""),
            PropertyRequirement::Conditional(condition) => (
                "conditional",
                condition.id().as_str(),
                condition.rationale(),
            ),
        };
        let multiplicity = match property.multiplicity() {
            PropertyMultiplicity::Single => "single",
            PropertyMultiplicity::Multiple => "multiple",
        };
        Self {
            values: [
                property.key().to_string(),
                property.description().to_owned(),
                requirement.into(),
                multiplicity.into(),
                condition.into(),
                rationale.into(),
            ],
        }
    }

    fn definition(&self) -> Result<PropertyDefinition, String> {
        let requirement = match self.values[2].as_str() {
            "required" => PropertyRequirement::Required,
            "recommended" => PropertyRequirement::Recommended,
            "optional" => PropertyRequirement::Optional,
            "conditional" => PropertyRequirement::Conditional(
                PropertyCondition::new(
                    ConditionId::new(self.values[4].clone())
                        .map_err(|error| format!("Condition id: {error}"))?,
                    self.values[5].clone(),
                )
                .map_err(|error| format!("Condition rationale: {error}"))?,
            ),
            _ => return Err("Choose a valid requirement level.".into()),
        };
        let multiplicity = match self.values[3].as_str() {
            "single" => PropertyMultiplicity::Single,
            "multiple" => PropertyMultiplicity::Multiple,
            _ => return Err("Choose single or multiple values.".into()),
        };
        PropertyDefinition::new(
            PropertyKey::new(self.values[0].clone())
                .map_err(|error| format!("Property key: {error}"))?,
            self.values[1].clone(),
            requirement,
            multiplicity,
        )
        .map_err(|error| error.to_string())
    }

    fn fields(&self, schema: usize, property: usize) -> Vec<Field> {
        let labels = [
            "key",
            "description",
            "requirement",
            "multiplicity",
            "condition id (conditional)",
            "condition rationale (conditional)",
        ];
        let mut fields: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                Field::new(
                    format!("  Property {} · {label}", property + 1),
                    &self.values[index],
                    matches!(index, 1 | 5),
                    false,
                    Group::Property(schema, property),
                )
            })
            .collect();
        fields[2].options = ["required", "recommended", "optional", "conditional"]
            .map(str::to_owned)
            .to_vec();
        fields[3].options = ["single", "multiple"].map(str::to_owned).to_vec();
        fields
    }
}

type Schemas = Vec<(String, Vec<PropertyDraft>)>;

pub(super) struct TypesetForm {
    pub form: Form,
    taxonomies: Vec<TaxonomyDefinition>,
    original: Option<TypesetDefinition>,
    schema_names: Vec<String>,
    cache: BTreeMap<String, Schemas>,
    reconcile: bool,
}

impl TypesetForm {
    pub(super) fn new(
        taxonomies: Vec<TaxonomyDefinition>,
        original: Option<TypesetDefinition>,
    ) -> Result<Self, String> {
        let selected = original
            .as_ref()
            .map(|value| value.taxonomy().as_str())
            .or_else(|| taxonomies.first().map(|value| value.id().as_str()))
            .ok_or("Create a taxonomy first.")?;
        let taxonomy = taxonomies
            .iter()
            .find(|value| value.id().as_str() == selected)
            .ok_or("The typeset's taxonomy is missing; restore it before editing.")?;
        let schemas = schema_drafts(taxonomy, original.as_ref());
        let reconcile = original.as_ref().is_some_and(|value| {
            value
                .schemas()
                .iter()
                .map(|schema| schema.change_type().as_str())
                .collect::<Vec<_>>()
                != schemas
                    .iter()
                    .map(|(id, _)| id.as_str())
                    .collect::<Vec<_>>()
        });
        let fields = vec![
            Field::new(
                "Typeset id",
                original.as_ref().map_or("", |value| value.id().as_str()),
                false,
                original.is_some(),
                Group::Metadata,
            ),
            Field::new(
                "Taxonomy",
                selected,
                false,
                original.is_some(),
                Group::Metadata,
            )
            .choices(
                taxonomies
                    .iter()
                    .map(|value| value.id().to_string())
                    .collect(),
            ),
            Field::new(
                "Description",
                original
                    .as_ref()
                    .map_or("", |value| value.description().as_str()),
                true,
                false,
                Group::Metadata,
            ),
        ];
        let mut editor = Self {
            form: Form::new("Typeset", fields),
            taxonomies,
            original,
            schema_names: Vec::new(),
            cache: BTreeMap::new(),
            reconcile,
        };
        editor.rebuild(schemas, Group::Metadata);
        editor.form = Form::new(
            if editor.original.is_some() {
                "Edit typeset"
            } else {
                "Create typeset"
            },
            editor.form.fields,
        );
        Ok(editor)
    }

    fn drafts(&self) -> Schemas {
        self.schema_names
            .iter()
            .enumerate()
            .map(|(schema, name)| {
                let fields: Vec<_> = self
                    .form
                    .fields
                    .iter()
                    .filter(
                        |field| matches!(field.group, Group::Property(index, _) if index == schema),
                    )
                    .collect();
                let properties = fields
                    .chunks_exact(6)
                    .map(|fields| PropertyDraft {
                        values: std::array::from_fn(|index| fields[index].value()),
                    })
                    .collect();
                (name.clone(), properties)
            })
            .collect()
    }

    fn rebuild(&mut self, schemas: Schemas, focus: Group) {
        self.form.fields.truncate(3);
        self.schema_names.clear();
        for (schema, (name, properties)) in schemas.into_iter().enumerate() {
            self.schema_names.push(name.clone());
            self.form.fields.push(Field::new(
                format!("Type: {name}"),
                if properties.is_empty() {
                    "No properties (intentionally empty). ctrl+n adds a property."
                } else {
                    "ctrl+n adds a property. ctrl+d removes the selected property."
                },
                false,
                true,
                Group::Schema(schema),
            ));
            for (index, property) in properties.into_iter().enumerate() {
                self.form.fields.extend(property.fields(schema, index));
            }
        }
        self.form.focus = self
            .form
            .fields
            .iter()
            .position(|field| field.group == focus)
            .unwrap_or(0);
        self.conditions();
    }

    fn conditions(&mut self) {
        for start in 3..self.form.fields.len() {
            if let Group::Property(_, _) = self.form.fields[start].group
                && (start == 0
                    || self.form.fields[start - 1].group != self.form.fields[start].group)
            {
                let conditional = self.form.fields[start + 2].value() == "conditional";
                self.form.fields[start + 4].readonly = !conditional;
                self.form.fields[start + 5].readonly = !conditional;
            }
        }
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> Result<FormAction, String> {
        if self.form.confirming_discard() {
            return Ok(self.form.key(key));
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('n' | 'd'))
        {
            self.change_property(key.code == KeyCode::Char('n'))?;
            return Ok(FormAction::Continue);
        }
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Up | KeyCode::Down)
        {
            self.move_property(key.code == KeyCode::Down);
            return Ok(FormAction::Continue);
        }
        let old_taxonomy = self.form.fields[1].value();
        let result = self.form.key(key);
        if old_taxonomy != self.form.fields[1].value() {
            self.cache.insert(old_taxonomy, self.drafts());
            let selected = self.form.fields[1].value();
            let taxonomy = self
                .taxonomies
                .iter()
                .find(|value| value.id().as_str() == selected)
                .ok_or("Unknown taxonomy.")?;
            let schemas = self
                .cache
                .get(&selected)
                .cloned()
                .unwrap_or_else(|| schema_drafts(taxonomy, None));
            self.rebuild(schemas, Group::Metadata);
            self.form.focus = 1;
        }
        self.conditions();
        Ok(result)
    }

    fn change_property(&mut self, add: bool) -> Result<(), String> {
        let group = self.form.fields[self.form.focus].group;
        let (Group::Schema(schema) | Group::Property(schema, _)) = group else {
            return Err("Select a change type or property first.".into());
        };
        let mut schemas = self.drafts();
        let focus = if add {
            let index = schemas[schema].1.len();
            schemas[schema].1.push(PropertyDraft::empty());
            Group::Property(schema, index)
        } else {
            let Group::Property(_, index) = group else {
                return Err("Select a property field to remove it.".into());
            };
            schemas[schema].1.remove(index);
            Group::Schema(schema)
        };
        self.rebuild(schemas, focus);
        Ok(())
    }

    fn move_property(&mut self, down: bool) {
        let Group::Property(schema, index) = self.form.fields[self.form.focus].group else {
            return;
        };
        let mut schemas = self.drafts();
        let target = if down {
            (index + 1).min(schemas[schema].1.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        schemas[schema].1.swap(index, target);
        self.rebuild(schemas, Group::Property(schema, target));
    }

    pub(super) fn submit(&self) -> Result<Vec<ConfigurationEdit>, String> {
        if self.original.is_some() && !self.form.is_dirty() && !self.reconcile {
            return Ok(Vec::new());
        }
        let id = self.form.parse(0, TypesetId::new)?;
        let taxonomy = self.form.parse(1, TaxonomyId::new)?;
        if self
            .original
            .as_ref()
            .is_some_and(|value| value.id() != &id || value.taxonomy() != &taxonomy)
        {
            return Err("Typeset identities cannot be changed.".into());
        }
        let description = self.form.parse(2, Description::new)?;
        let schemas = self
            .drafts()
            .into_iter()
            .map(|(name, drafts)| {
                let properties = drafts
                    .iter()
                    .enumerate()
                    .map(|(index, draft)| {
                        draft
                            .definition()
                            .map_err(|error| format!("{name}, property {}: {error}", index + 1))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                ChangeTypeSchema::new(
                    ChangeTypeId::new(name).map_err(|error| error.to_string())?,
                    properties,
                )
                .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let version = self
            .original
            .as_ref()
            .map_or(Some(1), |value| value.version().get().checked_add(1))
            .ok_or("Typeset version is exhausted; fork to a new identity.")?;
        let value = TypesetDefinition::new(
            taxonomy,
            id,
            TypesetVersion::new(version).map_err(|error| error.to_string())?,
            description,
            schemas,
        )
        .map_err(|error| error.to_string())?;
        Ok(vec![if self.original.is_some() {
            ConfigurationEdit::UpdateTypeset(value)
        } else {
            ConfigurationEdit::CreateTypeset(value)
        }])
    }
}

fn schema_drafts(taxonomy: &TaxonomyDefinition, original: Option<&TypesetDefinition>) -> Schemas {
    let mut schemas = Vec::new();
    if let Some(value) = original {
        for schema in value.schemas() {
            if taxonomy
                .change_types()
                .iter()
                .any(|change| change.id() == schema.change_type())
            {
                schemas.push((
                    schema.change_type().to_string(),
                    schema
                        .properties()
                        .iter()
                        .map(PropertyDraft::from_definition)
                        .collect(),
                ));
            }
        }
    }
    for change in taxonomy.change_types() {
        if !schemas.iter().any(|(id, _)| id == change.id().as_str()) {
            schemas.push((change.id().to_string(), Vec::new()));
        }
    }
    schemas
}
