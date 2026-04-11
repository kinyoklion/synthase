use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{Error, Result};

/// A changelog section mapping: commit type → section heading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChangelogSection {
    #[serde(rename = "type")]
    pub commit_type: String,
    pub section: String,
    #[serde(default)]
    pub hidden: bool,
}

/// An extra file to update with the new version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ExtraFile {
    /// Simple path string — uses annotation-based generic updater.
    Simple(String),
    /// Typed file update with jsonpath or xpath.
    Typed(ExtraFileTyped),
}

/// A typed extra file update specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtraFileTyped {
    #[serde(rename = "type")]
    pub file_type: String,
    pub path: String,
    #[serde(default)]
    pub glob: bool,
    pub jsonpath: Option<String>,
    pub xpath: Option<String>,
}

/// Per-package configuration options. Also used for root-level defaults.
///
/// All fields are optional — `None` means "inherit from parent/defaults".
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case", default)]
pub struct ReleaserConfig {
    pub release_type: Option<String>,
    pub versioning: Option<String>,
    pub bump_minor_pre_major: Option<bool>,
    pub bump_patch_for_minor_pre_major: Option<bool>,
    pub prerelease_type: Option<String>,
    pub changelog_sections: Option<Vec<ChangelogSection>>,
    pub changelog_path: Option<String>,
    pub changelog_type: Option<String>,
    pub changelog_host: Option<String>,
    pub include_component_in_tag: Option<bool>,
    pub include_v_in_tag: Option<bool>,
    pub include_v_in_release_name: Option<bool>,
    pub tag_separator: Option<String>,
    pub pull_request_title_pattern: Option<String>,
    pub pull_request_header: Option<String>,
    pub pull_request_footer: Option<String>,
    pub separate_pull_requests: Option<bool>,
    pub extra_files: Option<Vec<ExtraFile>>,
    pub exclude_paths: Option<Vec<String>>,
    pub draft: Option<bool>,
    pub draft_pull_request: Option<bool>,
    pub prerelease: Option<bool>,
    pub skip_github_release: Option<bool>,
    pub skip_changelog: Option<bool>,
    pub initial_version: Option<String>,
    pub release_as: Option<String>,
    pub component: Option<String>,
    pub package_name: Option<String>,
    pub version_file: Option<String>,
    pub date_format: Option<String>,
    pub component_no_space: Option<bool>,
    pub extra_label: Option<String>,
    pub include_commit_authors: Option<bool>,
    pub snapshot_label: Option<String>,
    pub skip_snapshot: Option<bool>,
    pub force_tag_creation: Option<bool>,
}

/// Top-level `release-please-config.json` structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ManifestConfig {
    /// Per-path package configuration.
    pub packages: HashMap<String, ReleaserConfig>,

    /// Root-level defaults (flattened into the same JSON object).
    #[serde(flatten)]
    pub defaults: ReleaserConfig,

    // --- Manifest-only options ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_sha: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_release_sha: Option<String>,

    /// Plugin configurations. Parsed as raw JSON values; full plugin
    /// deserialization is deferred to Phase 6.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<Vec<serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signoff: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_pull_request_title_pattern: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_search_depth: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_search_depth: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_batch_size: Option<u32>,

    /// Comma-separated labels for pending release PRs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Comma-separated labels for tagged/released PRs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequential_calls: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_update: Option<bool>,

    /// JSON schema reference (ignored at runtime).
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
}

/// Fully resolved configuration for a single package, with all defaults applied.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub release_type: String,
    pub versioning: String,
    pub bump_minor_pre_major: bool,
    pub bump_patch_for_minor_pre_major: bool,
    pub prerelease_type: Option<String>,
    pub changelog_sections: Option<Vec<ChangelogSection>>,
    pub changelog_path: String,
    pub changelog_type: String,
    pub changelog_host: Option<String>,
    pub include_component_in_tag: bool,
    pub include_v_in_tag: bool,
    pub include_v_in_release_name: bool,
    pub tag_separator: String,
    pub pull_request_title_pattern: Option<String>,
    pub pull_request_header: Option<String>,
    pub pull_request_footer: Option<String>,
    pub separate_pull_requests: bool,
    pub extra_files: Vec<ExtraFile>,
    pub exclude_paths: Vec<String>,
    pub draft: bool,
    pub draft_pull_request: bool,
    pub prerelease: bool,
    pub skip_github_release: bool,
    pub skip_changelog: bool,
    pub initial_version: Option<String>,
    pub release_as: Option<String>,
    pub component: Option<String>,
    pub package_name: Option<String>,
    pub version_file: Option<String>,
    pub date_format: Option<String>,
    pub component_no_space: bool,
    pub include_commit_authors: bool,
    pub force_tag_creation: bool,
}

/// Load and parse `release-please-config.json` from disk.
pub fn load_config(path: &Path) -> Result<ManifestConfig> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!(
            "failed to read config file {}: {}",
            path.display(),
            e
        ))
    })?;
    let config: ManifestConfig = serde_json::from_str(&content)?;
    Ok(config)
}

/// Load and parse `.release-please-manifest.json` from disk.
///
/// Returns a map of path → version string.
pub fn load_manifest(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!(
            "failed to read manifest file {}: {}",
            path.display(),
            e
        ))
    })?;
    let manifest: HashMap<String, String> = serde_json::from_str(&content)?;
    Ok(manifest)
}

/// Merge root defaults with per-package overrides, applying hardcoded fallbacks.
///
/// Priority: package config > root defaults > hardcoded defaults.
pub fn resolve_config(defaults: &ReleaserConfig, package: &ReleaserConfig) -> ResolvedConfig {
    // Helper: pick package value, then default value, then fallback
    macro_rules! resolve {
        ($field:ident, $fallback:expr) => {
            package
                .$field
                .clone()
                .or_else(|| defaults.$field.clone())
                .unwrap_or_else(|| $fallback)
        };
    }
    macro_rules! resolve_bool {
        ($field:ident, $fallback:expr) => {
            package.$field.or(defaults.$field).unwrap_or($fallback)
        };
    }

    ResolvedConfig {
        release_type: resolve!(release_type, "node".to_string()),
        versioning: resolve!(versioning, "default".to_string()),
        bump_minor_pre_major: resolve_bool!(bump_minor_pre_major, false),
        bump_patch_for_minor_pre_major: resolve_bool!(bump_patch_for_minor_pre_major, false),
        prerelease_type: package
            .prerelease_type
            .clone()
            .or_else(|| defaults.prerelease_type.clone()),
        changelog_sections: package
            .changelog_sections
            .clone()
            .or_else(|| defaults.changelog_sections.clone()),
        changelog_path: resolve!(changelog_path, "CHANGELOG.md".to_string()),
        changelog_type: resolve!(changelog_type, "default".to_string()),
        changelog_host: package
            .changelog_host
            .clone()
            .or_else(|| defaults.changelog_host.clone()),
        include_component_in_tag: resolve_bool!(include_component_in_tag, true),
        include_v_in_tag: resolve_bool!(include_v_in_tag, true),
        include_v_in_release_name: resolve_bool!(include_v_in_release_name, true),
        tag_separator: resolve!(tag_separator, "-".to_string()),
        pull_request_title_pattern: package
            .pull_request_title_pattern
            .clone()
            .or_else(|| defaults.pull_request_title_pattern.clone()),
        pull_request_header: package
            .pull_request_header
            .clone()
            .or_else(|| defaults.pull_request_header.clone()),
        pull_request_footer: package
            .pull_request_footer
            .clone()
            .or_else(|| defaults.pull_request_footer.clone()),
        separate_pull_requests: resolve_bool!(separate_pull_requests, false),
        extra_files: package
            .extra_files
            .clone()
            .or_else(|| defaults.extra_files.clone())
            .unwrap_or_default(),
        exclude_paths: package
            .exclude_paths
            .clone()
            .or_else(|| defaults.exclude_paths.clone())
            .unwrap_or_default(),
        draft: resolve_bool!(draft, false),
        draft_pull_request: resolve_bool!(draft_pull_request, false),
        prerelease: resolve_bool!(prerelease, false),
        skip_github_release: resolve_bool!(skip_github_release, false),
        skip_changelog: resolve_bool!(skip_changelog, false),
        initial_version: package
            .initial_version
            .clone()
            .or_else(|| defaults.initial_version.clone()),
        release_as: package
            .release_as
            .clone()
            .or_else(|| defaults.release_as.clone()),
        component: package
            .component
            .clone()
            .or_else(|| defaults.component.clone()),
        package_name: package
            .package_name
            .clone()
            .or_else(|| defaults.package_name.clone()),
        version_file: package
            .version_file
            .clone()
            .or_else(|| defaults.version_file.clone()),
        date_format: package
            .date_format
            .clone()
            .or_else(|| defaults.date_format.clone()),
        component_no_space: resolve_bool!(component_no_space, false),
        include_commit_authors: resolve_bool!(include_commit_authors, false),
        force_tag_creation: resolve_bool!(force_tag_creation, false),
    }
}

/// Parse a comma-separated label string into a Vec of trimmed labels.
pub fn parse_labels(label_str: &str) -> Vec<String> {
    label_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Default labels for pending release PRs.
pub const DEFAULT_LABELS: &[&str] = &["autorelease: pending"];
/// Default labels for tagged/released PRs.
pub const DEFAULT_RELEASE_LABELS: &[&str] = &["autorelease: tagged"];
/// Default release search depth.
pub const DEFAULT_RELEASE_SEARCH_DEPTH: u32 = 400;
/// Default commit search depth.
pub const DEFAULT_COMMIT_SEARCH_DEPTH: u32 = 500;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_minimal_config() {
        let json = r#"{
            "packages": {
                ".": {}
            }
        }"#;
        let config: ManifestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.packages.len(), 1);
        assert!(config.packages.contains_key("."));
        assert_eq!(config.defaults.release_type, None);
    }

    #[test]
    fn test_parse_config_with_defaults() {
        let json = r#"{
            "release-type": "rust",
            "bump-minor-pre-major": true,
            "include-v-in-tag": true,
            "packages": {
                ".": {},
                "packages/foo": {
                    "release-type": "node",
                    "component": "foo"
                }
            }
        }"#;
        let config: ManifestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.defaults.release_type.as_deref(), Some("rust"));
        assert_eq!(config.defaults.bump_minor_pre_major, Some(true));
        assert_eq!(
            config.packages["packages/foo"].release_type.as_deref(),
            Some("node")
        );
        assert_eq!(
            config.packages["packages/foo"].component.as_deref(),
            Some("foo")
        );
    }

    #[test]
    fn test_parse_config_with_changelog_sections() {
        let json = r#"{
            "changelog-sections": [
                {"type": "feat", "section": "Features"},
                {"type": "fix", "section": "Bug Fixes"},
                {"type": "chore", "section": "Chores", "hidden": true}
            ],
            "packages": {".": {}}
        }"#;
        let config: ManifestConfig = serde_json::from_str(json).unwrap();
        let sections = config.defaults.changelog_sections.unwrap();
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].commit_type, "feat");
        assert_eq!(sections[0].section, "Features");
        assert!(!sections[0].hidden);
        assert!(sections[2].hidden);
    }

    #[test]
    fn test_parse_config_with_extra_files() {
        let json = r#"{
            "extra-files": [
                "version.txt",
                {"type": "json", "path": "config.json", "jsonpath": "$.version"},
                {"type": "xml", "path": "pom.xml", "xpath": "//version"}
            ],
            "packages": {".": {}}
        }"#;
        let config: ManifestConfig = serde_json::from_str(json).unwrap();
        let files = config.defaults.extra_files.unwrap();
        assert_eq!(files.len(), 3);
        assert!(matches!(&files[0], ExtraFile::Simple(s) if s == "version.txt"));
        assert!(matches!(&files[1], ExtraFile::Typed(t) if t.file_type == "json"));
    }

    #[test]
    fn test_parse_config_with_plugins() {
        let json = r#"{
            "plugins": [
                "sentence-case",
                {"type": "linked-versions", "groupName": "core", "components": ["a", "b"]}
            ],
            "packages": {".": {}}
        }"#;
        let config: ManifestConfig = serde_json::from_str(json).unwrap();
        let plugins = config.plugins.unwrap();
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0], serde_json::json!("sentence-case"));
    }

    #[test]
    fn test_parse_config_with_manifest_options() {
        let json = r#"{
            "bootstrap-sha": "abc123",
            "last-release-sha": "def456",
            "release-search-depth": 200,
            "commit-search-depth": 300,
            "label": "release: pending, bot",
            "release-label": "release: tagged",
            "sequential-calls": true,
            "always-update": false,
            "signoff": "Signed-off-by: Bot",
            "group-pull-request-title-pattern": "chore: release ${branch}",
            "packages": {".": {}}
        }"#;
        let config: ManifestConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.bootstrap_sha.as_deref(), Some("abc123"));
        assert_eq!(config.last_release_sha.as_deref(), Some("def456"));
        assert_eq!(config.release_search_depth, Some(200));
        assert_eq!(config.commit_search_depth, Some(300));
        assert_eq!(config.label.as_deref(), Some("release: pending, bot"));
        assert_eq!(config.sequential_calls, Some(true));
        assert_eq!(config.signoff.as_deref(), Some("Signed-off-by: Bot"));
    }

    #[test]
    fn test_parse_manifest() {
        let json = r#"{"." : "1.2.3", "packages/foo": "0.5.0"}"#;
        let manifest: HashMap<String, String> = serde_json::from_str(json).unwrap();
        assert_eq!(manifest["."], "1.2.3");
        assert_eq!(manifest["packages/foo"], "0.5.0");
    }

    #[test]
    fn test_resolve_config_package_overrides_defaults() {
        let defaults = ReleaserConfig {
            release_type: Some("rust".to_string()),
            bump_minor_pre_major: Some(true),
            tag_separator: Some("/".to_string()),
            ..Default::default()
        };
        let package = ReleaserConfig {
            release_type: Some("node".to_string()),
            // bump_minor_pre_major not set — inherits from defaults
            tag_separator: Some("-".to_string()), // overrides
            ..Default::default()
        };

        let resolved = resolve_config(&defaults, &package);
        assert_eq!(resolved.release_type, "node"); // package wins
        assert!(resolved.bump_minor_pre_major); // inherited from defaults
        assert_eq!(resolved.tag_separator, "-"); // package wins
    }

    #[test]
    fn test_resolve_config_hardcoded_defaults() {
        let defaults = ReleaserConfig::default();
        let package = ReleaserConfig::default();

        let resolved = resolve_config(&defaults, &package);
        assert_eq!(resolved.release_type, "node");
        assert_eq!(resolved.versioning, "default");
        assert!(!resolved.bump_minor_pre_major);
        assert!(!resolved.bump_patch_for_minor_pre_major);
        assert_eq!(resolved.changelog_path, "CHANGELOG.md");
        assert_eq!(resolved.changelog_type, "default");
        assert!(resolved.include_component_in_tag);
        assert!(resolved.include_v_in_tag);
        assert_eq!(resolved.tag_separator, "-");
        assert!(!resolved.separate_pull_requests);
        assert!(!resolved.draft);
        assert!(!resolved.skip_github_release);
        assert!(!resolved.skip_changelog);
    }

    #[test]
    fn test_parse_labels() {
        assert_eq!(
            parse_labels("autorelease: pending, bot"),
            vec!["autorelease: pending", "bot"]
        );
        assert_eq!(parse_labels("single"), vec!["single"]);
        assert_eq!(
            parse_labels("  spaced , labels  "),
            vec!["spaced", "labels"]
        );
        assert!(parse_labels("").is_empty());
    }

    #[test]
    fn test_load_config_from_file() {
        // Use TestRepo to create a file on disk
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("release-please-config.json");
        std::fs::write(
            &config_path,
            r#"{"release-type": "rust", "packages": {".": {}}}"#,
        )
        .unwrap();

        let config = load_config(&config_path).unwrap();
        assert_eq!(config.defaults.release_type.as_deref(), Some("rust"));
    }

    #[test]
    fn test_load_config_missing_file() {
        let result = load_config(Path::new("/nonexistent/config.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_manifest_from_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let manifest_path = dir.path().join(".release-please-manifest.json");
        std::fs::write(&manifest_path, r#"{".": "1.0.0"}"#).unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        assert_eq!(manifest["."], "1.0.0");
    }

    #[test]
    fn test_roundtrip_serialize_config() {
        let json = r#"{"release-type":"rust","packages":{".":{"component":"my-lib"}}}"#;
        let config: ManifestConfig = serde_json::from_str(json).unwrap();
        let serialized = serde_json::to_string(&config).unwrap();
        let reparsed: ManifestConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(config, reparsed);
    }
}
