use std::path::Path;
use std::process::Command;

use gitserious_app::{FileEditor, RepositoryRoot};

/// Opens files in the editor `git var GIT_EDITOR` names, through a shell as Git does.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitEditor;

impl FileEditor for GitEditor {
    fn edit(&self, root: &RepositoryRoot, path: &Path) -> Result<(), String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root.as_path())
            .args(["var", "GIT_EDITOR"])
            .output()
            .map_err(|error| format!("could not execute git var: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        let editor = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        // Git runs the editor through `sh -c "<editor> \"$@\""`, so arguments and quoting in
        // core.editor work the same here.
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{editor} \"$@\""))
            .arg(&editor)
            .arg(path)
            .current_dir(root.as_path())
            .status()
            .map_err(|error| format!("could not start the editor {editor}: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("the editor {editor} exited with {status}"))
        }
    }
}
