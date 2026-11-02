use gitserious_core::{
    CommitTypeId, PropertyExample, PropertyExampleError, PropertyKey, built_in_sample,
};

use super::{LibraryDraft, PropertyNode, TaxonomyNode, TypeNode, changes::written};

#[derive(Clone, Debug)]
pub(super) enum ExampleDraft {
    LegacyMissing(String),
    Generated(String),
    Authored(String),
}

impl ExampleDraft {
    pub(super) fn text(&self) -> &str {
        match self {
            Self::LegacyMissing(text) | Self::Generated(text) | Self::Authored(text) => text,
        }
    }

    pub(super) fn text_mut(&mut self) -> &mut String {
        match self {
            Self::LegacyMissing(text) | Self::Generated(text) | Self::Authored(text) => text,
        }
    }

    pub(super) fn saved_text(&self) -> Option<&str> {
        match self {
            Self::LegacyMissing(_) => None,
            Self::Generated(text) | Self::Authored(text) => Some(text),
        }
    }

    pub(super) fn build(&self) -> Result<Option<PropertyExample>, PropertyExampleError> {
        self.saved_text().map(PropertyExample::new).transpose()
    }
}

impl LibraryDraft {
    pub(super) fn refresh_implicit_examples(&mut self) {
        let mut updates = Vec::new();
        for (t, taxonomy) in self.taxonomies.iter().enumerate() {
            for (c, commit_type) in taxonomy.types.iter().enumerate() {
                for (p, property) in commit_type.properties.iter().enumerate() {
                    let text = match property.example {
                        ExampleDraft::Authored(_) => continue,
                        ExampleDraft::LegacyMissing(_) => self
                            .sample_example(taxonomy, commit_type, property)
                            .unwrap_or_else(|| template(commit_type, property)),
                        ExampleDraft::Generated(_) => template(commit_type, property),
                    };
                    updates.push((t, c, p, text));
                }
            }
        }
        for (t, c, p, text) in updates {
            *self.taxonomies[t].types[c].properties[p].example.text_mut() = text;
        }
    }

    fn sample_example(
        &self,
        taxonomy: &TaxonomyNode,
        commit_type: &TypeNode,
        property: &PropertyNode,
    ) -> Option<String> {
        std::iter::successors(Some(taxonomy), |node| {
            let source = node.derived_from.as_ref()?.id().as_str();
            self.taxonomies.iter().find(|other| other.name == source)
        })
        .take(self.taxonomies.len())
        .filter(|node| node.origin == crate::TaxonomyOrigin::BuiltIn)
        .find_map(|node| {
            let sample = built_in_sample(&node.name, &commit_type.name)?;
            Some(format!(
                "{}\n{}:\n{}",
                sample.subject(),
                property.name,
                sample.value(&property.name)?
            ))
        })
    }
}

fn template(commit_type: &TypeNode, property: &PropertyNode) -> String {
    let commit_type = if commit_type.name.is_empty() {
        "<type>".into()
    } else {
        written(CommitTypeId::from_label, &commit_type.name)
    };
    let property = if property.name.is_empty() {
        "<property>".into()
    } else {
        written(PropertyKey::from_label, &property.name)
    };
    format!("{commit_type}([<scope>]): <subject>\n{property}:\n<value>")
}
