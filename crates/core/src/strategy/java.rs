use regex::Regex;
use semver::Version;
use std::path::Path;
use std::sync::LazyLock;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{
    build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy,
};

/// Java/Maven strategy: updates pom.xml version and CHANGELOG.md.
pub struct JavaStrategy;

/// Matches the first `<version>x.y.z</version>` in a pom.xml.
/// This targets the project version (usually the first <version> element).
static POM_VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(<version>)([^<]+)(</version>)").unwrap());

impl ReleaseStrategy for JavaStrategy {
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

        // pom.xml — update the first <version> element
        let pom_path = join_pkg_path(pkg_path, "pom.xml");
        let pom_full = repo_path.join(&pom_path);
        if pom_full.exists() {
            let content = std::fs::read_to_string(&pom_full)?;
            // Replace only the first occurrence (project version, not parent/dep versions)
            let updated = POM_VERSION_RE
                .replacen(&content, 1, |caps: &regex::Captures| {
                    format!("{}{}{}", &caps[1], version_str, &caps[3])
                })
                .to_string();
            updates.push(FileUpdate {
                path: pom_path,
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

    fn java_config() -> ResolvedConfig {
        let defaults = crate::config::ReleaserConfig {
            release_type: Some("java".to_string()),
            ..Default::default()
        };
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_java_pom_xml() {
        let repo = TestRepo::new();
        repo.write_file(
            "pom.xml",
            r#"<?xml version="1.0"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>my-app</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>4.13</version>
    </dependency>
  </dependencies>
</project>
"#,
        );

        let strategy = JavaStrategy;
        let updates = strategy
            .build_updates(
                repo.path(),
                ".",
                &Version::new(1, 1, 0),
                "## 1.1.0\n",
                &java_config(),
            )
            .unwrap();

        let pom = updates.iter().find(|u| u.path == "pom.xml").unwrap();
        // Project version updated
        assert!(pom.content.contains("<version>1.1.0</version>"));
        // Dependency version NOT updated (only first <version> replaced)
        assert!(pom.content.contains("<version>4.13</version>"));
    }
}
