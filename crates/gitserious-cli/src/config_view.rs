//! Presentation of configuration catalog queries for command delivery.

use std::fmt::Write;

use gitserious_app::{ConfigurationCatalog, taxonomy_origin, template_origin, typeset_origin};
use gitserious_core::{
    PropertyRequirement, TaxonomyDefinition, TemplateDefinition, TypesetDefinition,
};

/// The entity kinds exposed by configuration commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationKind {
    /// One reusable taxonomy of change types.
    Taxonomy,
    /// One taxonomy-scoped durable-property typeset.
    Typeset,
    /// One reusable selection of a taxonomy and typeset.
    Template,
}

impl ConfigurationKind {
    /// Returns the plural section heading used by listings.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Taxonomy => "TAXONOMIES",
            Self::Typeset => "TYPESETS",
            Self::Template => "TEMPLATES",
        }
    }
}

/// Renders every effective definition grouped by entity kind.
#[must_use]
pub fn render_list(catalog: &ConfigurationCatalog) -> String {
    ALL_KINDS
        .iter()
        .map(|kind| render_list_kind(catalog, *kind))
        .collect()
}

/// Every entity kind in listing order.
const ALL_KINDS: [ConfigurationKind; 3] = [
    ConfigurationKind::Taxonomy,
    ConfigurationKind::Typeset,
    ConfigurationKind::Template,
];

/// Renders one entity kind's definitions under its section heading.
#[must_use]
pub fn render_list_kind(catalog: &ConfigurationCatalog, kind: ConfigurationKind) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "{}", kind.heading());
    match kind {
        ConfigurationKind::Taxonomy => {
            for taxonomy in catalog.taxonomies() {
                let _ = writeln!(
                    output,
                    "  {}",
                    summary(
                        taxonomy.id().as_str(),
                        taxonomy_origin(taxonomy.id()).as_str(),
                        taxonomy.version().get(),
                        taxonomy.description().as_str()
                    )
                );
            }
        }
        ConfigurationKind::Typeset => {
            for typeset in catalog.typesets() {
                let _ = writeln!(
                    output,
                    "  {}",
                    summary(
                        &format!("{}/{}", typeset.taxonomy(), typeset.id()),
                        typeset_origin(typeset.taxonomy(), typeset.id()).as_str(),
                        typeset.version().get(),
                        typeset.description().as_str()
                    )
                );
            }
        }
        ConfigurationKind::Template => {
            for template in catalog.templates() {
                let _ = writeln!(
                    output,
                    "  {}",
                    summary(
                        template.id().as_str(),
                        template_origin(template.id()).as_str(),
                        template.version().get(),
                        &format!("{} / {}", template.taxonomy(), template.typeset())
                    )
                );
            }
        }
    }
    output
}

/// Renders one taxonomy with its ordered change types.
#[must_use]
pub fn render_taxonomy(taxonomy: &TaxonomyDefinition) -> String {
    let mut output = String::new();
    let _ = writeln!(
        output,
        "taxonomy {} ({})",
        taxonomy.id(),
        taxonomy_origin(taxonomy.id()).as_str()
    );
    let _ = writeln!(output, "version: {}", taxonomy.version().get());
    let _ = writeln!(output, "{}", taxonomy.description());
    let _ = writeln!(output, "change types:");
    for change_type in taxonomy.change_types() {
        let _ = writeln!(
            output,
            "  {}  {}",
            change_type.id(),
            change_type.description()
        );
    }
    output
}

/// Renders one typeset with every schema and property definition.
#[must_use]
pub fn render_typeset(typeset: &TypesetDefinition) -> String {
    let mut output = String::new();
    let _ = writeln!(
        output,
        "typeset {}/{} ({})",
        typeset.taxonomy(),
        typeset.id(),
        typeset_origin(typeset.taxonomy(), typeset.id()).as_str()
    );
    let _ = writeln!(output, "version: {}", typeset.version().get());
    let _ = writeln!(output, "{}", typeset.description());
    let _ = writeln!(output, "schemas:");
    for schema in typeset.schemas() {
        let _ = writeln!(output, "  {}", schema.change_type());
        for property in schema.properties() {
            let _ = writeln!(
                output,
                "    {}  {}  {}  {}",
                property.key(),
                requirement(property.requirement()),
                multiplicity(property.multiplicity()),
                property.description()
            );
        }
    }
    output
}

/// Renders one template with its selection and resolution size.
#[must_use]
pub fn render_template(template: &TemplateDefinition, catalog: &ConfigurationCatalog) -> String {
    let resolved = catalog.resolve(template.id());
    let coverage = resolved.map_or_else(
        |_| String::from("unavailable"),
        |taxonomy| format!("{} change types", taxonomy.change_types().len()),
    );
    let mut output = String::new();
    let _ = writeln!(
        output,
        "template {} ({})",
        template.id(),
        template_origin(template.id()).as_str()
    );
    let _ = writeln!(output, "version: {}", template.version().get());
    let _ = writeln!(output, "{}", template.description());
    let _ = writeln!(
        output,
        "selects taxonomy {} with typeset {}",
        template.taxonomy(),
        template.typeset()
    );
    let _ = writeln!(output, "resolves to {coverage}");
    output
}

fn summary(identity: &str, origin: &str, version: u16, detail: &str) -> String {
    format!("{identity}  {origin} v{version}  {detail}")
}

fn requirement(requirement: &PropertyRequirement) -> String {
    match requirement {
        PropertyRequirement::Required => String::from("required"),
        PropertyRequirement::Recommended => String::from("recommended"),
        PropertyRequirement::Optional => String::from("optional"),
        PropertyRequirement::Conditional(condition) => {
            format!("conditional({})", condition.id())
        }
    }
}

const fn multiplicity(multiplicity: gitserious_core::PropertyMultiplicity) -> &'static str {
    match multiplicity {
        gitserious_core::PropertyMultiplicity::Single => "single",
        gitserious_core::PropertyMultiplicity::Multiple => "multiple",
    }
}
