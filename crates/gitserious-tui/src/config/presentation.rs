use std::collections::BTreeMap;
use std::fmt::Write as _;

use gitserious_app::{
    ConfigurationSession, CustomConfiguration, taxonomy_origin, template_origin, typeset_origin,
};
use gitserious_core::{
    PropertyRequirement, TaxonomyDefinition, TemplateDefinition, TypesetDefinition,
    built_in_configuration,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Taxonomy,
    Typeset,
    Template,
}

impl Kind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Taxonomy => "Taxonomies",
            Self::Typeset => "Typesets",
            Self::Template => "Templates",
        }
    }
}

#[derive(Clone)]
pub(super) enum Definition {
    Taxonomy(TaxonomyDefinition),
    Typeset(TypesetDefinition),
    Template(TemplateDefinition),
}

impl Definition {
    pub(super) fn is_builtin(&self) -> bool {
        let origin = match self {
            Self::Taxonomy(value) => taxonomy_origin(value.id()),
            Self::Typeset(value) => typeset_origin(value.taxonomy(), value.id()),
            Self::Template(value) => template_origin(value.id()),
        };
        origin == gitserious_app::ConfigurationOrigin::BuiltIn
    }
    pub(super) fn identity(&self) -> String {
        match self {
            Self::Taxonomy(value) => value.id().to_string(),
            Self::Typeset(value) => format!("{}/{}", value.taxonomy(), value.id()),
            Self::Template(value) => value.id().to_string(),
        }
    }

    pub(super) fn label(&self) -> String {
        let (version, origin) = match self {
            Self::Taxonomy(value) => (value.version().get(), taxonomy_origin(value.id())),
            Self::Typeset(value) => (
                value.version().get(),
                typeset_origin(value.taxonomy(), value.id()),
            ),
            Self::Template(value) => (value.version().get(), template_origin(value.id())),
        };
        format!("{}  {} v{version}", self.identity(), origin.as_str())
    }

    pub(super) fn describe(&self) -> String {
        let mut text = format!("{}\n\n", self.label());
        match self {
            Self::Taxonomy(value) => {
                let _ = writeln!(text, "{}\n\nChange types (in order):", value.description());
                for change in value.change_types() {
                    let _ = writeln!(text, "\n{}\n{}", change.id(), change.description());
                }
            }
            Self::Typeset(value) => {
                let _ = writeln!(
                    text,
                    "{}\n\nTaxonomy: {}",
                    value.description(),
                    value.taxonomy()
                );
                for schema in value.schemas() {
                    let _ = writeln!(text, "\n{}", schema.change_type());
                    if schema.properties().is_empty() {
                        text.push_str("  No properties\n");
                    }
                    for property in schema.properties() {
                        let requirement = match property.requirement() {
                            PropertyRequirement::Required => "required",
                            PropertyRequirement::Recommended => "recommended",
                            PropertyRequirement::Optional => "optional",
                            PropertyRequirement::Conditional(_) => "conditional",
                        };
                        let _ = writeln!(
                            text,
                            "  {} ({requirement}, {:?})\n    {}",
                            property.key(),
                            property.multiplicity(),
                            property.description()
                        );
                        if let PropertyRequirement::Conditional(condition) = property.requirement()
                        {
                            let _ = writeln!(
                                text,
                                "    Condition {}: {}",
                                condition.id(),
                                condition.rationale()
                            );
                        }
                    }
                }
            }
            Self::Template(value) => {
                let _ = writeln!(
                    text,
                    "{}\n\nTaxonomy: {}\nTypeset: {}",
                    value.description(),
                    value.taxonomy(),
                    value.typeset()
                );
            }
        }
        text
    }
}

pub(super) fn entries(custom: &CustomConfiguration, kind: Kind) -> Vec<Definition> {
    let built = built_in_configuration();
    let mut result: Vec<_> = match kind {
        Kind::Taxonomy => built
            .taxonomies()
            .iter()
            .chain(custom.taxonomies())
            .cloned()
            .map(Definition::Taxonomy)
            .collect(),
        Kind::Typeset => built
            .typesets()
            .iter()
            .chain(custom.typesets())
            .cloned()
            .map(Definition::Typeset)
            .collect(),
        Kind::Template => built
            .templates()
            .iter()
            .chain(custom.templates())
            .cloned()
            .map(Definition::Template)
            .collect(),
    };
    result.sort_by_key(Definition::identity);
    result
}

pub(super) fn review(session: &ConfigurationSession) -> String {
    let mut text = String::new();
    if session.active_template() != session.original_active_template() {
        let _ = writeln!(
            text,
            "Project default: {} -> {}\n",
            session
                .original_active_template()
                .map_or("none", |id| id.as_str()),
            session.active_template().map_or("none", |id| id.as_str())
        );
    }
    for kind in [Kind::Taxonomy, Kind::Typeset, Kind::Template] {
        let before: BTreeMap<_, _> = entries(session.original(), kind)
            .into_iter()
            .map(|entry| (entry.identity(), entry.describe()))
            .collect();
        let after: BTreeMap<_, _> = entries(session.custom(), kind)
            .into_iter()
            .map(|entry| (entry.identity(), entry.describe()))
            .collect();
        for id in before
            .keys()
            .chain(after.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            if before.get(id) == after.get(id) {
                continue;
            }
            let _ = writeln!(
                text,
                "{} / {}\n\nBEFORE\n{}\nAFTER\n{}\n",
                kind.label(),
                id,
                before.get(id).map_or("Absent", String::as_str),
                after.get(id).map_or("Absent", String::as_str)
            );
        }
    }
    text
}
