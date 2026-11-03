use gitserious_app::{LibraryDraft, NodeId};

/// Name-based lookup for tests; production navigation uses stable node ids.
pub trait LibraryLookup {
    fn find_path<S: AsRef<str>>(&self, names: &[S]) -> Option<NodeId>;
}

impl LibraryLookup for LibraryDraft {
    fn find_path<S: AsRef<str>>(&self, names: &[S]) -> Option<NodeId> {
        let (name, rest) = names.split_first()?;
        let taxonomy = self
            .taxonomies()
            .iter()
            .find(|node| node.name() == name.as_ref())?;
        let Some((name, rest)) = rest.split_first() else {
            return Some(taxonomy.id().clone());
        };
        let commit_type = taxonomy
            .types()
            .iter()
            .find(|node| node.name() == name.as_ref())?;
        match rest {
            [] => Some(commit_type.id().clone()),
            [name] => commit_type
                .properties()
                .iter()
                .find(|node| node.name() == name.as_ref())
                .map(|node| node.id().clone()),
            _ => None,
        }
    }
}
