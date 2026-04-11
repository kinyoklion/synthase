//! Integration tests: build real git repos and exercise the full release pipeline.
//!
//! Each test creates a temporary git repository with specific commits, tags,
//! and config files, then runs `process_repo_with_config()` and asserts on
//! the output.

use rustlease_please::config;
use rustlease_please::manifest::process_repo_with_config;
use rustlease_please::testutil::TestRepo;
use semver::Version;
use std::collections::HashMap;

// ===========================================================================
// Helpers
// ===========================================================================

fn load_config_and_manifest(
    repo: &TestRepo,
) -> (config::ManifestConfig, HashMap<String, String>) {
    let config =
        config::load_config(&repo.path().join("release-please-config.json")).unwrap();
    let manifest_path = repo.path().join(".release-please-manifest.json");
    let manifest = if manifest_path.exists() {
        config::load_manifest(&manifest_path).unwrap()
    } else {
        HashMap::new()
    };
    (config, manifest)
}

// ===========================================================================
// Single-package repos
// ===========================================================================

#[test]
fn test_rust_single_crate() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "rust",
        "packages": { ".": { "component": "my-crate", "package-name": "my-crate" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.write_file("Cargo.toml", "[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\nedition = \"2021\"\n");
    repo.write_file("Cargo.lock", "[[package]]\nname = \"my-crate\"\nversion = \"1.0.0\"\n");
    repo.add_and_commit("chore: initial");
    repo.create_tag("my-crate-v1.0.0");

    repo.write_file("src/lib.rs", "pub fn new_feature() {}");
    repo.add_and_commit("feat: add new feature");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 1);
    let r = &output.releases[0];
    assert_eq!(r.new_version, Version::new(1, 1, 0));
    assert_eq!(r.tag, "my-crate-v1.1.0");

    // Cargo.toml should be updated
    let cargo_update = r.file_updates.iter().find(|u| u.path == "Cargo.toml").unwrap();
    assert!(cargo_update.content.contains("version = \"1.1.0\""));
    assert!(cargo_update.content.contains("name = \"my-crate\""));

    // Cargo.lock should be updated
    let lock_update = r.file_updates.iter().find(|u| u.path == "Cargo.lock").unwrap();
    assert!(lock_update.content.contains("version = \"1.1.0\""));

    // CHANGELOG should exist
    assert!(r.file_updates.iter().any(|u| u.path == "CHANGELOG.md"));
    assert!(r.changelog_entry.contains("### Features"));
}

#[test]
fn test_node_single_package() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "node",
        "packages": { ".": {} }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.write_file("package.json", "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\"\n}\n");
    repo.write_file("package-lock.json", "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\",\n  \"lockfileVersion\": 3,\n  \"packages\": {\n    \"\": {\n      \"version\": \"1.0.0\"\n    }\n  }\n}\n");
    repo.add_and_commit("chore: initial");
    repo.create_tag("v1.0.0");

    repo.write_file("index.js", "// fix");
    repo.add_and_commit("fix: resolve crash");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 1);
    let r = &output.releases[0];
    assert_eq!(r.new_version, Version::new(1, 0, 1));

    let pkg_update = r.file_updates.iter().find(|u| u.path == "package.json").unwrap();
    assert!(pkg_update.content.contains("\"version\": \"1.0.1\""));

    let lock_update = r.file_updates.iter().find(|u| u.path == "package-lock.json").unwrap();
    assert!(lock_update.content.contains("\"version\": \"1.0.1\""));
}

#[test]
fn test_simple_strategy() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "my-tool" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.write_file("version.txt", "1.0.0\n");
    repo.add_and_commit("chore: initial");
    repo.create_tag("my-tool-v1.0.0");

    repo.write_file("src/main.sh", "#!/bin/bash\necho hello");
    repo.add_and_commit("feat: add feature");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 1);
    let r = &output.releases[0];
    assert_eq!(r.new_version, Version::new(1, 1, 0));

    let version_update = r.file_updates.iter().find(|u| u.path == "version.txt").unwrap();
    assert_eq!(version_update.content, "1.1.0\n");

    assert!(r.file_updates.iter().any(|u| u.path == "CHANGELOG.md"));
}

#[test]
fn test_initial_release_no_prior_tags() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "initial-version": "0.1.0",
        "packages": { ".": { "component": "new-pkg" } }
    }));
    // No manifest — first release
    repo.write_file("README.md", "# New Package");
    repo.add_and_commit("feat: initial feature");

    let config = config::load_config(&repo.path().join("release-please-config.json")).unwrap();
    let manifest = HashMap::new();
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 1);
    let r = &output.releases[0];
    assert_eq!(r.new_version, Version::parse("0.1.0").unwrap());
    assert!(r.current_version.is_none());
}

// ===========================================================================
// Version bumping
// ===========================================================================

#[test]
fn test_patch_bump_from_fix() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    repo.write_file("a.txt", "fix");
    repo.add_and_commit("fix: resolve null pointer");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases[0].new_version, Version::new(1, 0, 1));
}

#[test]
fn test_minor_bump_from_feat() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    repo.write_file("a.txt", "feat");
    repo.add_and_commit("feat: add search functionality");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases[0].new_version, Version::new(1, 1, 0));
    assert!(output.releases[0].changelog_entry.contains("### Features"));
}

#[test]
fn test_major_bump_from_breaking() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    repo.write_file("a.txt", "breaking");
    repo.add_and_commit("feat!: redesign API");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases[0].new_version, Version::new(2, 0, 0));
    assert!(output.releases[0].changelog_entry.contains("BREAKING CHANGES"));
}

#[test]
fn test_multiple_commits_highest_bump_wins() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    repo.write_file("a.txt", "1");
    repo.add_and_commit("fix: typo");
    repo.write_file("b.txt", "2");
    repo.add_and_commit("feat: new thing");
    repo.write_file("c.txt", "3");
    repo.add_and_commit("fix: another bug");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    // feat wins → minor bump
    assert_eq!(output.releases[0].new_version, Version::new(1, 1, 0));
}

#[test]
fn test_no_releasable_commits() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    repo.write_file("README.md", "updated");
    repo.add_and_commit("chore: update CI");
    repo.write_file("docs.md", "new docs");
    repo.add_and_commit("docs: update readme");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert!(output.releases.is_empty());
    assert!(output.manifest_update.is_none());
}

#[test]
fn test_pre_major_bump_minor_pre_major() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "bump-minor-pre-major": true,
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "0.5.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v0.5.0");

    repo.write_file("a.txt", "breaking");
    repo.add_and_commit("feat!: breaking change");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    // With bump-minor-pre-major, breaking at 0.x → minor (not major)
    assert_eq!(output.releases[0].new_version, Version::new(0, 6, 0));
}

#[test]
fn test_release_as_override() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    repo.write_file("a.txt", "override");
    repo.add_and_commit("fix: something\n\nRelease-As: 3.0.0");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases[0].new_version, Version::new(3, 0, 0));
}

// ===========================================================================
// Monorepo
// ===========================================================================

#[test]
fn test_monorepo_two_packages_independent() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "packages": {
            "packages/a": { "release-type": "simple", "component": "a" },
            "packages/b": { "release-type": "simple", "component": "b" }
        }
    }));
    repo.write_manifest(&serde_json::json!({
        "packages/a": "1.0.0",
        "packages/b": "2.0.0"
    }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("a-v1.0.0");
    repo.create_tag("b-v2.0.0");

    repo.write_file("packages/a/lib.rs", "// new");
    repo.add_and_commit("feat: a feature");

    repo.write_file("packages/b/lib.rs", "// fix");
    repo.add_and_commit("fix: b fix");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 2);

    let a = output.releases.iter().find(|r| r.component.as_deref() == Some("a")).unwrap();
    assert_eq!(a.new_version, Version::new(1, 1, 0));

    let b = output.releases.iter().find(|r| r.component.as_deref() == Some("b")).unwrap();
    assert_eq!(b.new_version, Version::new(2, 0, 1));

    // Manifest should have both new versions
    let mu = output.manifest_update.unwrap();
    assert!(mu.content.contains("\"1.1.0\""));
    assert!(mu.content.contains("\"2.0.1\""));
}

#[test]
fn test_monorepo_only_one_changed() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "packages": {
            "packages/a": { "release-type": "simple", "component": "a" },
            "packages/b": { "release-type": "simple", "component": "b" }
        }
    }));
    repo.write_manifest(&serde_json::json!({
        "packages/a": "1.0.0",
        "packages/b": "2.0.0"
    }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("a-v1.0.0");
    repo.create_tag("b-v2.0.0");

    // Only change package a
    repo.write_file("packages/a/lib.rs", "// new");
    repo.add_and_commit("feat: a only");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 1);
    assert_eq!(output.releases[0].component.as_deref(), Some("a"));
    assert_eq!(output.releases[0].new_version, Version::new(1, 1, 0));
}

#[test]
fn test_monorepo_mixed_release_types() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "packages": {
            "packages/rust-lib": {
                "release-type": "rust",
                "component": "rust-lib",
                "package-name": "rust-lib"
            },
            "packages/node-app": {
                "release-type": "node",
                "component": "node-app"
            }
        }
    }));
    repo.write_manifest(&serde_json::json!({
        "packages/rust-lib": "1.0.0",
        "packages/node-app": "1.0.0"
    }));
    repo.write_file("packages/rust-lib/Cargo.toml", "[package]\nname = \"rust-lib\"\nversion = \"1.0.0\"\n");
    repo.write_file("packages/node-app/package.json", "{\n  \"name\": \"node-app\",\n  \"version\": \"1.0.0\"\n}\n");
    repo.add_and_commit("chore: initial");
    repo.create_tag("rust-lib-v1.0.0");
    repo.create_tag("node-app-v1.0.0");

    repo.write_file("packages/rust-lib/src/lib.rs", "// feat");
    repo.add_and_commit("feat: rust feature");
    repo.write_file("packages/node-app/index.js", "// fix");
    repo.add_and_commit("fix: node fix");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 2);

    let rust = output.releases.iter().find(|r| r.component.as_deref() == Some("rust-lib")).unwrap();
    assert_eq!(rust.new_version, Version::new(1, 1, 0));
    assert!(rust.file_updates.iter().any(|u| u.path == "packages/rust-lib/Cargo.toml"));

    let node = output.releases.iter().find(|r| r.component.as_deref() == Some("node-app")).unwrap();
    assert_eq!(node.new_version, Version::new(1, 0, 1));
    assert!(node.file_updates.iter().any(|u| u.path == "packages/node-app/package.json"));
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn test_tag_but_no_new_commits() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    // No new commits after tag
    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert!(output.releases.is_empty());
}

#[test]
fn test_extra_files_annotation_marker() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "extra-files": ["version.h"],
        "packages": { ".": { "component": "pkg" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.write_file("version.h", "#define VERSION \"1.0.0\" // x-release-please-version\n");
    repo.add_and_commit("chore: initial");
    repo.create_tag("pkg-v1.0.0");

    repo.write_file("src/main.c", "// new");
    repo.add_and_commit("feat: add feature");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    assert_eq!(output.releases.len(), 1);
    let r = &output.releases[0];
    assert_eq!(r.new_version, Version::new(1, 1, 0));

    let header_update = r.file_updates.iter().find(|u| u.path == "version.h").unwrap();
    assert!(header_update.content.contains("\"1.1.0\""));
    assert!(header_update.content.contains("x-release-please-version"));
}

// ===========================================================================
// PR formatting (end-to-end)
// ===========================================================================

#[test]
fn test_pr_title_single_package() {
    let repo = TestRepo::new();
    repo.write_config(&serde_json::json!({
        "release-type": "simple",
        "packages": { ".": { "component": "my-tool" } }
    }));
    repo.write_manifest(&serde_json::json!({ ".": "1.0.0" }));
    repo.add_and_commit("chore: initial");
    repo.create_tag("my-tool-v1.0.0");

    repo.write_file("a.txt", "feat");
    repo.add_and_commit("feat: something");

    let (config, manifest) = load_config_and_manifest(&repo);
    let output = process_repo_with_config(repo.path(), &config, &manifest).unwrap();

    let title = rustlease_please::manifest::format_pr_title(&output.releases, &config, "main");
    assert_eq!(title, "chore(main): release my-tool 1.1.0");

    let body = rustlease_please::manifest::format_pr_body(&output.releases, &config);
    assert!(body.contains(":robot:"));
    assert!(body.contains("### Features"));
    assert!(!body.contains("<details>")); // single component, no collapsible
}
