//! Cargo workspace plugin: cascades version bumps through workspace dependencies.

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

/// Parsed crate info from a workspace member's Cargo.toml.
#[derive(Debug, Clone)]
struct CrateInfo {
    name: String,
    path: String,
    version: Version,
    /// Names of workspace crates this crate depends on.
    workspace_deps: Vec<String>,
}

pub struct CargoWorkspacePlugin {
    pub merge: bool,
}

impl CargoWorkspacePlugin {
    pub fn from_config(config: &serde_json::Value) -> Self {
        let merge = config
            .get("merge")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        CargoWorkspacePlugin { merge }
    }
}

impl Plugin for CargoWorkspacePlugin {
    fn run(
        &self,
        repo_path: &Path,
        releases: Vec<ComponentRelease>,
        manifest_config: &ManifestConfig,
        manifest_versions: &HashMap<String, String>,
    ) -> Result<Vec<ComponentRelease>> {
        // Parse workspace structure
        let crates = parse_workspace(repo_path)?;
        if crates.is_empty() {
            return Ok(releases);
        }

        // Build map of crate name → new version from existing releases
        let mut updated_versions: HashMap<String, Version> = HashMap::new();
        for release in &releases {
            if let Some(ref comp) = release.component {
                updated_versions.insert(comp.clone(), release.new_version.clone());
            }
            // Also try package_name from config
            if let Some(ref pkg_name) = release.config.package_name {
                updated_versions.insert(pkg_name.clone(), release.new_version.clone());
            }
        }

        // Find crates that need cascade bumps
        let mut cascade_needed: HashMap<String, Version> = HashMap::new();
        let ordered = topological_sort(&crates);

        for crate_info in &ordered {
            let needs_cascade = crate_info
                .workspace_deps
                .iter()
                .any(|dep| updated_versions.contains_key(dep) || cascade_needed.contains_key(dep));

            if needs_cascade && !updated_versions.contains_key(&crate_info.name) {
                // This crate needs a cascade bump
                let new_version = version::bump(&crate_info.version, version::BumpType::Patch);
                cascade_needed.insert(crate_info.name.clone(), new_version);
            }
        }

        if cascade_needed.is_empty() && updated_versions.is_empty() {
            return Ok(releases);
        }

        // Merge cascade versions into updated_versions for dep reference updating
        let mut all_versions = updated_versions.clone();
        all_versions.extend(cascade_needed.iter().map(|(k, v)| (k.clone(), v.clone())));

        // Update existing releases' Cargo.toml dep references
        let mut updated_releases: Vec<ComponentRelease> = releases
            .into_iter()
            .map(|mut release| {
                update_cargo_toml_deps_in_release(&mut release, repo_path, &all_versions);
                release
            })
            .collect();

        // Create new releases for cascade-bumped crates
        for crate_info in &crates {
            if let Some(new_version) = cascade_needed.get(&crate_info.name) {
                if let Some(release) = create_cascade_release(
                    crate_info,
                    new_version,
                    repo_path,
                    &all_versions,
                    manifest_config,
                    manifest_versions,
                )? {
                    updated_releases.push(release);
                }
            }
        }

        Ok(updated_releases)
    }
}

/// Parse the Cargo workspace structure from the repo root.
fn parse_workspace(repo_path: &Path) -> Result<Vec<CrateInfo>> {
    let root_toml_path = repo_path.join("Cargo.toml");
    if !root_toml_path.exists() {
        return Ok(Vec::new());
    }

    let root_content = std::fs::read_to_string(&root_toml_path)?;
    let root: toml::Value = toml::from_str(&root_content).map_err(|e| {
        crate::error::Error::Config(format!("failed to parse root Cargo.toml: {e}"))
    })?;

    let members = match root
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        Some(m) => m,
        None => return Ok(Vec::new()), // Not a workspace
    };

    let mut crates = Vec::new();

    // Resolve member patterns (simple glob support)
    for member_val in members {
        let pattern = match member_val.as_str() {
            Some(s) => s,
            None => continue,
        };

        let resolved = resolve_workspace_members(repo_path, pattern);
        for member_path in resolved {
            if let Some(info) = parse_member_crate(repo_path, &member_path)? {
                crates.push(info);
            }
        }
    }

    // Also parse root package if it has [package]
    if root.get("package").is_some() {
        if let Some(info) = parse_member_crate(repo_path, ".")? {
            // Only add if not already in the list
            if !crates.iter().any(|c| c.name == info.name) {
                crates.push(info);
            }
        }
    }

    // Now resolve workspace_deps to only include other workspace members
    let all_names: HashSet<String> = crates.iter().map(|c| c.name.clone()).collect();
    for c in &mut crates {
        c.workspace_deps.retain(|d| all_names.contains(d));
    }

    Ok(crates)
}

/// Resolve a workspace member glob pattern to actual directories.
fn resolve_workspace_members(repo_path: &Path, pattern: &str) -> Vec<String> {
    if pattern.contains('*') {
        // Use glob to resolve
        let full_pattern = repo_path.join(pattern).join("Cargo.toml");
        let pattern_str = full_pattern.to_string_lossy().to_string();
        glob::glob(&pattern_str)
            .ok()
            .map(|paths| {
                paths
                    .filter_map(|p| p.ok())
                    .filter_map(|p| {
                        p.parent().and_then(|dir| {
                            dir.strip_prefix(repo_path)
                                .ok()
                                .map(|rel| rel.to_string_lossy().to_string())
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![pattern.to_string()]
    }
}

/// Parse a single workspace member's Cargo.toml.
fn parse_member_crate(repo_path: &Path, member_path: &str) -> Result<Option<CrateInfo>> {
    let toml_path = if member_path == "." {
        repo_path.join("Cargo.toml")
    } else {
        repo_path.join(member_path).join("Cargo.toml")
    };

    if !toml_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&toml_path)?;
    let parsed: toml::Value = toml::from_str(&content).map_err(|e| {
        crate::error::Error::Config(format!("failed to parse {}: {}", toml_path.display(), e))
    })?;

    let name = parsed
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let version = parsed
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .and_then(|s| Version::parse(s).ok());

    let (name, version) = match (name, version) {
        (Some(n), Some(v)) => (n, v),
        _ => return Ok(None),
    };

    // Collect all dependency names
    let mut all_deps = Vec::new();
    collect_dep_names(&parsed, "dependencies", &mut all_deps);
    collect_dep_names(&parsed, "dev-dependencies", &mut all_deps);
    collect_dep_names(&parsed, "build-dependencies", &mut all_deps);

    // Also check target-specific deps
    if let Some(targets) = parsed.get("target").and_then(|t| t.as_table()) {
        for (_target, target_val) in targets {
            collect_dep_names(target_val, "dependencies", &mut all_deps);
            collect_dep_names(target_val, "dev-dependencies", &mut all_deps);
            collect_dep_names(target_val, "build-dependencies", &mut all_deps);
        }
    }

    Ok(Some(CrateInfo {
        name,
        path: member_path.to_string(),
        version,
        workspace_deps: all_deps,
    }))
}

/// Collect dependency names from a TOML section.
fn collect_dep_names(parent: &toml::Value, section: &str, out: &mut Vec<String>) {
    if let Some(deps) = parent.get(section).and_then(|d| d.as_table()) {
        for (name, dep_val) in deps {
            // Only include deps that have a `path` field (workspace deps)
            let has_path = dep_val.as_table().and_then(|t| t.get("path")).is_some();
            if has_path {
                out.push(name.clone());
            }
        }
    }
}

/// Topological sort of crates (dependencies first).
fn topological_sort(crates: &[CrateInfo]) -> Vec<&CrateInfo> {
    let by_name: HashMap<&str, &CrateInfo> = crates.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut visited = HashSet::new();
    let mut order = Vec::new();

    for c in crates {
        visit_dfs(c, &by_name, &mut visited, &mut order, &mut Vec::new());
    }

    order
}

fn visit_dfs<'a>(
    node: &'a CrateInfo,
    by_name: &HashMap<&str, &'a CrateInfo>,
    visited: &mut HashSet<String>,
    order: &mut Vec<&'a CrateInfo>,
    path: &mut Vec<String>,
) {
    if visited.contains(&node.name) {
        return;
    }
    if path.contains(&node.name) {
        return; // cycle — skip
    }

    path.push(node.name.clone());

    for dep in &node.workspace_deps {
        if let Some(dep_crate) = by_name.get(dep.as_str()) {
            visit_dfs(dep_crate, by_name, visited, order, path);
        }
    }

    path.pop();
    visited.insert(node.name.clone());
    order.push(node);
}

/// Update Cargo.toml dependency version references within an existing release.
fn update_cargo_toml_deps_in_release(
    release: &mut ComponentRelease,
    _repo_path: &Path,
    all_versions: &HashMap<String, Version>,
) {
    for update in &mut release.file_updates {
        if update.path.ends_with("Cargo.toml") {
            update.content = update_cargo_deps_in_toml(&update.content, all_versions);
        }
    }
}

/// Update dependency version strings in a Cargo.toml content string.
fn update_cargo_deps_in_toml(content: &str, versions: &HashMap<String, Version>) -> String {
    let mut result = content.to_string();

    // Parse the TOML to find dependency sections
    let parsed: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(_) => return result,
    };

    for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(deps) = parsed.get(section).and_then(|d| d.as_table()) {
            for (dep_name, dep_val) in deps {
                if let Some(new_ver) = versions.get(dep_name) {
                    // Only update if it has a path (workspace dep) and a version field
                    if let Some(table) = dep_val.as_table() {
                        if table.contains_key("path") && table.contains_key("version") {
                            result = update_dep_version_in_section(
                                &result,
                                section,
                                dep_name,
                                &new_ver.to_string(),
                            );
                        }
                    }
                }
            }
        }
    }

    result
}

/// Replace a specific dependency's version in a TOML section using regex.
fn update_dep_version_in_section(
    content: &str,
    _section: &str,
    dep_name: &str,
    new_version: &str,
) -> String {
    // Match: dep_name = { ... version = "x.y.z" ... }
    // We use a targeted regex for the version within the dep's inline table
    let pattern = format!(
        r#"(?m)({}(?:\s*=\s*\{{[^}}]*?version\s*=\s*"))[^"]*(")"#,
        regex::escape(dep_name)
    );

    if let Ok(re) = regex::Regex::new(&pattern) {
        re.replace_all(content, |caps: &regex::Captures| {
            format!("{}{}{}", &caps[1], new_version, &caps[2])
        })
        .to_string()
    } else {
        content.to_string()
    }
}

/// Create a cascade release for a crate that wasn't directly bumped.
fn create_cascade_release(
    crate_info: &CrateInfo,
    new_version: &Version,
    repo_path: &Path,
    all_versions: &HashMap<String, Version>,
    manifest_config: &ManifestConfig,
    _manifest_versions: &HashMap<String, String>,
) -> Result<Option<ComponentRelease>> {
    let pkg_config = manifest_config
        .packages
        .get(&crate_info.path)
        .cloned()
        .unwrap_or_default();
    let resolved = config::resolve_config(&manifest_config.defaults, &pkg_config);

    // Build Cargo.toml update with new version + dep updates
    let cargo_path = if crate_info.path == "." {
        "Cargo.toml".to_string()
    } else {
        format!("{}/Cargo.toml", crate_info.path)
    };

    let cargo_full = repo_path.join(&cargo_path);
    if !cargo_full.exists() {
        return Ok(None);
    }

    let cargo_content = std::fs::read_to_string(&cargo_full)?;
    let updated_cargo =
        updater::update_cargo_toml_version(&cargo_content, &new_version.to_string());
    let updated_cargo = update_cargo_deps_in_toml(&updated_cargo, all_versions);

    // Generate a simple changelog entry for the dependency update
    let changelog_entry = format!(
        "## {} ({})\n\n### Dependencies\n\n* Updated workspace dependencies\n",
        new_version,
        chrono::Utc::now().format("%Y-%m-%d"),
    );

    let mut file_updates = vec![FileUpdate {
        path: cargo_path,
        content: updated_cargo,
        create_if_missing: false,
    }];

    // Update changelog if not skipped
    if !resolved.skip_changelog {
        let cl_path = if crate_info.path == "." {
            resolved.changelog_path.clone()
        } else {
            format!("{}/{}", crate_info.path, resolved.changelog_path)
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
        Some(crate_info.name.clone()),
        resolved.include_component_in_tag,
        &resolved.tag_separator,
        resolved.include_v_in_tag,
    );

    Ok(Some(ComponentRelease {
        component: Some(crate_info.name.clone()),
        package_path: crate_info.path.clone(),
        current_version: Some(crate_info.version.clone()),
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
    use crate::testutil::TestRepo;

    #[test]
    fn test_parse_workspace() {
        let repo = TestRepo::new();
        repo.write_file(
            "Cargo.toml",
            r#"[workspace]
members = ["crates/a", "crates/b"]
"#,
        );
        repo.write_file(
            "crates/a/Cargo.toml",
            r#"[package]
name = "crate-a"
version = "1.0.0"

[dependencies]
crate-b = { version = "1.0.0", path = "../b" }
"#,
        );
        repo.write_file(
            "crates/b/Cargo.toml",
            r#"[package]
name = "crate-b"
version = "1.0.0"
"#,
        );
        repo.add_and_commit("init");

        let crates = parse_workspace(repo.path()).unwrap();
        assert_eq!(crates.len(), 2);

        let a = crates.iter().find(|c| c.name == "crate-a").unwrap();
        assert_eq!(a.workspace_deps, vec!["crate-b"]);

        let b = crates.iter().find(|c| c.name == "crate-b").unwrap();
        assert!(b.workspace_deps.is_empty());
    }

    #[test]
    fn test_topological_sort() {
        let crates = vec![
            CrateInfo {
                name: "a".into(),
                path: "crates/a".into(),
                version: Version::new(1, 0, 0),
                workspace_deps: vec!["b".into()],
            },
            CrateInfo {
                name: "b".into(),
                path: "crates/b".into(),
                version: Version::new(1, 0, 0),
                workspace_deps: vec![],
            },
        ];

        let sorted = topological_sort(&crates);
        let names: Vec<&str> = sorted.iter().map(|c| c.name.as_str()).collect();
        // b should come before a (dependency first)
        let b_idx = names.iter().position(|n| *n == "b").unwrap();
        let a_idx = names.iter().position(|n| *n == "a").unwrap();
        assert!(b_idx < a_idx);
    }

    #[test]
    fn test_cascade_bump() {
        let repo = TestRepo::new();
        repo.write_file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n",
        );
        repo.write_file(
            "crates/a/Cargo.toml",
            "[package]\nname = \"crate-a\"\nversion = \"1.0.0\"\n\n[dependencies]\ncrate-b = { version = \"1.0.0\", path = \"../b\" }\n",
        );
        repo.write_file(
            "crates/b/Cargo.toml",
            "[package]\nname = \"crate-b\"\nversion = \"1.0.0\"\n",
        );
        repo.write_config(&serde_json::json!({
            "release-type": "rust",
            "plugins": ["cargo-workspace"],
            "packages": {
                "crates/a": { "component": "crate-a", "package-name": "crate-a" },
                "crates/b": { "component": "crate-b", "package-name": "crate-b" }
            }
        }));
        repo.write_manifest(&serde_json::json!({
            "crates/a": "1.0.0",
            "crates/b": "1.0.0"
        }));
        repo.add_and_commit("chore: initial");
        repo.create_tag("crate-a-v1.0.0");
        repo.create_tag("crate-b-v1.0.0");

        // Only bump crate-b
        repo.write_file("crates/b/src/lib.rs", "// new");
        repo.add_and_commit("feat: b feature");

        let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
        let manifest =
            config::load_manifest(&repo.path().join(".release-please-manifest.json")).unwrap();

        let output =
            crate::manifest::process_repo_with_config(repo.path(), &config, &manifest).unwrap();

        // crate-b should have minor bump (feat)
        let b_rel = output
            .releases
            .iter()
            .find(|r| r.component.as_deref() == Some("crate-b"))
            .unwrap();
        assert_eq!(b_rel.new_version, Version::new(1, 1, 0));

        // crate-a should have cascade patch bump (depends on crate-b)
        let a_rel = output
            .releases
            .iter()
            .find(|r| r.component.as_deref() == Some("crate-a"))
            .unwrap();
        assert_eq!(a_rel.new_version, Version::new(1, 0, 1));

        // crate-a's Cargo.toml should have updated dep version
        let a_cargo = a_rel
            .file_updates
            .iter()
            .find(|u| u.path.contains("Cargo.toml"))
            .unwrap();
        assert!(
            a_cargo.content.contains("version = \"1.0.1\"")
                || a_cargo.content.contains("version = \"1.1.0\"")
        );
    }

    #[test]
    fn test_no_cascade_when_no_deps() {
        let crates = vec![
            CrateInfo {
                name: "a".into(),
                path: "crates/a".into(),
                version: Version::new(1, 0, 0),
                workspace_deps: vec![],
            },
            CrateInfo {
                name: "b".into(),
                path: "crates/b".into(),
                version: Version::new(1, 0, 0),
                workspace_deps: vec![],
            },
        ];

        // No deps between a and b, so cascade_needed should be empty
        let sorted = topological_sort(&crates);
        assert_eq!(sorted.len(), 2);
    }
}
