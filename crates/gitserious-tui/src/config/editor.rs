use gitserious_app::ConfigurationEdit;
use ratatui::crossterm::event::KeyEvent;

use super::{
    form::{Form, FormAction},
    taxonomy_form::TaxonomyForm,
    typeset_form::TypesetForm,
};

pub(super) enum Editor {
    Taxonomy(TaxonomyForm),
    Typeset(TypesetForm),
}

impl Editor {
    pub(super) fn form(&self) -> &Form {
        match self {
            Self::Taxonomy(editor) => &editor.form,
            Self::Typeset(editor) => &editor.form,
        }
    }
    pub(super) fn form_mut(&mut self) -> &mut Form {
        match self {
            Self::Taxonomy(editor) => &mut editor.form,
            Self::Typeset(editor) => &mut editor.form,
        }
    }
    pub(super) fn key(&mut self, key: KeyEvent) -> Result<FormAction, String> {
        match self {
            Self::Taxonomy(editor) => editor.key(key),
            Self::Typeset(editor) => editor.key(key),
        }
    }
    pub(super) fn submit(&self) -> Result<Vec<ConfigurationEdit>, String> {
        match self {
            Self::Taxonomy(editor) => editor.submit(),
            Self::Typeset(editor) => editor.submit(),
        }
    }
}
