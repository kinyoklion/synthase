use regex::Regex;
use semver::Version;
use std::path::Path;
use std::sync::LazyLock;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy};

/// Bazel strategy: updates MODULE.bazel version and CHANGELOG.md.
pub struct BazelStrategy;

/// Matches version in MODULE.bazel: version = "1.0.0"
static MODULE_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(version\s*=\s*")([^"]+)(")"#).unwrap()
});

impl ReleaseStrategy for BazelStrategy {
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

        let module_path = join_pkg_path(pkg_path, "MODULE.bazel");
        let module_full = repo_path.join(&module_path);
        if module_full.exists() {
            let content = std::fs::read_to_string(&module_full)?;
            // Replace only the first version (module version, not dep versions)
            let updated = MODULE_VERSION_RE
                .replacen(&content, 1, |caps: &regex::Captures| {
                    format!("{}{}{}", &caps[1], version_str, &caps[3])
                })
                .to_string();
            updates.push(FileUpdate {
                path: module_path,
                content: updated,
                create_if_missing: false,
            });
        }

        updates.extend(build_extra_file_updates(repo_path, pkg_path, new_version, &config.extra_files)?);

        Ok(updates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestRepo;

    fn bazel_config() -> ResolvedConfig {
        let mut defaults = crate::config::ReleaserConfig::default();
        defaults.release_type = Some("bazel".to_string());
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_bazel_module_bazel() {
        let repo = TestRepo::new();
        repo.write_file("MODULE.bazel", "module(\n    name = \"my_module\",\n    version = \"1.0.0\",\n)\n\nbazel_dep(name = \"rules_go\", version = \"0.40.0\")\n");

        let strategy = BazelStrategy;
        let updates = strategy.build_updates(
            repo.path(), ".", &Version::new(1, 1, 0), "## 1.1.0\n", &bazel_config(),
        ).unwrap();

        let module = updates.iter().find(|u| u.path == "MODULE.bazel").unwrap();
        // Module version updated
        assert!(module.content.contains("version = \"1.1.0\""));
        // Dependency version NOT updated
        assert!(module.content.contains("version = \"0.40.0\""));
    }
}
