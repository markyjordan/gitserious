use gitserious_app::ConfigurationEdit;
use gitserious_core::{
    ChangeTypeDefinition, ChangeTypeId, Description, TaxonomyDefinition, TaxonomyId,
    TaxonomyVersion,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::form::{Field, Form, FormAction, Group};

pub(super) struct TaxonomyForm {
    pub form: Form,
    original: Option<TaxonomyDefinition>,
}

impl TaxonomyForm {
    pub(super) fn new(original: Option<TaxonomyDefinition>) -> Self {
        let mut fields = vec![
            Field::new(
                "Taxonomy id",
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
        ];
        if let Some(value) = &original {
            for (index, change) in value.change_types().iter().enumerate() {
                fields.extend(type_fields(
                    index,
                    change.id().as_str(),
                    change.description().as_str(),
                ));
            }
        } else {
            fields.extend(type_fields(0, "", ""));
        }
        Self {
            form: Form::new(
                if original.is_some() {
                    "Edit taxonomy"
                } else {
                    "Create taxonomy"
                },
                fields,
            ),
            original,
        }
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> Result<FormAction, String> {
        if self.form.confirming_discard() {
            return Ok(self.form.key(key));
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('n') => {
                    let index = (self.form.fields.len() - 2) / 2;
                    self.form.focus = self.form.fields.len();
                    self.form.fields.extend(type_fields(index, "", ""));
                    return Ok(FormAction::Continue);
                }
                KeyCode::Char('d') => {
                    self.remove_type()?;
                    return Ok(FormAction::Continue);
                }
                _ => {}
            }
        }
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Up | KeyCode::Down)
        {
            self.move_type(key.code == KeyCode::Down);
            return Ok(FormAction::Continue);
        }
        Ok(self.form.key(key))
    }

    fn remove_type(&mut self) -> Result<(), String> {
        let Group::Item(index) = self.form.fields[self.form.focus].group else {
            return Err("Select a change type field to remove that type.".into());
        };
        if self.form.fields.len() == 4 {
            return Err("A taxonomy needs at least one change type.".into());
        }
        self.form.fields.drain(2 + index * 2..4 + index * 2);
        self.form.focus = self.form.focus.min(self.form.fields.len() - 1);
        self.renumber();
        Ok(())
    }

    fn move_type(&mut self, down: bool) {
        let Group::Item(index) = self.form.fields[self.form.focus].group else {
            return;
        };
        let count = (self.form.fields.len() - 2) / 2;
        let target = if down {
            index.saturating_add(1).min(count - 1)
        } else {
            index.saturating_sub(1)
        };
        for offset in 0..2 {
            self.form
                .fields
                .swap(2 + index * 2 + offset, 2 + target * 2 + offset);
        }
        self.form.focus = 2 + target * 2 + (self.form.focus - 2) % 2;
        self.renumber();
    }

    fn renumber(&mut self) {
        for (offset, field) in self.form.fields[2..].iter_mut().enumerate() {
            field.group = Group::Item(offset / 2);
            field.label = format!(
                "Type {} · {}",
                offset / 2 + 1,
                if offset % 2 == 0 { "id" } else { "meaning" }
            );
        }
    }

    pub(super) fn submit(&self) -> Result<Vec<ConfigurationEdit>, String> {
        if self.original.is_some() && !self.form.is_dirty() {
            return Ok(Vec::new());
        }
        let id = self.form.parse(0, TaxonomyId::new)?;
        if self
            .original
            .as_ref()
            .is_some_and(|value| value.id() != &id)
        {
            return Err("Taxonomy identities cannot be changed.".into());
        }
        let description = self.form.parse(1, Description::new)?;
        let mut types = Vec::new();
        for index in (2..self.form.fields.len()).step_by(2) {
            types.push(ChangeTypeDefinition::new(
                self.form.parse(index, ChangeTypeId::new)?,
                self.form.parse(index + 1, Description::new)?,
            ));
        }
        let version = self
            .original
            .as_ref()
            .map_or(Some(1), |value| value.version().get().checked_add(1))
            .ok_or("Taxonomy version is exhausted; fork to a new identity.")?;
        let value = TaxonomyDefinition::new(
            id,
            TaxonomyVersion::new(version).map_err(|error| error.to_string())?,
            description,
            types,
        )
        .map_err(|error| error.to_string())?;
        Ok(vec![if self.original.is_some() {
            ConfigurationEdit::UpdateTaxonomy(value)
        } else {
            ConfigurationEdit::CreateTaxonomy(value)
        }])
    }
}

fn type_fields(index: usize, id: &str, description: &str) -> [Field; 2] {
    [
        Field::new(
            format!("Type {} · id", index + 1),
            id,
            false,
            false,
            Group::Item(index),
        ),
        Field::new(
            format!("Type {} · meaning", index + 1),
            description,
            true,
            false,
            Group::Item(index),
        ),
    ]
}
