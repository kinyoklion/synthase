//! Linked versions plugin: groups components to share the same version.

use semver::Version;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ManifestConfig;
use crate::error::Result;
use crate::manifest::ComponentRelease;
use crate::tag::TagName;

use super::Plugin;

pub struct LinkedVersionsPlugin {
    pub group_name: String,
    pub components: Vec<String>,
    pub merge: bool,
}

impl LinkedVersionsPlugin {
    pub fn from_config(config: &serde_json::Value) -> Option<Self> {
        let group_name = config.get("groupName")?.as_str()?.to_string();
        let components: Vec<String> = config
            .get("components")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        if components.is_empty() {
            return None;
        }

        let merge = config.get("merge").and_then(|v| v.as_bool()).unwrap_or(true);

        Some(LinkedVersionsPlugin {
            group_name,
            components,
            merge,
        })
    }
}

impl Plugin for LinkedVersionsPlugin {
    fn run(
        &self,
        _repo_path: &Path,
        releases: Vec<ComponentRelease>,
        _manifest_config: &ManifestConfig,
        _manifest_versions: &HashMap<String, String>,
    ) -> Result<Vec<ComponentRelease>> {
        // Find the highest version among grouped components
        let mut highest_version: Option<Version> = None;

        for release in &releases {
            let component = release.component.as_deref().unwrap_or("");
            if self.components.iter().any(|c| c == component) {
                match &highest_version {
                    None => highest_version = Some(release.new_version.clone()),
                    Some(current) => {
                        if release.new_version > *current {
                            highest_version = Some(release.new_version.clone());
                        }
                    }
                }
            }
        }

        let highest = match highest_version {
            Some(v) => v,
            None => return Ok(releases), // no grouped releases found
        };

        // Update all grouped releases to the highest version
        let updated: Vec<ComponentRelease> = releases
            .into_iter()
            .map(|mut release| {
                let component = release.component.as_deref().unwrap_or("");
                if self.components.iter().any(|c| c == component) {
                    if release.new_version < highest {
                        release.new_version = highest.clone();

                        // Update tag
                        let tag = TagName::from_config(
                            highest.clone(),
                            release.component.clone(),
                            release.config.include_component_in_tag,
                            &release.config.tag_separator,
                            release.config.include_v_in_tag,
                        );
                        release.tag = tag.to_string();
                    }
                }
                release
            })
            .collect();

        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn make_release(component: &str, version: &str) -> ComponentRelease {
        let resolved = config::resolve_config(
            &config::ReleaserConfig::default(),
            &config::ReleaserConfig::default(),
        );
        ComponentRelease {
            component: Some(component.to_string()),
            package_path: format!("packages/{}", component),
            current_version: Some(Version::new(1, 0, 0)),
            new_version: Version::parse(version).unwrap(),
            tag: format!("{}-v{}", component, version),
            changelog_entry: String::new(),
            file_updates: vec![],
            config: resolved,
        }
    }

    #[test]
    fn test_linked_versions_upgrades_to_highest() {
        let plugin = LinkedVersionsPlugin {
            group_name: "core".to_string(),
            components: vec!["a".to_string(), "b".to_string()],
            merge: false,
        };

        let releases = vec![
            make_release("a", "1.1.0"),  // minor bump
            make_release("b", "1.0.1"),  // patch bump
        ];

        let result = plugin
            .run(Path::new("."), releases, &minimal_config(), &HashMap::new())
            .unwrap();

        assert_eq!(result.len(), 2);
        // Both should be at 1.1.0 (highest)
        assert_eq!(result[0].new_version, Version::new(1, 1, 0));
        assert_eq!(result[1].new_version, Version::new(1, 1, 0));
    }

    #[test]
    fn test_linked_versions_no_change_when_equal() {
        let plugin = LinkedVersionsPlugin {
            group_name: "core".to_string(),
            components: vec!["a".to_string(), "b".to_string()],
            merge: false,
        };

        let releases = vec![
            make_release("a", "1.1.0"),
            make_release("b", "1.1.0"),
        ];

        let result = plugin
            .run(Path::new("."), releases, &minimal_config(), &HashMap::new())
            .unwrap();

        assert_eq!(result[0].new_version, Version::new(1, 1, 0));
        assert_eq!(result[1].new_version, Version::new(1, 1, 0));
    }

    #[test]
    fn test_linked_versions_ignores_non_grouped() {
        let plugin = LinkedVersionsPlugin {
            group_name: "core".to_string(),
            components: vec!["a".to_string(), "b".to_string()],
            merge: false,
        };

        let releases = vec![
            make_release("a", "1.1.0"),
            make_release("b", "1.0.1"),
            make_release("c", "3.0.0"),  // not in group
        ];

        let result = plugin
            .run(Path::new("."), releases, &minimal_config(), &HashMap::new())
            .unwrap();

        assert_eq!(result[2].new_version, Version::new(3, 0, 0)); // unchanged
    }

    fn minimal_config() -> ManifestConfig {
        serde_json::from_str(r#"{"packages": {".": {}}}"#).unwrap()
    }
}
