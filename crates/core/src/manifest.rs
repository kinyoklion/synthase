//! Manifest orchestrator — the top-level entry point that ties all modules together.
//!
//! Processes a repository (single or multi-package) and produces a complete
//! set of release outputs: version bumps, changelog entries, file updates,
//! and PR metadata.

use chrono::Utc;
use git2::Repository;
use semver::Version;
use std::collections::HashMap;
use std::path::Path;

use crate::changelog::{self, ChangelogOptions};
use crate::commit::{self, ConventionalCommit};
use crate::config::{self, ManifestConfig, ResolvedConfig};
use crate::error::{Error, Result};
use crate::git::{self, GitCommit};
use crate::strategy::{self, FileUpdate};
use crate::tag::TagName;
use crate::versioning;

/// A single component's release information.
#[derive(Debug, Clone)]
pub struct ComponentRelease {
    /// Component name (for tagging/PR titles).
    pub component: Option<String>,
    /// Package path relative to repo root (e.g., "." or "packages/foo").
    pub package_path: String,
    /// Previous version (None for first release).
    pub current_version: Option<Version>,
    /// New version to release.
    pub new_version: Version,
    /// The tag string for this release.
    pub tag: String,
    /// Generated changelog entry markdown.
    pub changelog_entry: String,
    /// File updates to apply.
    pub file_updates: Vec<FileUpdate>,
    /// Resolved config for this package.
    pub config: ResolvedConfig,
}

/// Complete output from processing a repository.
#[derive(Debug)]
pub struct ReleaseOutput {
    /// Per-component release information.
    pub releases: Vec<ComponentRelease>,
    /// Updated .release-please-manifest.json content.
    pub manifest_update: Option<FileUpdate>,
}

/// Process a repository and compute releases for all configured packages.
pub fn process_repo(repo_path: &Path) -> Result<ReleaseOutput> {
    let config_path = repo_path.join("release-please-config.json");
    let manifest_path = repo_path.join(".release-please-manifest.json");

    let manifest_config = config::load_config(&config_path)?;
    let manifest_versions = if manifest_path.exists() {
        config::load_manifest(&manifest_path)?
    } else {
        HashMap::new()
    };

    process_repo_with_config(repo_path, &manifest_config, &manifest_versions)
}

/// Process a repository with pre-loaded configuration.
///
/// This is the main orchestration function, separated from `process_repo`
/// for testability.
pub fn process_repo_with_config(
    repo_path: &Path,
    manifest_config: &ManifestConfig,
    manifest_versions: &HashMap<String, String>,
) -> Result<ReleaseOutput> {
    let repo = Repository::open(repo_path).map_err(|e| {
        Error::Config(format!("failed to open repository at {}: {}", repo_path.display(), e))
    })?;

    let tags = git::find_tags(&repo)?;

    // Determine the earliest stop SHA across all packages
    let stop_sha = manifest_config
        .last_release_sha
        .as_deref()
        .or(manifest_config.bootstrap_sha.as_deref());

    let all_commits = git::walk_commits(&repo, stop_sha)?;

    // Split commits by package paths
    let pkg_paths: Vec<&str> = manifest_config
        .packages
        .keys()
        .map(|s| s.as_str())
        .collect();

    let commits_by_path = git::split_commits_by_path(&all_commits, &pkg_paths);

    let today = Utc::now().format("%Y-%m-%d").to_string();
    let mut releases = Vec::new();

    for (pkg_path, pkg_config) in &manifest_config.packages {
        let resolved = config::resolve_config(&manifest_config.defaults, pkg_config);

        // Determine component name
        let component = resolved
            .component
            .clone()
            .or_else(|| {
                if pkg_path == "." {
                    resolved.package_name.clone()
                } else {
                    // Use last path segment as component
                    pkg_path.rsplit('/').next().map(|s| s.to_string())
                }
            });

        // Find latest release for this component
        let latest_tag = git::find_latest_tag_for_component(
            &tags,
            component.as_deref(),
            resolved.include_component_in_tag,
        );

        let current_version = latest_tag
            .map(|t| t.version().clone())
            .or_else(|| {
                manifest_versions
                    .get(pkg_path)
                    .and_then(|v| Version::parse(v).ok())
            });

        // Get commits for this path, filtered to those after the last release
        let pkg_commits = commits_by_path
            .get(pkg_path)
            .cloned()
            .unwrap_or_default();

        let filtered_commits: Vec<&GitCommit> = if let Some(ref tag) = latest_tag {
            filter_commits_after_sha(&pkg_commits, &tag.sha)
        } else {
            pkg_commits.to_vec()
        };

        // Parse conventional commits
        let conventional: Vec<ConventionalCommit> = filtered_commits
            .iter()
            .filter_map(|c| commit::parse_conventional_commit(&c.sha, &c.message))
            .collect();

        if conventional.is_empty() {
            continue;
        }

        // Determine version bump
        let strategy = versioning::create_versioning_strategy(
            &resolved.versioning,
            resolved.bump_minor_pre_major,
            resolved.bump_patch_for_minor_pre_major,
            resolved.prerelease_type.as_deref(),
        );

        let base_version = current_version
            .clone()
            .or_else(|| {
                resolved
                    .initial_version
                    .as_ref()
                    .and_then(|v| Version::parse(v).ok())
            })
            .unwrap_or_else(|| Version::new(0, 0, 0));

        let new_version = match strategy.bump(&base_version, &conventional) {
            Some(v) => v,
            None => continue, // no releasable commits
        };

        // For initial release, use the computed version directly
        // (initial_version is used as the base, bump produces the actual release)
        let new_version = if current_version.is_none() {
            if let Some(ref iv) = resolved.initial_version {
                Version::parse(iv).unwrap_or(new_version)
            } else {
                new_version
            }
        } else {
            new_version
        };

        // Build tag
        let tag_name = TagName::from_config(
            new_version.clone(),
            component.clone(),
            resolved.include_component_in_tag,
            &resolved.tag_separator,
            resolved.include_v_in_tag,
        );
        let tag_str = tag_name.to_string();

        // Generate changelog entry
        let previous_tag = latest_tag.map(|t| t.name());
        let changelog_options = ChangelogOptions {
            version: new_version.to_string(),
            previous_tag,
            current_tag: tag_str.clone(),
            date: today.clone(),
            host: resolved
                .changelog_host
                .clone()
                .unwrap_or_else(|| "https://github.com".to_string()),
            owner: String::new(), // filled by caller or action layer
            repository: String::new(),
            changelog_sections: resolved.changelog_sections.clone(),
        };

        let changelog_entry =
            changelog::generate_changelog_entry(&conventional, &changelog_options);

        // Build file updates
        let release_strategy = strategy::create_strategy(&resolved.release_type);
        let file_updates = release_strategy.build_updates(
            repo_path,
            pkg_path,
            &new_version,
            &changelog_entry,
            &resolved,
        )?;

        releases.push(ComponentRelease {
            component,
            package_path: pkg_path.clone(),
            current_version,
            new_version,
            tag: tag_str,
            changelog_entry,
            file_updates,
            config: resolved,
        });
    }

    // Build manifest update
    let manifest_update = build_manifest_update(manifest_versions, &releases)?;

    Ok(ReleaseOutput {
        releases,
        manifest_update,
    })
}

/// Filter commits to only those after a given SHA.
fn filter_commits_after_sha<'a>(
    commits: &[&'a GitCommit],
    sha: &str,
) -> Vec<&'a GitCommit> {
    let mut result = Vec::new();
    for commit in commits {
        if commit.sha == sha {
            break;
        }
        result.push(*commit);
    }
    result
}

/// Build the updated .release-please-manifest.json content.
fn build_manifest_update(
    existing: &HashMap<String, String>,
    releases: &[ComponentRelease],
) -> Result<Option<FileUpdate>> {
    if releases.is_empty() {
        return Ok(None);
    }

    let mut manifest = existing.clone();
    for release in releases {
        manifest.insert(
            release.package_path.clone(),
            release.new_version.to_string(),
        );
    }

    let content = serde_json::to_string_pretty(&manifest)? + "\n";

    Ok(Some(FileUpdate {
        path: ".release-please-manifest.json".to_string(),
        content,
        create_if_missing: true,
    }))
}

// ---------------------------------------------------------------------------
// PR formatting
// ---------------------------------------------------------------------------

/// Format a PR title for the release.
pub fn format_pr_title(
    releases: &[ComponentRelease],
    config: &ManifestConfig,
    branch: &str,
) -> String {
    if releases.len() == 1 {
        let r = &releases[0];
        let pattern = r
            .config
            .pull_request_title_pattern
            .as_deref()
            .unwrap_or("chore(${branch}): release${component} ${version}");

        let component_str = r
            .component
            .as_ref()
            .map(|c| format!(" {}", c))
            .unwrap_or_default();

        pattern
            .replace("${branch}", branch)
            .replace("${component}", &component_str)
            .replace("${version}", &r.new_version.to_string())
    } else {
        let pattern = config
            .group_pull_request_title_pattern
            .as_deref()
            .unwrap_or("chore: release ${branch}");

        pattern.replace("${branch}", branch)
    }
}

/// Format a PR body for the release.
pub fn format_pr_body(
    releases: &[ComponentRelease],
    config: &ManifestConfig,
) -> String {
    let mut body = String::new();

    // Header
    let header = releases
        .first()
        .and_then(|r| r.config.pull_request_header.as_deref())
        .or(config.defaults.pull_request_header.as_deref())
        .unwrap_or(":robot: I have created a release *beep* *boop*");
    body.push_str(header);
    body.push_str("\n---\n");

    if releases.len() == 1 {
        body.push('\n');
        body.push_str(&releases[0].changelog_entry);
    } else {
        for release in releases {
            let label = release
                .component
                .as_deref()
                .unwrap_or(&release.package_path);
            body.push_str(&format!(
                "\n<details><summary>{}: {}</summary>\n\n{}</details>\n",
                label, release.new_version, release.changelog_entry,
            ));
        }
    }

    // Footer
    body.push_str("\n---\n");
    let footer = releases
        .first()
        .and_then(|r| r.config.pull_request_footer.as_deref())
        .or(config.defaults.pull_request_footer.as_deref())
        .unwrap_or("This PR was generated with [Rustlease Please](https://github.com/rustlease-please/rustlease-please).");
    body.push_str(footer);
    body.push('\n');

    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestRepo;

    /// Helper: set up a single-package Rust repo with config + manifest + initial tag.
    fn setup_rust_repo() -> TestRepo {
        let repo = TestRepo::new();
        repo.write_config(&serde_json::json!({
            "release-type": "rust",
            "packages": {
                ".": {
                    "component": "my-crate",
                    "package-name": "my-crate"
                }
            }
        }));
        repo.write_manifest(&serde_json::json!({
            ".": "1.0.0"
        }));
        repo.write_file(
            "Cargo.toml",
            "[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\n",
        );
        repo.add_and_commit("chore: initial setup");
        repo.create_tag("v1.0.0");
        repo
    }

    #[test]
    fn test_single_package_feat_bump() {
        let repo = setup_rust_repo();

        repo.write_file("src/lib.rs", "// new feature");
        repo.add_and_commit("feat: add new feature");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest = config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        assert_eq!(output.releases.len(), 1);
        let release = &output.releases[0];
        assert_eq!(release.new_version, Version::new(1, 1, 0));
        assert_eq!(release.current_version, Some(Version::new(1, 0, 0)));
        assert!(release.changelog_entry.contains("### Features"));
        assert!(release.file_updates.iter().any(|u| u.path == "Cargo.toml"));
    }

    #[test]
    fn test_single_package_fix_bump() {
        let repo = setup_rust_repo();

        repo.write_file("src/lib.rs", "// fix");
        repo.add_and_commit("fix: resolve bug");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest = config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        assert_eq!(output.releases.len(), 1);
        assert_eq!(output.releases[0].new_version, Version::new(1, 0, 1));
    }

    #[test]
    fn test_no_releasable_commits() {
        let repo = setup_rust_repo();

        repo.write_file("README.md", "updated");
        repo.add_and_commit("chore: update readme");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest = config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        assert!(output.releases.is_empty());
        assert!(output.manifest_update.is_none());
    }

    #[test]
    fn test_manifest_gets_updated() {
        let repo = setup_rust_repo();

        repo.write_file("src/lib.rs", "// feature");
        repo.add_and_commit("feat: add feature");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest = config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        let mu = output.manifest_update.unwrap();
        assert!(mu.content.contains("\"1.1.0\""));
    }

    #[test]
    fn test_monorepo_independent_releases() {
        let repo = TestRepo::new();
        repo.write_config(&serde_json::json!({
            "packages": {
                "packages/a": {
                    "release-type": "simple",
                    "component": "a"
                },
                "packages/b": {
                    "release-type": "simple",
                    "component": "b"
                }
            }
        }));
        repo.write_manifest(&serde_json::json!({
            "packages/a": "1.0.0",
            "packages/b": "2.0.0"
        }));
        repo.add_and_commit("chore: initial");
        repo.create_tag("a-v1.0.0");
        repo.create_tag("b-v2.0.0");

        // Change only package a
        repo.write_file("packages/a/lib.rs", "// new");
        repo.add_and_commit("feat: a feature");

        // Change only package b
        repo.write_file("packages/b/lib.rs", "// fix");
        repo.add_and_commit("fix: b fix");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest = config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        assert_eq!(output.releases.len(), 2);

        let a_release = output.releases.iter().find(|r| r.component.as_deref() == Some("a")).unwrap();
        assert_eq!(a_release.new_version, Version::new(1, 1, 0)); // feat → minor

        let b_release = output.releases.iter().find(|r| r.component.as_deref() == Some("b")).unwrap();
        assert_eq!(b_release.new_version, Version::new(2, 0, 1)); // fix → patch
    }

    #[test]
    fn test_monorepo_only_one_package_changed() {
        let repo = TestRepo::new();
        repo.write_config(&serde_json::json!({
            "packages": {
                "packages/a": {
                    "release-type": "simple",
                    "component": "a"
                },
                "packages/b": {
                    "release-type": "simple",
                    "component": "b"
                }
            }
        }));
        repo.write_manifest(&serde_json::json!({
            "packages/a": "1.0.0",
            "packages/b": "2.0.0"
        }));
        repo.add_and_commit("chore: initial");
        repo.create_tag("a-v1.0.0");
        repo.create_tag("b-v2.0.0");

        // Only change package a
        repo.write_file("packages/a/lib.rs", "// feat");
        repo.add_and_commit("feat: a feature");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest = config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        // Only a should have a release
        assert_eq!(output.releases.len(), 1);
        assert_eq!(output.releases[0].component.as_deref(), Some("a"));
    }

    #[test]
    fn test_tag_generation() {
        let repo = setup_rust_repo();

        repo.write_file("src/lib.rs", "// feat");
        repo.add_and_commit("feat: feature");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest = config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        assert_eq!(output.releases[0].tag, "my-crate-v1.1.0");
    }

    // === PR formatting ===

    #[test]
    fn test_pr_title_single_component() {
        let releases = vec![ComponentRelease {
            component: Some("my-lib".to_string()),
            package_path: ".".to_string(),
            current_version: Some(Version::new(1, 0, 0)),
            new_version: Version::new(1, 1, 0),
            tag: "my-lib-v1.1.0".to_string(),
            changelog_entry: String::new(),
            file_updates: vec![],
            config: default_resolved_config(),
        }];
        let config = minimal_manifest_config();
        let title = format_pr_title(&releases, &config, "main");
        assert_eq!(title, "chore(main): release my-lib 1.1.0");
    }

    #[test]
    fn test_pr_title_no_component() {
        let releases = vec![ComponentRelease {
            component: None,
            package_path: ".".to_string(),
            current_version: Some(Version::new(1, 0, 0)),
            new_version: Version::new(2, 0, 0),
            tag: "v2.0.0".to_string(),
            changelog_entry: String::new(),
            file_updates: vec![],
            config: default_resolved_config(),
        }];
        let config = minimal_manifest_config();
        let title = format_pr_title(&releases, &config, "main");
        assert_eq!(title, "chore(main): release 2.0.0");
    }

    #[test]
    fn test_pr_title_multi_component() {
        let releases = vec![
            ComponentRelease {
                component: Some("a".to_string()),
                package_path: "packages/a".to_string(),
                current_version: None,
                new_version: Version::new(1, 0, 0),
                tag: "a-v1.0.0".to_string(),
                changelog_entry: String::new(),
                file_updates: vec![],
                config: default_resolved_config(),
            },
            ComponentRelease {
                component: Some("b".to_string()),
                package_path: "packages/b".to_string(),
                current_version: None,
                new_version: Version::new(2, 0, 0),
                tag: "b-v2.0.0".to_string(),
                changelog_entry: String::new(),
                file_updates: vec![],
                config: default_resolved_config(),
            },
        ];
        let config = minimal_manifest_config();
        let title = format_pr_title(&releases, &config, "main");
        assert_eq!(title, "chore: release main");
    }

    #[test]
    fn test_pr_body_single() {
        let releases = vec![ComponentRelease {
            component: Some("my-lib".to_string()),
            package_path: ".".to_string(),
            current_version: None,
            new_version: Version::new(1, 0, 0),
            tag: "v1.0.0".to_string(),
            changelog_entry: "### Features\n\n* init\n".to_string(),
            file_updates: vec![],
            config: default_resolved_config(),
        }];
        let config = minimal_manifest_config();
        let body = format_pr_body(&releases, &config);
        assert!(body.contains(":robot:"));
        assert!(body.contains("### Features"));
        assert!(!body.contains("<details>"));
    }

    #[test]
    fn test_pr_body_multi_collapsible() {
        let releases = vec![
            ComponentRelease {
                component: Some("a".to_string()),
                package_path: "packages/a".to_string(),
                current_version: None,
                new_version: Version::new(1, 0, 0),
                tag: "a-v1.0.0".to_string(),
                changelog_entry: "### Features\n\n* a stuff\n".to_string(),
                file_updates: vec![],
                config: default_resolved_config(),
            },
            ComponentRelease {
                component: Some("b".to_string()),
                package_path: "packages/b".to_string(),
                current_version: None,
                new_version: Version::new(2, 0, 0),
                tag: "b-v2.0.0".to_string(),
                changelog_entry: "### Bug Fixes\n\n* b stuff\n".to_string(),
                file_updates: vec![],
                config: default_resolved_config(),
            },
        ];
        let config = minimal_manifest_config();
        let body = format_pr_body(&releases, &config);
        assert!(body.contains("<details><summary>a: 1.0.0</summary>"));
        assert!(body.contains("<details><summary>b: 2.0.0</summary>"));
        assert!(body.contains("a stuff"));
        assert!(body.contains("b stuff"));
    }

    // === Test helpers ===

    fn default_resolved_config() -> ResolvedConfig {
        config::resolve_config(
            &config::ReleaserConfig::default(),
            &config::ReleaserConfig::default(),
        )
    }

    fn minimal_manifest_config() -> ManifestConfig {
        serde_json::from_str(r#"{"packages": {".": {}}}"#).unwrap()
    }
}
