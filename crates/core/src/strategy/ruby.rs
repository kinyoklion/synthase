use regex::Regex;
use semver::Version;
use std::path::Path;
use std::sync::LazyLock;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{build_changelog_update, build_extra_file_updates, FileUpdate, ReleaseStrategy};

/// Ruby strategy: updates version.rb and CHANGELOG.md.
pub struct RubyStrategy;

/// Matches version strings in Ruby files: VERSION = "1.0.0" or version = '1.0.0'
static RUBY_VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(["'])(\d+\.\d+\.\d+[^"']*)(["'])"#).unwrap());

impl ReleaseStrategy for RubyStrategy {
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

        // Look for lib/**/version.rb or version file from config
        let version_file = config
            .version_file
            .as_deref()
            .unwrap_or("lib/**/version.rb");
        let full_pattern = if pkg_path == "." {
            repo_path.join(version_file)
        } else {
            repo_path.join(pkg_path).join(version_file)
        };

        let pattern_str = full_pattern.to_string_lossy().to_string();
        if let Ok(paths) = glob::glob(&pattern_str) {
            for path_result in paths.flatten() {
                if let Ok(content) = std::fs::read_to_string(&path_result) {
                    let updated = RUBY_VERSION_RE
                        .replace(&content, |caps: &regex::Captures| {
                            format!("{}{}{}", &caps[1], version_str, &caps[3])
                        })
                        .to_string();
                    if updated != content {
                        let rel_path = path_result
                            .strip_prefix(repo_path)
                            .unwrap_or(&path_result)
                            .to_string_lossy()
                            .to_string();
                        updates.push(FileUpdate {
                            path: rel_path,
                            content: updated,
                            create_if_missing: false,
                        });
                    }
                }
            }
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

    fn ruby_config() -> ResolvedConfig {
        let defaults = crate::config::ReleaserConfig {
            release_type: Some("ruby".to_string()),
            ..Default::default()
        };
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_ruby_version_rb() {
        let repo = TestRepo::new();
        repo.write_file(
            "lib/my_gem/version.rb",
            "module MyGem\n  VERSION = \"1.0.0\"\nend\n",
        );

        let strategy = RubyStrategy;
        let updates = strategy
            .build_updates(
                repo.path(),
                ".",
                &Version::new(1, 1, 0),
                "## 1.1.0\n",
                &ruby_config(),
            )
            .unwrap();

        let version_rb = updates
            .iter()
            .find(|u| u.path.ends_with("version.rb"))
            .unwrap();
        assert!(version_rb.content.contains("\"1.1.0\""));
        assert!(version_rb.content.contains("module MyGem"));
    }
}
