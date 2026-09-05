use gitserious_app::{ConfigurationCatalog, ConfigurationEdit, ConfigurationSession};
use gitserious_core::{
    Description, TaxonomyDefinition, TaxonomyId, TemplateDefinition, TemplateId, TemplateVersion,
    TypesetDefinition, TypesetId,
};
use ratatui::crossterm::event::KeyEvent;

use super::form::{Field, Form, FormAction, Group};

pub(super) struct TemplateForm {
    pub form: Form,
    original: Option<TemplateDefinition>,
    typesets: Vec<TypesetDefinition>,
}

impl TemplateForm {
    pub(super) fn new(
        taxonomies: &[TaxonomyDefinition],
        typesets: Vec<TypesetDefinition>,
        original: Option<TemplateDefinition>,
    ) -> Result<Self, String> {
        let taxonomy = original
            .as_ref()
            .map(|value| value.taxonomy().as_str())
            .or_else(|| taxonomies.first().map(|value| value.id().as_str()))
            .ok_or("Create a taxonomy first.")?;
        let mut fields = vec![
            Field::new(
                "Template id",
                original.as_ref().map_or("", |value| value.id().as_str()),
                false,
                original.is_some(),
                Group::Metadata,
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
            Field::new("Taxonomy", taxonomy, false, false, Group::Metadata).choices(
                taxonomies
                    .iter()
                    .map(|value| value.id().to_string())
                    .collect(),
            ),
            Field::new(
                "Typeset",
                original
                    .as_ref()
                    .map_or("", |value| value.typeset().as_str()),
                false,
                false,
                Group::Metadata,
            ),
        ];
        set_typesets(&mut fields, &typesets, original.is_some());
        Ok(Self {
            form: Form::new(
                if original.is_some() {
                    "Edit template"
                } else {
                    "Create template"
                },
                fields,
            ),
            original,
            typesets,
        })
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> FormAction {
        let taxonomy = self.form.fields[2].value();
        let result = self.form.key(key);
        if taxonomy != self.form.fields[2].value() {
            set_typesets(&mut self.form.fields, &self.typesets, false);
        }
        result
    }

    pub(super) fn submit(&self) -> Result<Vec<ConfigurationEdit>, String> {
        if self.original.is_some() && !self.form.is_dirty() {
            return Ok(Vec::new());
        }
        let id = self.form.parse(0, TemplateId::new)?;
        if self
            .original
            .as_ref()
            .is_some_and(|value| value.id() != &id)
        {
            return Err("Template identities cannot be changed.".into());
        }
        let description = self.form.parse(1, Description::new)?;
        let taxonomy = self.form.parse(2, TaxonomyId::new)?;
        let typeset = self.form.parse(3, TypesetId::new)?;
        let version = self
            .original
            .as_ref()
            .map_or(Some(1), |value| value.version().get().checked_add(1))
            .ok_or("Template version is exhausted; fork to a new identity.")?;
        let value = TemplateDefinition::new(
            id,
            TemplateVersion::new(version).map_err(|error| error.to_string())?,
            description,
            taxonomy,
            typeset,
        );
        Ok(vec![if self.original.is_some() {
            ConfigurationEdit::UpdateTemplate(value)
        } else {
            ConfigurationEdit::CreateTemplate(value)
        }])
    }
}

fn set_typesets(fields: &mut [Field], typesets: &[TypesetDefinition], preserve: bool) {
    let taxonomy = fields[2].value();
    let choices: Vec<_> = typesets
        .iter()
        .filter(|value| value.taxonomy().as_str() == taxonomy)
        .map(|value| value.id().to_string())
        .collect();
    if !preserve && !choices.contains(&fields[3].value()) {
        fields[3].set_value(choices.first().map_or("", String::as_str));
    }
    fields[3].readonly = choices.is_empty();
    fields[3].options = choices;
}

pub(super) struct ForkForm {
    pub form: Form,
}

impl ForkForm {
    pub(super) fn new(source: &str, catalog: &ConfigurationCatalog) -> Self {
        Self {
            form: Form::new(
                "Fork template bundle",
                vec![
                    Field::new("Source template", source, false, false, Group::Metadata).choices(
                        catalog
                            .templates()
                            .iter()
                            .map(|value| value.id().to_string())
                            .collect(),
                    ),
                    Field::new("New template id", "", false, false, Group::Metadata),
                    Field::new(
                        "New taxonomy id (blank: template-taxonomy)",
                        "",
                        false,
                        false,
                        Group::Metadata,
                    ),
                    Field::new(
                        "New typeset id (blank: template-typeset)",
                        "",
                        false,
                        false,
                        Group::Metadata,
                    ),
                ],
            ),
        }
    }

    pub(super) fn stage(&self, session: &mut ConfigurationSession) -> Result<(), String> {
        let source = self.form.parse(0, TemplateId::new)?;
        let template = self.form.parse(1, TemplateId::new)?;
        let taxonomy = self.form.fields[2].value();
        let typeset = self.form.fields[3].value();
        let taxonomy = TaxonomyId::new(if taxonomy.is_empty() {
            format!("{template}-taxonomy")
        } else {
            taxonomy
        })
        .map_err(|error| format!("New taxonomy id: {error}"))?;
        let typeset = TypesetId::new(if typeset.is_empty() {
            format!("{template}-typeset")
        } else {
            typeset
        })
        .map_err(|error| format!("New typeset id: {error}"))?;
        session.fork_template(&source, &template, &taxonomy, &typeset)
    }
}

pub(super) struct ImportForm {
    pub form: Form,
    source: ConfigurationCatalog,
}

impl ImportForm {
    pub(super) fn new(source: ConfigurationCatalog) -> Result<Self, String> {
        let first = source
            .templates()
            .first()
            .ok_or("No global templates are available.")?
            .id()
            .to_string();
        let fields = vec![
            Field::new("Global template", &first, false, false, Group::Metadata).choices(
                source
                    .templates()
                    .iter()
                    .map(|value| value.id().to_string())
                    .collect(),
            ),
            Field::new(
                "Select as project default",
                "no",
                false,
                false,
                Group::Metadata,
            )
            .choices(vec!["no".into(), "yes".into()]),
        ];
        Ok(Self {
            form: Form::new("Import global template", fields),
            source,
        })
    }

    pub(super) fn stage(&self, session: &mut ConfigurationSession) -> Result<(), String> {
        let template = self.form.parse(0, TemplateId::new)?;
        let select = match self.form.fields[1].value().as_str() {
            "yes" => true,
            "no" => false,
            _ => return Err("Choose yes or no for project default selection.".into()),
        };
        session.import_template(&self.source, &template, select)
    }
}
