use regex::Regex;
use semver::Version;
use std::path::Path;
use std::sync::LazyLock;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy};

/// Elixir strategy: updates mix.exs version and CHANGELOG.md.
pub struct ElixirStrategy;

/// Matches version in mix.exs: version: "1.0.0"
static MIX_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(version:\s*")([^"]+)(")"#).unwrap()
});

impl ReleaseStrategy for ElixirStrategy {
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

        let mix_path = join_pkg_path(pkg_path, "mix.exs");
        let mix_full = repo_path.join(&mix_path);
        if mix_full.exists() {
            let content = std::fs::read_to_string(&mix_full)?;
            let updated = MIX_VERSION_RE
                .replace(&content, |caps: &regex::Captures| {
                    format!("{}{}{}", &caps[1], version_str, &caps[3])
                })
                .to_string();
            updates.push(FileUpdate {
                path: mix_path,
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

    fn elixir_config() -> ResolvedConfig {
        let mut defaults = crate::config::ReleaserConfig::default();
        defaults.release_type = Some("elixir".to_string());
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_elixir_mix_exs() {
        let repo = TestRepo::new();
        repo.write_file("mix.exs", "defmodule MyApp.MixProject do\n  def project do\n    [\n      app: :my_app,\n      version: \"1.0.0\",\n      elixir: \"~> 1.14\"\n    ]\n  end\nend\n");

        let strategy = ElixirStrategy;
        let updates = strategy.build_updates(
            repo.path(), ".", &Version::new(1, 1, 0), "## 1.1.0\n", &elixir_config(),
        ).unwrap();

        let mix = updates.iter().find(|u| u.path == "mix.exs").unwrap();
        assert!(mix.content.contains("version: \"1.1.0\""));
        assert!(mix.content.contains("app: :my_app"));
    }
}
