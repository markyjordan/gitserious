//! What Home and Commit say when no usable work tree contains the directory.

use gitserious_app::MissingWorkTree;

/// What to do about `missing`, for a directory where gitserious cannot commit.
pub(crate) fn guidance(missing: &MissingWorkTree) -> String {
    match missing {
        MissingWorkTree::NotRepository => "No Git repository contains this directory.".to_owned(),
        MissingWorkTree::HomeOrRoot => {
            "This is a home directory or a filesystem root, so gitserious will not create a \
             repository here. Open a project folder."
                .to_owned()
        }
        MissingWorkTree::Bare => "This is a bare repository, which has no working tree to \
                                  commit from. Open a clone or a worktree of it."
            .to_owned(),
        MissingWorkTree::InsideGitDir {
            worktree: Some(worktree),
        } => format!(
            "This is inside a repository's .git directory. Its working tree is {}.",
            worktree.display()
        ),
        MissingWorkTree::InsideGitDir { worktree: None } => {
            "This is inside a repository's Git directory. Open its working tree.".to_owned()
        }
        MissingWorkTree::DubiousOwnership { repository } => format!(
            "Git refuses {} because another user owns it. If you trust it, run: git config \
             --global --add safe.directory {}",
            repository.display(),
            repository.display()
        ),
    }
}
