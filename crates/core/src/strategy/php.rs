use semver::Version;
use std::path::Path;

use crate::config::ResolvedConfig;
use crate::error::Result;
use crate::updater;

use super::{
    build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy,
};

/// PHP strategy: updates composer.json version and CHANGELOG.md.
pub struct PhpStrategy;

impl ReleaseStrategy for PhpStrategy {
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

        if let Some(cl) = build_changelog_update(repo_path, pkg_path, changelog_entry, config)? {
            updates.push(cl);
        }

        // composer.json (same JSON format as package.json)
        let composer_path = join_pkg_path(pkg_path, "composer.json");
        let composer_full = repo_path.join(&composer_path);
        if composer_full.exists() {
            let content = std::fs::read_to_string(&composer_full)?;
            let updated = updater::update_package_json_version(&content, &version_str);
            updates.push(FileUpdate {
                path: composer_path,
                content: updated,
                create_if_missing: false,
            });
        }

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

    fn php_config() -> ResolvedConfig {
        let defaults = crate::config::ReleaserConfig {
            release_type: Some("php".to_string()),
            ..Default::default()
        };
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_php_composer_json() {
        let repo = TestRepo::new();
        repo.write_file(
            "composer.json",
            "{\n  \"name\": \"vendor/pkg\",\n  \"version\": \"1.0.0\"\n}\n",
        );

        let strategy = PhpStrategy;
        let updates = strategy
            .build_updates(
                repo.path(),
                ".",
                &Version::new(1, 0, 1),
                "## 1.0.1\n",
                &php_config(),
            )
            .unwrap();

        let composer = updates.iter().find(|u| u.path == "composer.json").unwrap();
        assert!(composer.content.contains("\"version\": \"1.0.1\""));
    }
}
