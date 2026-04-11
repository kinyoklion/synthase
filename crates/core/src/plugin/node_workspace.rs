//! Node workspace plugin: cascades version bumps through npm workspace dependencies.

use semver::Version;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::changelog;
use crate::config::{self, ManifestConfig};
use crate::error::Result;
use crate::manifest::ComponentRelease;
use crate::strategy::FileUpdate;
use crate::tag::TagName;
use crate::updater;
use crate::version;

use super::Plugin;

/// Parsed package info from an npm workspace member.
#[derive(Debug, Clone)]
struct PackageInfo {
    name: String,
    path: String,
    version: Version,
    /// Names of workspace packages this package depends on.
    workspace_deps: Vec<String>,
}

pub struct NodeWorkspacePlugin {
    pub merge: bool,
    pub update_peer_dependencies: bool,
}

impl NodeWorkspacePlugin {
    pub fn from_config(config: &serde_json::Value) -> Self {
        let merge = config.get("merge").and_then(|v| v.as_bool()).unwrap_or(true);
        let update_peer = config
            .get("updatePeerDependencies")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        NodeWorkspacePlugin {
            merge,
            update_peer_dependencies: update_peer,
        }
    }
}

impl Plugin for NodeWorkspacePlugin {
    fn run(
        &self,
        repo_path: &Path,
        releases: Vec<ComponentRelease>,
        manifest_config: &ManifestConfig,
        _manifest_versions: &HashMap<String, String>,
    ) -> Result<Vec<ComponentRelease>> {
        let packages = parse_workspace(repo_path, manifest_config)?;
        if packages.is_empty() {
            return Ok(releases);
        }

        // Build map of package name → new version from existing releases
        let mut updated_versions: HashMap<String, Version> = HashMap::new();
        for release in &releases {
            if let Some(ref comp) = release.component {
                updated_versions.insert(comp.clone(), release.new_version.clone());
            }
            if let Some(ref pkg_name) = release.config.package_name {
                updated_versions.insert(pkg_name.clone(), release.new_version.clone());
            }
        }

        // Find packages needing cascade bumps
        let mut cascade_needed: HashMap<String, Version> = HashMap::new();
        for pkg in &packages {
            let needs_cascade = pkg.workspace_deps.iter().any(|dep| {
                updated_versions.contains_key(dep) || cascade_needed.contains_key(dep)
            });
            if needs_cascade && !updated_versions.contains_key(&pkg.name) {
                let new_version = version::bump(&pkg.version, version::BumpType::Patch);
                cascade_needed.insert(pkg.name.clone(), new_version);
            }
        }

        if cascade_needed.is_empty() {
            return Ok(releases);
        }

        let mut all_versions = updated_versions;
        all_versions.extend(cascade_needed.iter().map(|(k, v)| (k.clone(), v.clone())));

        // Update existing releases' package.json dep references
        let mut updated_releases: Vec<ComponentRelease> = releases
            .into_iter()
            .map(|mut release| {
                update_package_json_deps(&mut release, &all_versions);
                release
            })
            .collect();

        // Create cascade releases
        for pkg in &packages {
            if let Some(new_version) = cascade_needed.get(&pkg.name) {
                if let Some(release) = create_cascade_release(
                    pkg,
                    new_version,
                    repo_path,
                    &all_versions,
                    manifest_config,
                )? {
                    updated_releases.push(release);
                }
            }
        }

        Ok(updated_releases)
    }
}

/// Parse npm workspace members from package.json files.
fn parse_workspace(repo_path: &Path, manifest_config: &ManifestConfig) -> Result<Vec<PackageInfo>> {
    let mut packages = Vec::new();
    let all_names: HashSet<String> = HashSet::new();

    // Collect packages from the manifest config paths
    for pkg_path in manifest_config.packages.keys() {
        let pkg_json_path = if pkg_path == "." {
            repo_path.join("package.json")
        } else {
            repo_path.join(pkg_path).join("package.json")
        };

        if !pkg_json_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&pkg_json_path)?;
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let name = parsed.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
        let version_str = parsed.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        let version = Version::parse(version_str).unwrap_or_else(|_| Version::new(0, 0, 0));

        // Collect dependency names
        let mut deps = Vec::new();
        for section in &["dependencies", "devDependencies", "optionalDependencies"] {
            if let Some(dep_obj) = parsed.get(section).and_then(|d| d.as_object()) {
                for dep_name in dep_obj.keys() {
                    deps.push(dep_name.clone());
                }
            }
        }

        if !name.is_empty() {
            packages.push(PackageInfo {
                name,
                path: pkg_path.clone(),
                version,
                workspace_deps: deps,
            });
        }
    }

    // Filter workspace_deps to only include other workspace members
    let workspace_names: HashSet<String> = packages.iter().map(|p| p.name.clone()).collect();
    for pkg in &mut packages {
        pkg.workspace_deps.retain(|d| workspace_names.contains(d));
    }

    Ok(packages)
}

/// Update package.json dependency version references in an existing release.
fn update_package_json_deps(
    release: &mut ComponentRelease,
    all_versions: &HashMap<String, Version>,
) {
    for update in &mut release.file_updates {
        if update.path.ends_with("package.json") {
            if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(&update.content) {
                let mut changed = false;
                for section in &["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"] {
                    if let Some(deps) = parsed.get_mut(section).and_then(|d| d.as_object_mut()) {
                        for (dep_name, dep_val) in deps.iter_mut() {
                            if let Some(new_ver) = all_versions.get(dep_name) {
                                if let Some(current) = dep_val.as_str() {
                                    // Preserve version range prefix (^, ~, >=, etc.)
                                    let prefix = extract_range_prefix(current);
                                    *dep_val = serde_json::Value::String(
                                        format!("{}{}", prefix, new_ver),
                                    );
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                if changed {
                    let indent = detect_indent(&update.content);
                    if let Ok(new_content) = serde_json::to_string_pretty(&parsed) {
                        let mut result = if indent != "  " {
                            re_indent(&new_content, &indent)
                        } else {
                            new_content
                        };
                        if update.content.ends_with('\n') && !result.ends_with('\n') {
                            result.push('\n');
                        }
                        update.content = result;
                    }
                }
            }
        }
    }
}

/// Extract the version range prefix from a semver range string.
fn extract_range_prefix(range: &str) -> &str {
    if range.starts_with(">=") { ">=" }
    else if range.starts_with("<=") { "<=" }
    else if range.starts_with('^') { "^" }
    else if range.starts_with('~') { "~" }
    else if range.starts_with('>') { ">" }
    else if range.starts_with('<') { "<" }
    else if range.starts_with('=') { "=" }
    else { "" }
}

fn detect_indent(content: &str) -> String {
    for line in content.lines().skip(1) {
        if line.starts_with("  ") {
            let spaces = line.len() - line.trim_start().len();
            return " ".repeat(spaces);
        } else if line.starts_with('\t') {
            return "\t".to_string();
        }
    }
    "  ".to_string()
}

fn re_indent(json: &str, indent: &str) -> String {
    let mut result = String::with_capacity(json.len());
    for line in json.lines() {
        let stripped = line.trim_start();
        let leading = line.len() - stripped.len();
        let level = leading / 2;
        for _ in 0..level {
            result.push_str(indent);
        }
        result.push_str(stripped);
        result.push('\n');
    }
    if result.ends_with('\n') && !json.ends_with('\n') {
        result.pop();
    }
    result
}

/// Create a cascade release for a package not directly bumped.
fn create_cascade_release(
    pkg: &PackageInfo,
    new_version: &Version,
    repo_path: &Path,
    all_versions: &HashMap<String, Version>,
    manifest_config: &ManifestConfig,
) -> Result<Option<ComponentRelease>> {
    let pkg_config = manifest_config
        .packages
        .get(&pkg.path)
        .cloned()
        .unwrap_or_default();
    let resolved = config::resolve_config(&manifest_config.defaults, &pkg_config);

    let pkg_json_path = if pkg.path == "." {
        "package.json".to_string()
    } else {
        format!("{}/package.json", pkg.path)
    };

    let pkg_json_full = repo_path.join(&pkg_json_path);
    if !pkg_json_full.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&pkg_json_full)?;
    let updated = updater::update_package_json_version(&content, &new_version.to_string());

    let changelog_entry = format!(
        "## {} ({})\n\n### Dependencies\n\n* Updated workspace dependencies\n",
        new_version,
        chrono::Utc::now().format("%Y-%m-%d"),
    );

    let mut file_updates = vec![FileUpdate {
        path: pkg_json_path,
        content: updated,
        create_if_missing: false,
    }];

    if !resolved.skip_changelog {
        let cl_path = if pkg.path == "." {
            resolved.changelog_path.clone()
        } else {
            format!("{}/{}", pkg.path, resolved.changelog_path)
        };
        let cl_full = repo_path.join(&cl_path);
        let existing = if cl_full.exists() {
            std::fs::read_to_string(&cl_full)?
        } else {
            String::new()
        };
        let new_cl = changelog::update_changelog(&existing, &changelog_entry);
        file_updates.push(FileUpdate {
            path: cl_path,
            content: new_cl,
            create_if_missing: true,
        });
    }

    let tag = TagName::from_config(
        new_version.clone(),
        Some(pkg.name.clone()),
        resolved.include_component_in_tag,
        &resolved.tag_separator,
        resolved.include_v_in_tag,
    );

    Ok(Some(ComponentRelease {
        component: Some(pkg.name.clone()),
        package_path: pkg.path.clone(),
        current_version: Some(pkg.version.clone()),
        new_version: new_version.clone(),
        tag: tag.to_string(),
        changelog_entry,
        file_updates,
        config: resolved,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_range_prefix() {
        assert_eq!(extract_range_prefix("^1.0.0"), "^");
        assert_eq!(extract_range_prefix("~1.0.0"), "~");
        assert_eq!(extract_range_prefix(">=1.0.0"), ">=");
        assert_eq!(extract_range_prefix("1.0.0"), "");
    }

    #[test]
    fn test_cascade_detection() {
        let packages = vec![
            PackageInfo {
                name: "@scope/app".into(),
                path: "packages/app".into(),
                version: Version::new(1, 0, 0),
                workspace_deps: vec!["@scope/lib".into()],
            },
            PackageInfo {
                name: "@scope/lib".into(),
                path: "packages/lib".into(),
                version: Version::new(1, 0, 0),
                workspace_deps: vec![],
            },
        ];

        let mut updated_versions: HashMap<String, Version> = HashMap::new();
        updated_versions.insert("@scope/lib".into(), Version::new(1, 1, 0));

        let mut cascade_needed: HashMap<String, Version> = HashMap::new();
        for pkg in &packages {
            let needs_cascade = pkg.workspace_deps.iter().any(|dep| {
                updated_versions.contains_key(dep)
            });
            if needs_cascade && !updated_versions.contains_key(&pkg.name) {
                let new_ver = version::bump(&pkg.version, version::BumpType::Patch);
                cascade_needed.insert(pkg.name.clone(), new_ver);
            }
        }

        assert!(cascade_needed.contains_key("@scope/app"));
        assert_eq!(cascade_needed["@scope/app"], Version::new(1, 0, 1));
    }
}
