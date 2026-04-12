use git2::{Oid, Repository, Signature};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A temporary git repository for testing.
///
/// Creates a real git repo in a temp directory. The directory is automatically
/// cleaned up when `TestRepo` is dropped.
pub struct TestRepo {
    _tempdir: TempDir,
    path: PathBuf,
    repo: Repository,
}

impl Default for TestRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRepo {
    /// Create a new empty git repository in a temporary directory.
    pub fn new() -> Self {
        let tempdir = TempDir::new().expect("failed to create temp dir");
        let path = tempdir.path().to_path_buf();
        let repo = Repository::init(&path).expect("failed to init repo");

        // Set up default author config so commits work without global git config
        {
            let mut config = repo.config().expect("failed to get repo config");
            config
                .set_str("user.name", "Test Author")
                .expect("failed to set user.name");
            config
                .set_str("user.email", "test@example.com")
                .expect("failed to set user.email");
        }

        TestRepo {
            _tempdir: tempdir,
            path,
            repo,
        }
    }

    /// Write a file relative to the repository root.
    ///
    /// Creates parent directories as needed.
    pub fn write_file(&self, relative_path: &str, content: &str) {
        let full_path = self.path.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dirs");
        }
        fs::write(&full_path, content).expect("failed to write file");
    }

    /// Stage all changes and create a commit with the given message.
    ///
    /// Returns the commit OID.
    pub fn add_and_commit(&self, message: &str) -> Oid {
        let mut index = self.repo.index().expect("failed to get index");
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .expect("failed to add files to index");
        index.write().expect("failed to write index");

        let tree_oid = index.write_tree().expect("failed to write tree");
        let tree = self.repo.find_tree(tree_oid).expect("failed to find tree");

        let sig =
            Signature::now("Test Author", "test@example.com").expect("failed to create signature");

        let parent_commit = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());

        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .expect("failed to create commit")
    }

    /// Create a lightweight tag at HEAD with the given name.
    pub fn create_tag(&self, name: &str) {
        let head = self
            .repo
            .head()
            .expect("failed to get HEAD")
            .peel_to_commit()
            .expect("failed to peel HEAD to commit");
        self.repo
            .tag_lightweight(name, head.as_object(), false)
            .expect("failed to create tag");
    }

    /// Write a config file (synthase-config.json) file in the repo root.
    pub fn write_config(&self, config: &serde_json::Value) {
        let content = serde_json::to_string_pretty(config).expect("failed to serialize config");
        self.write_file("synthase-config.json", &content);
    }

    /// Write a manifest file (.synthase-manifest.json) file in the repo root.
    pub fn write_manifest(&self, manifest: &serde_json::Value) {
        let content = serde_json::to_string_pretty(manifest).expect("failed to serialize manifest");
        self.write_file(".synthase-manifest.json", &content);
    }

    /// Returns the working directory path of the repository.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns a reference to the underlying `git2::Repository`.
    pub fn repo(&self) -> &Repository {
        &self.repo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test_create_repo_commit_and_tag() {
        let test_repo = TestRepo::new();

        // Repo should exist and be empty (no HEAD yet)
        assert!(test_repo.repo().is_empty().unwrap());

        // Write a file and commit
        test_repo.write_file("README.md", "# Hello");
        let oid = test_repo.add_and_commit("feat: initial commit");

        // Repo should no longer be empty
        assert!(!test_repo.repo().is_empty().unwrap());

        // Commit should be findable
        let commit = test_repo.repo().find_commit(oid).unwrap();
        assert_eq!(commit.message().unwrap(), "feat: initial commit");

        // Create a tag and verify it exists
        test_repo.create_tag("v1.0.0");
        let tag_ref = test_repo.repo().find_reference("refs/tags/v1.0.0").unwrap();
        let tag_target = tag_ref.peel_to_commit().unwrap();
        assert_eq!(tag_target.id(), oid);
    }

    #[test]
    fn smoke_test_multiple_commits() {
        let test_repo = TestRepo::new();

        test_repo.write_file("file1.txt", "content1");
        let oid1 = test_repo.add_and_commit("feat: first feature");

        test_repo.write_file("file2.txt", "content2");
        let oid2 = test_repo.add_and_commit("fix: first fix");

        // Both commits should exist and be different
        assert_ne!(oid1, oid2);

        // Second commit should have first as parent
        let commit2 = test_repo.repo().find_commit(oid2).unwrap();
        assert_eq!(commit2.parent(0).unwrap().id(), oid1);
    }

    #[test]
    fn smoke_test_write_config_and_manifest() {
        let test_repo = TestRepo::new();

        let config = serde_json::json!({
            "release-type": "rust",
            "packages": {
                ".": {}
            }
        });
        test_repo.write_config(&config);

        let manifest = serde_json::json!({
            ".": "1.0.0"
        });
        test_repo.write_manifest(&manifest);

        // Verify files exist and are valid JSON
        let config_content =
            fs::read_to_string(test_repo.path().join("synthase-config.json")).unwrap();
        let parsed_config: serde_json::Value = serde_json::from_str(&config_content).unwrap();
        assert_eq!(parsed_config["release-type"], "rust");

        let manifest_content =
            fs::read_to_string(test_repo.path().join(".synthase-manifest.json")).unwrap();
        let parsed_manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
        assert_eq!(parsed_manifest["."], "1.0.0");
    }

    #[test]
    fn smoke_test_subdirectory_files() {
        let test_repo = TestRepo::new();

        test_repo.write_file("packages/foo/src/lib.rs", "fn main() {}");
        test_repo.write_file("packages/bar/Cargo.toml", "[package]\nname = \"bar\"");
        let oid = test_repo.add_and_commit("feat: add workspace packages");

        // Files should exist on disk
        assert!(test_repo.path().join("packages/foo/src/lib.rs").exists());
        assert!(test_repo.path().join("packages/bar/Cargo.toml").exists());

        // Commit should include the files
        let commit = test_repo.repo().find_commit(oid).unwrap();
        let tree = commit.tree().unwrap();
        assert!(tree.get_path(Path::new("packages/foo/src/lib.rs")).is_ok());
        assert!(tree.get_path(Path::new("packages/bar/Cargo.toml")).is_ok());
    }
}
