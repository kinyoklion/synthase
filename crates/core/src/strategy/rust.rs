use semver::Version;
use std::path::Path;

use crate::config::ResolvedConfig;
use crate::error::Result;
use crate::updater;

use super::{
    build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy,
};

/// Rust/Cargo strategy: updates Cargo.toml, Cargo.lock, and CHANGELOG.md.
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

        // Cargo.toml
        let cargo_toml_path = join_pkg_path(pkg_path, "Cargo.toml");
        let cargo_toml_full = repo_path.join(&cargo_toml_path);
        if cargo_toml_full.exists() {
            let content = std::fs::read_to_string(&cargo_toml_full)?;
            let updated = updater::update_cargo_toml_version(&content, &version_str);
            updates.push(FileUpdate {
                path: cargo_toml_path,
                content: updated,
                create_if_missing: false,
            });
        }

        // Cargo.lock (always at repo root)
        let cargo_lock_full = repo_path.join("Cargo.lock");
        if cargo_lock_full.exists() {
            let content = std::fs::read_to_string(&cargo_lock_full)?;
            // Determine the package name from Cargo.toml or config
            let pkg_name = config
                .package_name
                .as_deref()
                .or(config.component.as_deref());

            if let Some(name) = pkg_name {
                let updated = updater::update_cargo_lock_version(&content, name, &version_str);
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
