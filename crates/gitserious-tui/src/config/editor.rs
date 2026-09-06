use gitserious_app::ConfigurationSession;
use ratatui::crossterm::event::KeyEvent;

use super::{
    form::{Form, FormAction},
    taxonomy_form::TaxonomyForm,
    template_form::{ForkForm, ImportForm, TemplateForm},
    typeset_form::TypesetForm,
};

pub(super) enum Editor {
    Taxonomy(TaxonomyForm),
    Typeset(TypesetForm),
    Template(TemplateForm),
    Fork(ForkForm),
    Import(ImportForm),
}

impl Editor {
    pub(super) const fn hint(&self) -> &'static str {
        match self {
            Self::Taxonomy(_) | Self::Typeset(_) => {
                "tab/shift+tab: fields | ctrl+n/d: add/remove | alt+↑/↓: order\nctrl+s: stage | esc: back"
            }
            Self::Template(_) | Self::Fork(_) | Self::Import(_) => {
                "tab/shift+tab: fields | ←/→: choices\nctrl+s: stage | esc: back"
            }
        }
    }
    pub(super) fn form(&self) -> &Form {
        match self {
            Self::Taxonomy(editor) => &editor.form,
            Self::Typeset(editor) => &editor.form,
            Self::Template(editor) => &editor.form,
            Self::Fork(editor) => &editor.form,
            Self::Import(editor) => &editor.form,
        }
    }
    pub(super) fn form_mut(&mut self) -> &mut Form {
        match self {
            Self::Taxonomy(editor) => &mut editor.form,
            Self::Typeset(editor) => &mut editor.form,
            Self::Template(editor) => &mut editor.form,
            Self::Fork(editor) => &mut editor.form,
            Self::Import(editor) => &mut editor.form,
        }
    }
    pub(super) fn key(&mut self, key: KeyEvent) -> Result<FormAction, String> {
        match self {
            Self::Taxonomy(editor) => editor.key(key),
            Self::Typeset(editor) => editor.key(key),
            Self::Template(editor) => Ok(editor.key(key)),
            Self::Fork(editor) => Ok(editor.form.key(key)),
            Self::Import(editor) => Ok(editor.form.key(key)),
        }
    }
    pub(super) fn stage(&self, session: &mut ConfigurationSession) -> Result<bool, String> {
        let before = session.custom().clone();
        let active = session.active_template().cloned();
        match self {
            Self::Taxonomy(editor) => session.stage(editor.submit()?)?,
            Self::Typeset(editor) => session.stage(editor.submit()?)?,
            Self::Template(editor) => session.stage(editor.submit()?)?,
            Self::Fork(editor) => editor.stage(session)?,
            Self::Import(editor) => editor.stage(session)?,
        }
        Ok(before != *session.custom() || active.as_ref() != session.active_template())
    }

    pub(super) fn target_identity(&self) -> String {
        match self {
            Self::Taxonomy(editor) => editor.form.fields[0].value(),
            Self::Typeset(editor) => format!(
                "{}/{}",
                editor.form.fields[1].value(),
                editor.form.fields[0].value()
            ),
            Self::Template(editor) => editor.form.fields[0].value(),
            Self::Fork(editor) => editor.form.fields[1].value(),
            Self::Import(editor) => editor.form.fields[0].value(),
        }
    }
}
