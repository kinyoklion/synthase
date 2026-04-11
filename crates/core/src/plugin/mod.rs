//! Plugin system for post-processing releases.
//!
//! Plugins run after individual package releases are computed and can
//! modify, add, or merge releases to handle cross-package concerns.

pub mod cargo_workspace;
pub mod linked_versions;
pub mod node_workspace;
pub mod sentence_case;

use std::collections::HashMap;
use std::path::Path;

use crate::config::ManifestConfig;
use crate::error::Result;
use crate::manifest::ComponentRelease;

/// A plugin that post-processes computed releases.
pub trait Plugin {
    fn run(
        &self,
        repo_path: &Path,
        releases: Vec<ComponentRelease>,
        manifest_config: &ManifestConfig,
        manifest_versions: &HashMap<String, String>,
    ) -> Result<Vec<ComponentRelease>>;
}

/// Run all configured plugins in sequence.
pub fn run_plugins(
    repo_path: &Path,
    mut releases: Vec<ComponentRelease>,
    manifest_config: &ManifestConfig,
    manifest_versions: &HashMap<String, String>,
) -> Result<Vec<ComponentRelease>> {
    let plugins = create_plugins(manifest_config);

    for plugin in &plugins {
        releases = plugin.run(repo_path, releases, manifest_config, manifest_versions)?;
    }

    Ok(releases)
}

/// Parse plugin configurations and instantiate plugins.
fn create_plugins(config: &ManifestConfig) -> Vec<Box<dyn Plugin>> {
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();

    let plugin_configs = match &config.plugins {
        Some(p) => p,
        None => return plugins,
    };

    for plugin_value in plugin_configs {
        if let Some(name) = plugin_value.as_str() {
            // Simple string plugin name
            if let Some(p) = create_plugin_by_name(name, plugin_value) {
                plugins.push(p);
            }
        } else if let Some(obj) = plugin_value.as_object() {
            // Object with "type" field
            if let Some(type_name) = obj.get("type").and_then(|v| v.as_str()) {
                if let Some(p) = create_plugin_by_name(type_name, plugin_value) {
                    plugins.push(p);
                }
            }
        }
    }

    plugins
}

fn create_plugin_by_name(
    name: &str,
    config: &serde_json::Value,
) -> Option<Box<dyn Plugin>> {
    match name {
        "cargo-workspace" => Some(Box::new(cargo_workspace::CargoWorkspacePlugin::from_config(config))),
        "node-workspace" => Some(Box::new(node_workspace::NodeWorkspacePlugin::from_config(config))),
        "linked-versions" => linked_versions::LinkedVersionsPlugin::from_config(config)
            .map(|p| Box::new(p) as Box<dyn Plugin>),
        "sentence-case" => Some(Box::new(sentence_case::SentenceCasePlugin::from_config(config))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_plugins_empty() {
        let config: ManifestConfig =
            serde_json::from_str(r#"{"packages": {".": {}}}"#).unwrap();
        let plugins = create_plugins(&config);
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_create_plugins_cargo_workspace() {
        let config: ManifestConfig = serde_json::from_str(
            r#"{"packages": {".": {}}, "plugins": ["cargo-workspace"]}"#,
        )
        .unwrap();
        let plugins = create_plugins(&config);
        assert_eq!(plugins.len(), 1);
    }

    #[test]
    fn test_create_plugins_linked_versions() {
        let config: ManifestConfig = serde_json::from_str(
            r#"{"packages": {".": {}}, "plugins": [{"type": "linked-versions", "groupName": "core", "components": ["a", "b"]}]}"#,
        )
        .unwrap();
        let plugins = create_plugins(&config);
        assert_eq!(plugins.len(), 1);
    }

    #[test]
    fn test_create_plugins_unknown_ignored() {
        let config: ManifestConfig = serde_json::from_str(
            r#"{"packages": {".": {}}, "plugins": ["nonexistent"]}"#,
        )
        .unwrap();
        let plugins = create_plugins(&config);
        assert!(plugins.is_empty());
    }
}
