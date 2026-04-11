use git2::{Oid, Repository, Sort};
use semver::Version;
use std::collections::HashMap;

use crate::error::Result;
use crate::tag::TagName;

/// A commit extracted from git history.
#[derive(Debug, Clone)]
pub struct GitCommit {
    /// The commit SHA hex string.
    pub sha: String,
    /// The full commit message.
    pub message: String,
    /// File paths changed in this commit, relative to the repo root.
    pub files: Vec<String>,
}

/// A parsed release tag from the repository.
#[derive(Debug, Clone)]
pub struct ReleaseTag {
    /// The parsed tag name (component, version, separator, v-prefix).
    pub tag: TagName,
    /// The commit SHA the tag points to.
    pub sha: String,
}

impl ReleaseTag {
    /// The full tag name string.
    pub fn name(&self) -> String {
        self.tag.to_string()
    }

    /// The parsed semver version.
    pub fn version(&self) -> &Version {
        &self.tag.version
    }

    /// The component name, if present in the tag.
    pub fn component(&self) -> Option<&str> {
        self.tag.component.as_deref()
    }
}

/// Walk commits from HEAD backwards, collecting commit info.
///
/// If `stop_at` is provided, stops walking when that SHA is encountered (exclusive).
/// Returns commits in reverse chronological order (newest first).
pub fn walk_commits(repo: &Repository, stop_at: Option<&str>) -> Result<Vec<GitCommit>> {
    let head = repo.head()?.peel_to_commit()?;
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    revwalk.push(head.id())?;

    let stop_oid = stop_at.and_then(|s| Oid::from_str(s).ok());

    let mut commits = Vec::new();
    for oid_result in revwalk {
        let oid = oid_result?;

        // Stop if we've reached the boundary commit
        if stop_oid == Some(oid) {
            break;
        }

        let commit = repo.find_commit(oid)?;
        let message = commit.message().unwrap_or("").to_string();
        let files = diff_commit_files(repo, &commit)?;

        commits.push(GitCommit {
            sha: oid.to_string(),
            message,
            files,
        });
    }

    Ok(commits)
}

/// Compute the list of files changed in a commit by diffing against its parent.
///
/// For the initial commit (no parent), diffs against an empty tree.
fn diff_commit_files(repo: &Repository, commit: &git2::Commit) -> Result<Vec<String>> {
    let tree = commit.tree()?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;

    let mut files = Vec::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            files.push(path.to_string_lossy().to_string());
        } else if let Some(path) = delta.old_file().path() {
            files.push(path.to_string_lossy().to_string());
        }
    }

    Ok(files)
}

/// Find all tags in the repository and parse those that look like version tags.
pub fn find_tags(repo: &Repository) -> Result<Vec<ReleaseTag>> {
    let mut tags = Vec::new();
    let tag_names = repo.tag_names(None)?;

    for name in tag_names.iter().flatten() {
        if let Some(tag) = parse_tag(repo, name)? {
            tags.push(tag);
        }
    }

    Ok(tags)
}

/// Try to parse a tag name as a release tag.
fn parse_tag(repo: &Repository, name: &str) -> Result<Option<ReleaseTag>> {
    let tag = match TagName::parse(name) {
        Some(t) => t,
        None => return Ok(None),
    };

    // Resolve the tag to a commit SHA
    let reference = repo.find_reference(&format!("refs/tags/{name}"))?;
    let commit = reference.peel_to_commit()?;

    Ok(Some(ReleaseTag {
        tag,
        sha: commit.id().to_string(),
    }))
}

/// Split commits into per-package buckets based on changed file paths.
///
/// Each commit is assigned to the package whose configured path is the longest
/// prefix match for any of the commit's changed files. A commit may appear in
/// multiple packages if it touches files in multiple paths.
///
/// Commits that don't match any configured path are collected under the
/// root path `"."` if it's in the path list.
pub fn split_commits_by_path<'a>(
    commits: &'a [GitCommit],
    paths: &[&str],
) -> HashMap<String, Vec<&'a GitCommit>> {
    // Sort paths longest-first for greedy matching
    let mut sorted_paths: Vec<&str> = paths.to_vec();
    sorted_paths.sort_by_key(|b| std::cmp::Reverse(b.len()));

    let mut result: HashMap<String, Vec<&GitCommit>> = HashMap::new();
    for path in paths {
        result.insert(path.to_string(), Vec::new());
    }

    for commit in commits {
        let mut matched_paths = std::collections::HashSet::new();

        for file in &commit.files {
            let mut found = false;
            for path in &sorted_paths {
                if *path == "." {
                    // Root path matches everything — but only as a fallback
                    continue;
                }
                let prefix = if path.ends_with('/') {
                    path.to_string()
                } else {
                    format!("{path}/")
                };
                if file.starts_with(&prefix) || file == *path {
                    matched_paths.insert(path.to_string());
                    found = true;
                    break; // longest match wins per file
                }
            }

            // If no specific path matched and "." is configured, assign to root
            if !found && paths.contains(&".") {
                matched_paths.insert(".".to_string());
            }
        }

        for path in matched_paths {
            if let Some(bucket) = result.get_mut(&path as &str) {
                bucket.push(commit);
            }
        }
    }

    result
}

/// Find the latest release tag for a given component.
///
/// When `include_component_in_tag` is true, matches tags with the given component.
/// When false, matches tags with no component.
pub fn find_latest_tag_for_component<'a>(
    tags: &'a [ReleaseTag],
    component: Option<&str>,
    include_component_in_tag: bool,
) -> Option<&'a ReleaseTag> {
    tags.iter()
        .filter(|tag| {
            if include_component_in_tag {
                tag.component() == component
            } else {
                tag.component().is_none()
            }
        })
        .max_by(|a, b| a.version().cmp(b.version()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestRepo;

    #[test]
    fn test_walk_commits_basic() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "hello");
        let oid1 = test_repo.add_and_commit("feat: first commit");
        test_repo.write_file("file2.txt", "world");
        let oid2 = test_repo.add_and_commit("fix: second commit");

        let commits = walk_commits(test_repo.repo(), None).unwrap();
        assert_eq!(commits.len(), 2);
        // Newest first
        assert_eq!(commits[0].sha, oid2.to_string());
        assert_eq!(commits[1].sha, oid1.to_string());
    }

    #[test]
    fn test_walk_commits_stop_at() {
        let test_repo = TestRepo::new();
        test_repo.write_file("a.txt", "a");
        let oid1 = test_repo.add_and_commit("feat: first");
        test_repo.write_file("b.txt", "b");
        test_repo.add_and_commit("fix: second");
        test_repo.write_file("c.txt", "c");
        test_repo.add_and_commit("chore: third");

        let commits = walk_commits(test_repo.repo(), Some(&oid1.to_string())).unwrap();
        // Should stop before oid1, so only 2 commits (third, second)
        assert_eq!(commits.len(), 2);
    }

    #[test]
    fn test_walk_commits_file_changes() {
        let test_repo = TestRepo::new();
        test_repo.write_file("src/lib.rs", "fn main() {}");
        test_repo.write_file("README.md", "# Hello");
        test_repo.add_and_commit("feat: initial");

        let commits = walk_commits(test_repo.repo(), None).unwrap();
        assert_eq!(commits.len(), 1);
        let files = &commits[0].files;
        assert!(files.contains(&"src/lib.rs".to_string()));
        assert!(files.contains(&"README.md".to_string()));
    }

    #[test]
    fn test_find_tags() {
        let test_repo = TestRepo::new();
        test_repo.write_file("a.txt", "a");
        test_repo.add_and_commit("feat: initial");
        test_repo.create_tag("v1.0.0");

        test_repo.write_file("b.txt", "b");
        test_repo.add_and_commit("feat: second");
        test_repo.create_tag("v1.1.0");

        let tags = find_tags(test_repo.repo()).unwrap();
        assert_eq!(tags.len(), 2);

        let v100 = tags.iter().find(|t| t.name() == "v1.0.0").unwrap();
        assert_eq!(*v100.version(), Version::new(1, 0, 0));
        assert!(v100.tag.include_v);
        assert!(v100.component().is_none());

        let v110 = tags.iter().find(|t| t.name() == "v1.1.0").unwrap();
        assert_eq!(*v110.version(), Version::new(1, 1, 0));
    }

    #[test]
    fn test_find_tags_with_component() {
        let test_repo = TestRepo::new();
        test_repo.write_file("a.txt", "a");
        test_repo.add_and_commit("feat: initial");
        test_repo.create_tag("my-lib-v1.0.0");
        test_repo.create_tag("other-v2.0.0");

        let tags = find_tags(test_repo.repo()).unwrap();
        assert_eq!(tags.len(), 2);

        let mylib = tags.iter().find(|t| t.name() == "my-lib-v1.0.0").unwrap();
        assert_eq!(mylib.component(), Some("my-lib"));
        assert_eq!(mylib.tag.separator.as_str(), "-");
        assert_eq!(*mylib.version(), Version::new(1, 0, 0));
    }

    #[test]
    fn test_find_tags_without_v_prefix() {
        let test_repo = TestRepo::new();
        test_repo.write_file("a.txt", "a");
        test_repo.add_and_commit("feat: initial");
        test_repo.create_tag("1.0.0");

        let tags = find_tags(test_repo.repo()).unwrap();
        assert_eq!(tags.len(), 1);
        assert!(!tags[0].tag.include_v);
        assert_eq!(*tags[0].version(), Version::new(1, 0, 0));
    }

    #[test]
    fn test_find_tags_with_slash_separator() {
        let test_repo = TestRepo::new();
        test_repo.write_file("a.txt", "a");
        test_repo.add_and_commit("feat: initial");
        test_repo.create_tag("my-lib/v1.0.0");

        let tags = find_tags(test_repo.repo()).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].component(), Some("my-lib"));
        assert_eq!(tags[0].tag.separator.as_str(), "/");
    }

    #[test]
    fn test_find_tags_ignores_non_version_tags() {
        let test_repo = TestRepo::new();
        test_repo.write_file("a.txt", "a");
        test_repo.add_and_commit("feat: initial");
        test_repo.create_tag("v1.0.0");
        test_repo.create_tag("some-random-tag");
        test_repo.create_tag("release-candidate");

        let tags = find_tags(test_repo.repo()).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name(), "v1.0.0");
    }

    #[test]
    fn test_split_commits_by_path() {
        let commits = vec![
            GitCommit {
                sha: "1".into(),
                message: "feat: add foo".into(),
                files: vec!["packages/foo/src/lib.rs".into()],
            },
            GitCommit {
                sha: "2".into(),
                message: "fix: fix bar".into(),
                files: vec!["packages/bar/index.js".into()],
            },
            GitCommit {
                sha: "3".into(),
                message: "feat: shared".into(),
                files: vec![
                    "packages/foo/README.md".into(),
                    "packages/bar/README.md".into(),
                ],
            },
        ];

        let result = split_commits_by_path(&commits, &["packages/foo", "packages/bar"]);
        assert_eq!(result["packages/foo"].len(), 2); // commits 1 and 3
        assert_eq!(result["packages/bar"].len(), 2); // commits 2 and 3
    }

    #[test]
    fn test_split_commits_with_root_path() {
        let commits = vec![
            GitCommit {
                sha: "1".into(),
                message: "feat: root change".into(),
                files: vec!["README.md".into()],
            },
            GitCommit {
                sha: "2".into(),
                message: "feat: package change".into(),
                files: vec!["packages/foo/lib.rs".into()],
            },
        ];

        let result = split_commits_by_path(&commits, &[".", "packages/foo"]);
        assert_eq!(result["."].len(), 1); // only root-level file
        assert_eq!(result["packages/foo"].len(), 1); // only package file
    }

    #[test]
    fn test_find_latest_tag_for_component() {
        let tags = vec![
            ReleaseTag {
                tag: TagName::new(Version::new(1, 0, 0), Some("foo".into()), "-", true),
                sha: "a".into(),
            },
            ReleaseTag {
                tag: TagName::new(Version::new(1, 1, 0), Some("foo".into()), "-", true),
                sha: "b".into(),
            },
            ReleaseTag {
                tag: TagName::new(Version::new(2, 0, 0), Some("bar".into()), "-", true),
                sha: "c".into(),
            },
        ];

        let latest_foo = find_latest_tag_for_component(&tags, Some("foo"), true).unwrap();
        assert_eq!(*latest_foo.version(), Version::new(1, 1, 0));

        let latest_bar = find_latest_tag_for_component(&tags, Some("bar"), true).unwrap();
        assert_eq!(*latest_bar.version(), Version::new(2, 0, 0));

        let no_match = find_latest_tag_for_component(&tags, Some("baz"), true);
        assert!(no_match.is_none());
    }

    #[test]
    fn test_find_latest_tag_no_component() {
        let tags = vec![
            ReleaseTag {
                tag: TagName::new(Version::new(1, 0, 0), None, "-", true),
                sha: "a".into(),
            },
            ReleaseTag {
                tag: TagName::new(Version::new(2, 0, 0), None, "-", true),
                sha: "b".into(),
            },
        ];

        let latest = find_latest_tag_for_component(&tags, None, false).unwrap();
        assert_eq!(*latest.version(), Version::new(2, 0, 0));
    }

    #[test]
    fn test_tag_with_prerelease() {
        let test_repo = TestRepo::new();
        test_repo.write_file("a.txt", "a");
        test_repo.add_and_commit("feat: initial");
        test_repo.create_tag("v1.0.0-alpha.1");

        let tags = find_tags(test_repo.repo()).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(*tags[0].version(), Version::parse("1.0.0-alpha.1").unwrap());
    }

    #[test]
    fn test_walk_commits_with_subdirectory_changes() {
        let test_repo = TestRepo::new();

        test_repo.write_file("packages/foo/lib.rs", "// foo");
        test_repo.add_and_commit("feat: add foo");

        test_repo.write_file("packages/bar/lib.rs", "// bar");
        test_repo.add_and_commit("feat: add bar");

        let commits = walk_commits(test_repo.repo(), None).unwrap();
        assert_eq!(commits.len(), 2);

        // Newest commit (bar) should only have bar file
        assert_eq!(commits[0].files, vec!["packages/bar/lib.rs"]);
        // Older commit (foo) should only have foo file
        assert_eq!(commits[1].files, vec!["packages/foo/lib.rs"]);
    }
}
