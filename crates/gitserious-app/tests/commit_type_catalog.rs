use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_app::{CommitTypeCatalog, find_commit_type, list_commit_types};
use gitserious_core::{CommitTypeDefinition, CommitTypeId, built_in_commit_types};

#[derive(Clone, Debug, Eq, PartialEq)]
enum FakeError {
    Unavailable,
}

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("catalog unavailable")
    }
}

impl Error for FakeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Request {
    Find(CommitTypeId),
    List,
}

struct RecordingCatalog {
    definitions: Vec<CommitTypeDefinition>,
    error: Option<FakeError>,
    requests: RefCell<Vec<Request>>,
}

impl RecordingCatalog {
    fn available() -> Self {
        Self {
            definitions: built_in_commit_types().to_vec(),
            error: None,
            requests: RefCell::new(Vec::new()),
        }
    }

    fn failing() -> Self {
        Self {
            definitions: Vec::new(),
            error: Some(FakeError::Unavailable),
            requests: RefCell::new(Vec::new()),
        }
    }
}

impl CommitTypeCatalog for RecordingCatalog {
    type Error = FakeError;

    fn find(&self, id: &CommitTypeId) -> Result<Option<CommitTypeDefinition>, Self::Error> {
        self.requests.borrow_mut().push(Request::Find(id.clone()));
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        Ok(self
            .definitions
            .iter()
            .find(|definition| definition.id() == id)
            .cloned())
    }

    fn list(&self) -> Result<Vec<CommitTypeDefinition>, Self::Error> {
        self.requests.borrow_mut().push(Request::List);
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        Ok(self.definitions.clone())
    }
}

#[test]
fn find_use_case_returns_a_match_and_forwards_the_exact_id() -> Result<(), Box<dyn Error>> {
    let catalog = RecordingCatalog::available();
    let id = CommitTypeId::new("fix")?;

    let found = find_commit_type(&catalog, &id)?;

    assert_eq!(
        found.as_ref().map(|definition| definition.id().as_str()),
        Some("fix")
    );
    assert_eq!(catalog.requests.borrow().as_slice(), [Request::Find(id)]);

    Ok(())
}

#[test]
fn find_use_case_preserves_a_successful_not_found_result() -> Result<(), Box<dyn Error>> {
    let catalog = RecordingCatalog::available();
    let id = CommitTypeId::new("custom")?;

    let found = find_commit_type(&catalog, &id)?;

    assert_eq!(found, None);
    assert_eq!(catalog.requests.borrow().as_slice(), [Request::Find(id)]);

    Ok(())
}

#[test]
fn list_use_case_preserves_catalog_order_and_calls_only_list() -> Result<(), Box<dyn Error>> {
    let catalog = RecordingCatalog::available();

    let listed = list_commit_types(&catalog)?;

    assert_eq!(listed, built_in_commit_types());
    assert_eq!(catalog.requests.borrow().as_slice(), [Request::List]);

    Ok(())
}

#[test]
fn use_cases_return_adapter_errors_unchanged() -> Result<(), Box<dyn Error>> {
    let catalog = RecordingCatalog::failing();
    let id = CommitTypeId::new("feat")?;

    assert_eq!(find_commit_type(&catalog, &id), Err(FakeError::Unavailable));
    assert_eq!(list_commit_types(&catalog), Err(FakeError::Unavailable));
    assert_eq!(
        catalog.requests.borrow().as_slice(),
        [Request::Find(id), Request::List]
    );

    Ok(())
}
