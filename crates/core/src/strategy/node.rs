use semver::Version;
use std::path::Path;

use crate::config::ResolvedConfig;
use crate::error::Result;
use crate::updater;

use super::{build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy};

/// Node/npm strategy: updates package.json, package-lock.json, and CHANGELOG.md.
pub struct NodeStrategy;

impl ReleaseStrategy for NodeStrategy {
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

        // package.json
        let pkg_json_path = join_pkg_path(pkg_path, "package.json");
        let pkg_json_full = repo_path.join(&pkg_json_path);
        if pkg_json_full.exists() {
            let content = std::fs::read_to_string(&pkg_json_full)?;
            let updated = updater::update_package_json_version(&content, &version_str);
            updates.push(FileUpdate {
                path: pkg_json_path,
                content: updated,
                create_if_missing: false,
            });
        }

        // package-lock.json (always at repo root or pkg root)
        let lock_path = join_pkg_path(pkg_path, "package-lock.json");
        let lock_full = repo_path.join(&lock_path);
        if lock_full.exists() {
            let content = std::fs::read_to_string(&lock_full)?;
            let updated =
                updater::update_package_lock_json_version(&content, &version_str);
            updates.push(FileUpdate {
                path: lock_path,
                content: updated,
                create_if_missing: false,
            });
        }

        // npm-shrinkwrap.json
        let shrinkwrap_path = join_pkg_path(pkg_path, "npm-shrinkwrap.json");
        let shrinkwrap_full = repo_path.join(&shrinkwrap_path);
        if shrinkwrap_full.exists() {
            let content = std::fs::read_to_string(&shrinkwrap_full)?;
            let updated =
                updater::update_package_lock_json_version(&content, &version_str);
            updates.push(FileUpdate {
                path: shrinkwrap_path,
                content: updated,
                create_if_missing: false,
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestRepo;

    fn node_config() -> ResolvedConfig {
        let mut defaults = crate::config::ReleaserConfig::default();
        defaults.release_type = Some("node".to_string());
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_node_updates_package_json() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "package.json",
            "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\"\n}\n",
        );

        let version = Version::new(1, 0, 1);
        let config = node_config();
        let strategy = NodeStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.0.1\n", &config)
            .unwrap();

        let pkg_update = updates.iter().find(|u| u.path == "package.json").unwrap();
        assert!(pkg_update.content.contains("\"version\": \"1.0.1\""));
        assert!(pkg_update.content.contains("\"name\": \"my-pkg\""));
    }

    #[test]
    fn test_node_updates_package_lock() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "package.json",
            "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\"\n}\n",
        );
        test_repo.write_file(
            "package-lock.json",
            "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\",\n  \"lockfileVersion\": 3,\n  \"packages\": {\n    \"\": {\n      \"version\": \"1.0.0\"\n    }\n  }\n}\n",
        );

        let version = Version::new(1, 1, 0);
        let config = node_config();
        let strategy = NodeStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.1.0\n", &config)
            .unwrap();

        let lock_update = updates
            .iter()
            .find(|u| u.path == "package-lock.json")
            .unwrap();
        assert!(lock_update.content.contains("\"version\": \"1.1.0\""));
    }

    #[test]
    fn test_node_includes_changelog() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "package.json",
            "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\"\n}\n",
        );

        let version = Version::new(1, 1, 0);
        let config = node_config();
        let strategy = NodeStrategy;

        let updates = strategy
            .build_updates(
                test_repo.path(),
                ".",
                &version,
                "## 1.1.0\n\n### Features\n\n* stuff\n",
                &config,
            )
            .unwrap();

        assert!(updates.iter().any(|u| u.path == "CHANGELOG.md"));
    }

    #[test]
    fn test_node_no_lock_file() {
        let test_repo = TestRepo::new();
        test_repo.write_file(
            "package.json",
            "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\"\n}\n",
        );

        let version = Version::new(1, 1, 0);
        let config = node_config();
        let strategy = NodeStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.1.0\n", &config)
            .unwrap();

        // No package-lock.json update since file doesn't exist
        assert!(!updates.iter().any(|u| u.path == "package-lock.json"));
    }
}
