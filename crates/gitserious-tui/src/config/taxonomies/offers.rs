use gitserious_app::{NodeRef, TaxonomyOrigin};

use super::outline::{self, Row};

/// What the selected NAV row lets the user do, in footer order. The footer hints each offer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Offer {
    Open,
    Close,
    /// Enter on the new taxonomy row.
    NewTaxonomy,
    /// Enter on an add type or add property row.
    Add,
    Fork,
    New,
    Edit,
    Copy,
    Delete,
    Reorder,
}

impl Offer {
    pub(super) const fn hint(self) -> (&'static str, &'static str) {
        match self {
            Self::Open => ("enter", "open"),
            Self::Close => ("enter", "close"),
            Self::NewTaxonomy => ("enter", "new"),
            Self::Add => ("enter", "add"),
            Self::Fork => ("f", "fork"),
            Self::New => ("n", "new"),
            Self::Edit => ("e", "edit"),
            Self::Copy => ("c", "copy property to other types"),
            Self::Delete => ("d", "delete"),
            Self::Reorder => ("alt+↑/↓", "reorder"),
        }
    }
}

/// The offers of `row` in NAV. `open` says whether the row's node is expanded.
pub(super) fn offers(row: Row<'_>, read_only: bool, open: bool) -> Vec<Offer> {
    let toggle = || {
        outline::has_children(row, read_only).then_some(if open {
            Offer::Close
        } else {
            Offer::Open
        })
    };
    let node = match row {
        Row::NewTaxonomy => return vec![Offer::NewTaxonomy],
        Row::AddType(_) | Row::AddProperty(..) => return vec![Offer::Add],
        Row::Node(_) if read_only => return toggle().into_iter().collect(),
        Row::Node(node) => node,
    };
    let custom = node.taxonomy().origin() == TaxonomyOrigin::Custom;
    if custom && matches!(node, NodeRef::Property { .. }) {
        return vec![Offer::Edit, Offer::Copy, Offer::Delete];
    }
    let mut offers: Vec<Offer> = toggle().into_iter().collect();
    offers.push(Offer::Fork);
    if custom {
        offers.extend([Offer::New, Offer::Edit, Offer::Delete]);
        if !matches!(node, NodeRef::Taxonomy(_)) {
            offers.push(Offer::Reorder);
        }
    }
    offers
}
