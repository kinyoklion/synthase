use semver::Version;
use std::path::Path;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{
    build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy,
};

/// Simple strategy: updates CHANGELOG.md and an optional version.txt file.
pub struct SimpleStrategy;

impl ReleaseStrategy for SimpleStrategy {
    fn build_updates(
        &self,
        repo_path: &Path,
        pkg_path: &str,
        new_version: &Version,
        changelog_entry: &str,
        config: &ResolvedConfig,
    ) -> Result<Vec<FileUpdate>> {
        let mut updates = Vec::new();

        // Changelog
        if let Some(cl) = build_changelog_update(repo_path, pkg_path, changelog_entry, config)? {
            updates.push(cl);
        }

        // version.txt (or custom version_file)
        let version_file = config.version_file.as_deref().unwrap_or("version.txt");
        let version_path = join_pkg_path(pkg_path, version_file);
        let full_path = repo_path.join(&version_path);

        // Only update if file exists (don't create by default for simple strategy)
        if full_path.exists() {
            updates.push(FileUpdate {
                path: version_path,
                content: format!("{new_version}\n"),
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
    use crate::config::ResolvedConfig;
    use crate::testutil::TestRepo;

    fn default_config() -> ResolvedConfig {
        crate::config::resolve_config(
            &crate::config::ReleaserConfig::default(),
            &crate::config::ReleaserConfig::default(),
        )
    }

    #[test]
    fn test_simple_changelog_only() {
        let test_repo = TestRepo::new();
        let version = Version::new(1, 0, 0);
        let config = default_config();
        let strategy = SimpleStrategy;

        let updates = strategy
            .build_updates(
                test_repo.path(),
                ".",
                &version,
                "## 1.0.0\n\n### Features\n\n* init\n",
                &config,
            )
            .unwrap();

        // Only changelog (version.txt doesn't exist yet)
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].path, "CHANGELOG.md");
        assert!(updates[0].content.contains("## 1.0.0"));
    }

    #[test]
    fn test_simple_with_existing_version_file() {
        let test_repo = TestRepo::new();
        test_repo.write_file("version.txt", "0.9.0\n");

        let version = Version::new(1, 0, 0);
        let config = default_config();
        let strategy = SimpleStrategy;

        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.0.0\n", &config)
            .unwrap();

        assert_eq!(updates.len(), 2);
        let version_update = updates.iter().find(|u| u.path == "version.txt").unwrap();
        assert_eq!(version_update.content, "1.0.0\n");
    }

    #[test]
    fn test_simple_skip_changelog() {
        let test_repo = TestRepo::new();
        let version = Version::new(1, 0, 0);
        let mut config = default_config();
        config.skip_changelog = true;

        let strategy = SimpleStrategy;
        let updates = strategy
            .build_updates(test_repo.path(), ".", &version, "## 1.0.0\n", &config)
            .unwrap();

        assert!(updates.iter().all(|u| u.path != "CHANGELOG.md"));
    }
}
