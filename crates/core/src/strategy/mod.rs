//! Release strategies — each knows which files to update for its ecosystem.

mod bazel;
mod dart;
mod elixir;
mod go;
mod helm;
mod java;
mod node;
mod php;
mod python;
mod ruby;
mod rust;
mod simple;

use semver::Version;
use std::path::Path;

use crate::changelog;
use crate::config::{ExtraFile, ResolvedConfig};
use crate::error::Result;
use crate::updater;

/// A file update produced by a release strategy.
#[derive(Debug, Clone)]
pub struct FileUpdate {
    /// Path relative to the repository root.
    pub path: String,
    /// New file content.
    pub content: String,
    /// Whether to create the file if it doesn't exist.
    pub create_if_missing: bool,
}

/// Trait for ecosystem-specific release strategies.
pub trait ReleaseStrategy {
    /// Compute file updates for a new release.
    ///
    /// `repo_path` is the absolute path to the repository root.
    /// `pkg_path` is the relative path to the package (e.g., "." or "packages/foo").
    fn build_updates(
        &self,
        repo_path: &Path,
        pkg_path: &str,
        new_version: &Version,
        changelog_entry: &str,
        config: &ResolvedConfig,
    ) -> Result<Vec<FileUpdate>>;
}

/// Create a release strategy from its name.
pub fn create_strategy(release_type: &str) -> Box<dyn ReleaseStrategy> {
    match release_type {
        "simple" => Box::new(simple::SimpleStrategy),
        "rust" => Box::new(rust::RustStrategy),
        "node" => Box::new(node::NodeStrategy),
        "python" => Box::new(python::PythonStrategy),
        "go" => Box::new(go::GoStrategy),
        "helm" => Box::new(helm::HelmStrategy),
        "dart" => Box::new(dart::DartStrategy),
        "java" | "maven" => Box::new(java::JavaStrategy),
        "ruby" => Box::new(ruby::RubyStrategy),
        "php" => Box::new(php::PhpStrategy),
        "elixir" => Box::new(elixir::ElixirStrategy),
        "bazel" => Box::new(bazel::BazelStrategy),
        _ => Box::new(simple::SimpleStrategy),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Join a package path with a relative file path.
fn join_pkg_path(pkg_path: &str, file_path: &str) -> String {
    if pkg_path == "." {
        file_path.to_string()
    } else {
        format!("{}/{}", pkg_path, file_path)
    }
}

/// Build changelog update if not skipped.
fn build_changelog_update(
    repo_path: &Path,
    pkg_path: &str,
    changelog_entry: &str,
    config: &ResolvedConfig,
) -> Result<Option<FileUpdate>> {
    if config.skip_changelog {
        return Ok(None);
    }

    let changelog_path = join_pkg_path(pkg_path, &config.changelog_path);
    let full_path = repo_path.join(&changelog_path);

    let existing = if full_path.exists() {
        std::fs::read_to_string(&full_path)?
    } else {
        String::new()
    };

    let new_content = changelog::update_changelog(&existing, changelog_entry);

    Ok(Some(FileUpdate {
        path: changelog_path,
        content: new_content,
        create_if_missing: true,
    }))
}

/// Build updates for extra-files (generic annotation-based).
fn build_extra_file_updates(
    repo_path: &Path,
    pkg_path: &str,
    new_version: &Version,
    extra_files: &[ExtraFile],
) -> Result<Vec<FileUpdate>> {
    let mut updates = Vec::new();
    let version_str = new_version.to_string();

    for extra in extra_files {
        match extra {
            ExtraFile::Simple(path) => {
                let full_rel = join_pkg_path(pkg_path, path);
                let full_path = repo_path.join(&full_rel);
                if full_path.exists() {
                    let content = std::fs::read_to_string(&full_path)?;
                    let updated = updater::update_generic_version(&content, &version_str);
                    if updated != content {
                        updates.push(FileUpdate {
                            path: full_rel,
                            content: updated,
                            create_if_missing: false,
                        });
                    }
                }
            }
            ExtraFile::Typed(_typed) => {
                // JSON/YAML/TOML/XML typed updates are deferred to later phases
            }
        }
    }

    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_pkg_path_root() {
        assert_eq!(join_pkg_path(".", "CHANGELOG.md"), "CHANGELOG.md");
    }

    #[test]
    fn test_join_pkg_path_subdir() {
        assert_eq!(
            join_pkg_path("packages/foo", "CHANGELOG.md"),
            "packages/foo/CHANGELOG.md"
        );
    }

    #[test]
    fn test_create_strategy_known() {
        // Just verify the factory doesn't panic
        let _ = create_strategy("simple");
        let _ = create_strategy("rust");
        let _ = create_strategy("node");
    }

    #[test]
    fn test_create_strategy_unknown_fallback() {
        let _ = create_strategy("unknown-type");
    }
}
