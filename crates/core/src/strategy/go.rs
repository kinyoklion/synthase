use semver::Version;
use std::path::Path;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{build_changelog_update, build_extra_file_updates, FileUpdate, ReleaseStrategy};

/// Go strategy: version comes from tags, only updates CHANGELOG.md.
pub struct GoStrategy;

impl ReleaseStrategy for GoStrategy {
    fn build_updates(
        &self,
        repo_path: &Path,
        pkg_path: &str,
        new_version: &Version,
        changelog_entry: &str,
        config: &ResolvedConfig,
    ) -> Result<Vec<FileUpdate>> {
        let mut updates = Vec::new();

        if let Some(cl) = build_changelog_update(repo_path, pkg_path, changelog_entry, config)? {
            updates.push(cl);
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

    fn go_config() -> ResolvedConfig {
        let defaults = crate::config::ReleaserConfig {
            release_type: Some("go".to_string()),
            ..Default::default()
        };
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_go_changelog_only() {
        let repo = TestRepo::new();
        repo.write_file("go.mod", "module example.com/mymod\n\ngo 1.21\n");

        let strategy = GoStrategy;
        let updates = strategy
            .build_updates(
                repo.path(),
                ".",
                &Version::new(1, 1, 0),
                "## 1.1.0\n\n### Features\n\n* thing\n",
                &go_config(),
            )
            .unwrap();

        // Only changelog, no go.mod update (version comes from tags)
        assert!(updates.iter().any(|u| u.path == "CHANGELOG.md"));
        assert!(!updates.iter().any(|u| u.path == "go.mod"));
    }
}
