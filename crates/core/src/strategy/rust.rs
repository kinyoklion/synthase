use semver::Version;
use std::path::Path;

use crate::config::ResolvedConfig;
use crate::error::Result;
use crate::updater;

use super::{
    build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy,
};

/// Rust/Cargo strategy: updates Cargo.toml, Cargo.lock, and CHANGELOG.md.
///
/// Handles both single-crate projects (root Cargo.toml has `[package]`) and
/// workspace projects (root Cargo.toml has `[workspace]` with members).
pub struct RustStrategy;

impl ReleaseStrategy for RustStrategy {
    fn build_updates(
        &self,
        repo_path: &Path,
        pkg_path: &str,
        new_version: &Version,
        changelog_entry: &str,
        config: &ResolvedConfig,
    ) -> Result<Vec<FileUpdate>> {
        let mut updates = Vec::new();
        let version_str = new_version.to_string();

        // Changelog
        if let Some(cl) = build_changelog_update(repo_path, pkg_path, changelog_entry, config)? {
            updates.push(cl);
        }

        // Cargo.toml — either single crate or workspace
        let cargo_toml_path = join_pkg_path(pkg_path, "Cargo.toml");
        let cargo_toml_full = repo_path.join(&cargo_toml_path);
        if cargo_toml_full.exists() {
            let content = std::fs::read_to_string(&cargo_toml_full)?;

            if content.contains("[package]") {
                // Single crate: update the [package] version directly
                let updated = updater::update_cargo_toml_version(&content, &version_str);
                updates.push(FileUpdate {
                    path: cargo_toml_path,
                    content: updated,
                    create_if_missing: false,
                });
            } else if content.contains("[workspace]") {
                // Workspace: find and update all member Cargo.toml files that
                // have [package] sections
                let member_updates =
                    update_workspace_member_cargo_tomls(repo_path, &content, &version_str)?;
                updates.extend(member_updates);
            }
        }

        // Cargo.lock (always at repo root)
        let cargo_lock_full = repo_path.join("Cargo.lock");
        if cargo_lock_full.exists() {
            let content = std::fs::read_to_string(&cargo_lock_full)?;
            let pkg_name = config
                .package_name
                .as_deref()
                .or(config.component.as_deref());

            if let Some(name) = pkg_name {
                let updated = updater::update_cargo_lock_version(&content, name, &version_str);
                // Also update the CLI crate in Cargo.lock if it exists
                let updated =
                    update_cargo_lock_all_workspace_crates(repo_path, &updated, &version_str);
                updates.push(FileUpdate {
                    path: "Cargo.lock".to_string(),
                    content: updated,
                    create_if_missing: false,
                });
            }
        }

        // Extra files
        updates.extend(build_extra_file_updates(
            repo_path,
            pkg_path,
            new_version,
            &config.extra_files,
        )?);

        Ok(updates)
    }
}

/// Find workspace members from a root Cargo.toml and update their versions.
fn update_workspace_member_cargo_tomls(
    repo_path: &Path,
    root_content: &str,
    new_version: &str,
) -> Result<Vec<FileUpdate>> {
    let mut updates = Vec::new();

    // Parse workspace members from the root Cargo.toml
    let parsed: toml::Value = match toml::from_str(root_content) {
        Ok(v) => v,
        Err(_) => return Ok(updates),
    };

    let members = match parsed
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        Some(m) => m,
        None => return Ok(updates),
    };

    for member_val in members {
        let pattern = match member_val.as_str() {
            Some(s) => s,
            None => continue,
        };

        // Resolve glob patterns
        let member_paths = resolve_members(repo_path, pattern);
        for member_path in member_paths {
            let cargo_path = repo_path.join(&member_path).join("Cargo.toml");
            if !cargo_path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&cargo_path)?;
            if !content.contains("[package]") {
                continue;
            }
            let updated = updater::update_cargo_toml_version(&content, new_version);
            if updated != content {
                let rel_path = format!("{member_path}/Cargo.toml");
                updates.push(FileUpdate {
                    path: rel_path,
                    content: updated,
                    create_if_missing: false,
                });
            }
        }
    }

    Ok(updates)
}

/// Resolve a workspace member glob pattern to actual directories.
fn resolve_members(repo_path: &Path, pattern: &str) -> Vec<String> {
    if pattern.contains('*') {
        let full_pattern = repo_path.join(pattern).join("Cargo.toml");
        let pattern_str = full_pattern.to_string_lossy().to_string();
        glob::glob(&pattern_str)
            .ok()
            .map(|paths| {
                paths
                    .filter_map(|p| p.ok())
                    .filter_map(|p| {
                        p.parent().and_then(|dir| {
                            dir.strip_prefix(repo_path)
                                .ok()
                                .map(|rel| rel.to_string_lossy().to_string())
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![pattern.to_string()]
    }
}

/// Update Cargo.lock for all workspace member crate names.
fn update_cargo_lock_all_workspace_crates(
    repo_path: &Path,
    lock_content: &str,
    new_version: &str,
) -> String {
    let root_toml_path = repo_path.join("Cargo.toml");
    let root_content = match std::fs::read_to_string(&root_toml_path) {
        Ok(c) => c,
        Err(_) => return lock_content.to_string(),
    };
    let parsed: toml::Value = match toml::from_str(&root_content) {
        Ok(v) => v,
        Err(_) => return lock_content.to_string(),
    };

    let members = match parsed
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        Some(m) => m,
        None => return lock_content.to_string(),
    };

    let mut result = lock_content.to_string();
    for member_val in members {
        let pattern = match member_val.as_str() {
            Some(s) => s,
            None => continue,
        };
        for member_path in resolve_members(repo_path, pattern) {
            let cargo_path = repo_path.join(&member_path).join("Cargo.toml");
            if let Ok(content) = std::fs::read_to_string(&cargo_path) {
                if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                    if let Some(name) = parsed
                        .get("package")
                        .and_then(|p| p.get("name"))
                        .and_then(|n| n.as_str())
                    {
                        result = updater::update_cargo_lock_version(&result, name, new_version);
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResolvedConfig;
    use crate::testutil::TestRepo;

    fn rust_config() -> ResolvedConfig {
        let defaults = crate::config::ReleaserConfig {
            release_type: Some("rust".to_string()),
            ..Default::default()
        };
        let pkg = crate::config::ReleaserConfig {
            component: Some("my-crate".to_string()),
            package_name: Some("my-crate".to_string()),
            ..Default::default()
        };
        crate::config::resolve_config(&defaults, &pkg)
    }

    #[test]
    fn test_rust_updates_cargo_toml() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "Cargo.toml",
            "[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
        );

        let version = Version::new(1, 1, 0);
        let config = rust_config();
        let strategy = RustStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.1.0\n", &config)
            .unwrap();

        let cargo_update = updates.iter().find(|u| u.path == "Cargo.toml").unwrap();
        assert!(cargo_update.content.contains("version = \"1.1.0\""));
        assert!(cargo_update.content.contains("name = \"my-crate\""));
    }

    #[test]
    fn test_rust_workspace_updates_member_cargo_tomls() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/core\", \"crates/cli\"]\nresolver = \"2\"\n",
        );
        test_repo.write_file(
            "crates/core/Cargo.toml",
            "[package]\nname = \"my-lib\"\nversion = \"1.0.0\"\n",
        );
        test_repo.write_file(
            "crates/cli/Cargo.toml",
            "[package]\nname = \"my-cli\"\nversion = \"1.0.0\"\n",
        );

        let version = Version::new(1, 1, 0);
        let config = rust_config();
        let strategy = RustStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.1.0\n", &config)
            .unwrap();

        // Both member Cargo.tomls should be updated
        let core_update = updates
            .iter()
            .find(|u| u.path == "crates/core/Cargo.toml")
            .expect("core Cargo.toml should be updated");
        assert!(core_update.content.contains("version = \"1.1.0\""));

        let cli_update = updates
            .iter()
            .find(|u| u.path == "crates/cli/Cargo.toml")
            .expect("cli Cargo.toml should be updated");
        assert!(cli_update.content.contains("version = \"1.1.0\""));

        // Root Cargo.toml should NOT be in updates (no [package] section)
        assert!(
            !updates.iter().any(|u| u.path == "Cargo.toml"),
            "workspace root Cargo.toml should not be updated"
        );
    }

    #[test]
    fn test_rust_workspace_with_glob_members() {
        let test_repo = TestRepo::new();
        test_repo.write_file("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        test_repo.write_file(
            "crates/foo/Cargo.toml",
            "[package]\nname = \"foo\"\nversion = \"1.0.0\"\n",
        );
        test_repo.write_file(
            "crates/bar/Cargo.toml",
            "[package]\nname = \"bar\"\nversion = \"1.0.0\"\n",
        );

        let version = Version::new(2, 0, 0);
        let config = rust_config();
        let strategy = RustStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 2.0.0\n", &config)
            .unwrap();

        assert!(updates.iter().any(|u| u.path == "crates/foo/Cargo.toml"));
        assert!(updates.iter().any(|u| u.path == "crates/bar/Cargo.toml"));
    }

    #[test]
    fn test_rust_updates_cargo_lock() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "Cargo.toml",
            "[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\n",
        );
        test_repo.write_file(
            "Cargo.lock",
            "[[package]]\nname = \"my-crate\"\nversion = \"1.0.0\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.100\"\n",
        );

        let version = Version::new(1, 1, 0);
        let config = rust_config();
        let strategy = RustStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.1.0\n", &config)
            .unwrap();

        let lock_update = updates.iter().find(|u| u.path == "Cargo.lock").unwrap();
        assert!(lock_update
            .content
            .contains("name = \"my-crate\"\nversion = \"1.1.0\""));
        assert!(lock_update
            .content
            .contains("name = \"serde\"\nversion = \"1.0.100\""));
    }

    #[test]
    fn test_rust_includes_changelog() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "Cargo.toml",
            "[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\n",
        );

        let version = Version::new(1, 1, 0);
        let config = rust_config();
        let strategy = RustStrategy;

        let updates = strategy
            .build_updates(
                test_repo.path(),
                ".",
                &version,
                "## 1.1.0\n\n### Features\n\n* stuff\n",
                &config,
            )
            .unwrap();

        let cl = updates.iter().find(|u| u.path == "CHANGELOG.md").unwrap();
        assert!(cl.content.contains("## 1.1.0"));
    }

    #[test]
    fn test_rust_subdirectory_package() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "crates/foo/Cargo.toml",
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n",
        );

        let version = Version::new(0, 2, 0);
        let mut config = rust_config();
        config.package_name = Some("foo".to_string());
        let strategy = RustStrategy;

        let updates = strategy
            .build_updates(
                test_repo.path(),
                "crates/foo",
                &version,
                "## 0.2.0\n",
                &config,
            )
            .unwrap();

        let cargo_update = updates
            .iter()
            .find(|u| u.path == "crates/foo/Cargo.toml")
            .unwrap();
        assert!(cargo_update.content.contains("version = \"0.2.0\""));
    }
}
