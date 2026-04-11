use regex::Regex;
use semver::Version;
use std::path::Path;
use std::sync::LazyLock;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy};

/// Helm strategy: updates Chart.yaml version field and CHANGELOG.md.
pub struct HelmStrategy;

static CHART_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(version:\s*)(.+)$").unwrap()
});

impl ReleaseStrategy for HelmStrategy {
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

        let chart_path = join_pkg_path(pkg_path, "Chart.yaml");
        let chart_full = repo_path.join(&chart_path);
        if chart_full.exists() {
            let content = std::fs::read_to_string(&chart_full)?;
            let updated = CHART_VERSION_RE
                .replace(&content, |caps: &regex::Captures| {
                    format!("{}{}", &caps[1], version_str)
                })
                .to_string();
            updates.push(FileUpdate {
                path: chart_path,
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

    fn helm_config() -> ResolvedConfig {
        let mut defaults = crate::config::ReleaserConfig::default();
        defaults.release_type = Some("helm".to_string());
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_helm_chart_yaml() {
        let repo = TestRepo::new();
        repo.write_file("Chart.yaml", "apiVersion: v2\nname: my-chart\nversion: 1.0.0\nappVersion: \"1.0\"\n");

        let strategy = HelmStrategy;
        let updates = strategy.build_updates(
            repo.path(), ".", &Version::new(1, 0, 1), "## 1.0.1\n", &helm_config(),
        ).unwrap();

        let chart = updates.iter().find(|u| u.path == "Chart.yaml").unwrap();
        assert!(chart.content.contains("version: 1.0.1"));
        assert!(chart.content.contains("name: my-chart"));
        // appVersion should not be changed
        assert!(chart.content.contains("appVersion: \"1.0\""));
    }
}
