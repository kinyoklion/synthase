//! Shared file updater utilities used by release strategies.

use regex::Regex;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Cargo.toml version updater
// ---------------------------------------------------------------------------

/// Regex matching `version = "..."` in the `[package]` section of Cargo.toml.
static CARGO_TOML_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Match: version = "x.y.z" (with optional whitespace variations)
    // Only match in [package] section — we search for it after finding [package]
    Regex::new(r#"(?m)^(\s*version\s*=\s*")([^"]+)(")"#).unwrap()
});

/// Update the `[package] version` field in a Cargo.toml string.
///
/// Preserves all formatting, comments, and other content. Only replaces the
/// first `version = "..."` occurrence after a `[package]` header.
pub fn update_cargo_toml_version(content: &str, new_version: &str) -> String {
    // Find the [package] section
    let package_pos = match content.find("[package]") {
        Some(pos) => pos,
        None => return content.to_string(),
    };

    // Find the next section header after [package] to bound our search
    let search_end = content[package_pos + 9..]
        .find("\n[")
        .map(|p| package_pos + 9 + p)
        .unwrap_or(content.len());

    let section = &content[package_pos..search_end];

    if let Some(m) = CARGO_TOML_VERSION_RE.find(section) {
        let abs_start = package_pos + m.start();
        let abs_end = package_pos + m.end();
        let matched = &content[abs_start..abs_end];

        // Replace just the version part within the matched string
        let replaced = CARGO_TOML_VERSION_RE
            .replace(matched, |caps: &regex::Captures| {
                format!("{}{}{}", &caps[1], new_version, &caps[3])
            })
            .to_string();

        format!(
            "{}{}{}",
            &content[..abs_start],
            replaced,
            &content[abs_end..]
        )
    } else {
        content.to_string()
    }
}

/// Update the version constraint for a named dependency in a Cargo.toml string.
///
/// Handles both inline-table form (`pkg = { version = "x", ... }`) and bare
/// string form (`pkg = "x"`). Only replaces the first match per package name.
pub fn update_cargo_toml_dep_version(content: &str, dep_name: &str, new_version: &str) -> String {
    // Inline table form: dep-name = { ..., version = "x.y.z", ... }
    // Use a two-step approach: find the dep line, then replace the version inside it.
    let mut result = String::with_capacity(content.len());

    for line in content.lines() {
        let trimmed = line.trim_start();

        // Match `dep-name = ...` (with optional whitespace around `=`)
        let after_name = trimmed
            .strip_prefix(dep_name)
            .and_then(|rest| rest.trim_start().strip_prefix('='));

        if let Some(after_eq) = after_name {
            let after_eq = after_eq.trim_start();

            if after_eq.starts_with('{') {
                // Inline table: replace `version = "..."` inside the braces
                let dep_version_re = Regex::new(r#"(version\s*=\s*")([^"]+)(")"#).unwrap();
                let replaced = dep_version_re
                    .replace(after_eq, |caps: &regex::Captures| {
                        format!("{}{}{}", &caps[1], new_version, &caps[3])
                    })
                    .to_string();
                let indent = &line[..line.len() - trimmed.len()];
                result.push_str(&format!("{}{} = {}\n", indent, dep_name, replaced));
                continue;
            } else if after_eq.starts_with('"') {
                // Bare string form: dep-name = "x.y.z"
                let indent = &line[..line.len() - trimmed.len()];
                result.push_str(&format!("{}{} = \"{}\"\n", indent, dep_name, new_version));
                continue;
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    // Preserve lack of trailing newline
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

// ---------------------------------------------------------------------------
// Cargo.lock version updater
// ---------------------------------------------------------------------------

/// Update a package's version in Cargo.lock content.
///
/// Finds the `[[package]]` entry with the given name and replaces its version.
pub fn update_cargo_lock_version(content: &str, package_name: &str, new_version: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let lines = content.lines().peekable();
    let mut in_target_package = false;
    let mut version_replaced = false;

    for line in lines {
        if line.trim() == "[[package]]" {
            in_target_package = false;
            version_replaced = false;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if !version_replaced && in_target_package {
            if let Some(rest) = line.trim().strip_prefix("version = \"") {
                if let Some(ver) = rest.strip_suffix('"') {
                    let _ = ver; // consume
                    let indent = &line[..line.len() - line.trim_start().len()];
                    result.push_str(&format!("{indent}version = \"{new_version}\"\n"));
                    version_replaced = true;
                    continue;
                }
            }
        }

        // Check if this line is `name = "package_name"`
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name = \"") {
            if let Some(name) = rest.strip_suffix('"') {
                if name == package_name {
                    in_target_package = true;
                }
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    // Handle files that don't end with newline
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

// ---------------------------------------------------------------------------
// JSON version updater (package.json / package-lock.json)
// ---------------------------------------------------------------------------

/// Detect the indentation used in a JSON string.
fn detect_json_indent(content: &str) -> String {
    for line in content.lines().skip(1) {
        if line.starts_with("  ") {
            // Count leading spaces
            let spaces = line.len() - line.trim_start().len();
            return " ".repeat(spaces);
        } else if line.starts_with('\t') {
            return "\t".to_string();
        }
    }
    "  ".to_string() // default: 2 spaces
}

/// Update the `"version"` field in a package.json string.
///
/// Preserves indentation and trailing newline.
pub fn update_package_json_version(content: &str, new_version: &str) -> String {
    let indent = detect_json_indent(content);
    let mut parsed: serde_json::Value = serde_json::from_str(content).unwrap_or_default();

    if let Some(obj) = parsed.as_object_mut() {
        obj.insert(
            "version".to_string(),
            serde_json::Value::String(new_version.to_string()),
        );
    }

    let mut result = serde_json::to_string_pretty(&parsed).unwrap_or_default();

    // Re-indent if needed (serde_json always uses 2-space indent)
    if indent != "  " {
        result = re_indent_json(&result, &indent);
    }

    // Preserve trailing newline
    if content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Update version fields in a package-lock.json string.
///
/// Updates the root `version` and `packages[""].version` fields.
pub fn update_package_lock_json_version(content: &str, new_version: &str) -> String {
    let indent = detect_json_indent(content);
    let mut parsed: serde_json::Value = serde_json::from_str(content).unwrap_or_default();

    if let Some(obj) = parsed.as_object_mut() {
        // Root version
        obj.insert(
            "version".to_string(),
            serde_json::Value::String(new_version.to_string()),
        );

        // packages[""].version (lockfile v2/v3)
        if let Some(packages) = obj.get_mut("packages") {
            if let Some(packages_obj) = packages.as_object_mut() {
                if let Some(root_pkg) = packages_obj.get_mut("") {
                    if let Some(root_obj) = root_pkg.as_object_mut() {
                        root_obj.insert(
                            "version".to_string(),
                            serde_json::Value::String(new_version.to_string()),
                        );
                    }
                }
            }
        }
    }

    let mut result = serde_json::to_string_pretty(&parsed).unwrap_or_default();

    if indent != "  " {
        result = re_indent_json(&result, &indent);
    }

    if content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Re-indent a JSON string from 2-space (serde default) to a custom indent.
fn re_indent_json(json: &str, indent: &str) -> String {
    let mut result = String::with_capacity(json.len());
    for line in json.lines() {
        let stripped = line.trim_start();
        let leading_spaces = line.len() - stripped.len();
        let indent_level = leading_spaces / 2;
        for _ in 0..indent_level {
            result.push_str(indent);
        }
        result.push_str(stripped);
        result.push('\n');
    }
    // Remove trailing newline added by loop (we handle it separately)
    if result.ends_with('\n') && !json.ends_with('\n') {
        result.pop();
    }
    result
}

// ---------------------------------------------------------------------------
// Generic annotation-based updater
// ---------------------------------------------------------------------------

// Supports both x-synthase-* and x-release-please-* markers for compatibility
static INLINE_VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"x-(synthase|release-please)-version").unwrap());

static BLOCK_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"x-(synthase|release-please)-start-version").unwrap());

static BLOCK_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"x-(synthase|release-please)-end").unwrap());

static SEMVER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?(\+[a-zA-Z0-9.]+)?").unwrap());

/// Update version strings in a file using annotation markers.
///
/// Supports both `x-synthase-*` and `x-release-please-*` markers:
/// - Inline: `x-synthase-version` on the same line as a version string
/// - Block: `x-synthase-start-version` ... `x-synthase-end`
pub fn update_generic_version(content: &str, new_version: &str) -> String {
    let mut result = Vec::new();
    let mut in_block = false;

    for line in content.lines() {
        if BLOCK_END_RE.is_match(line) {
            in_block = false;
            result.push(line.to_string());
            continue;
        }

        if BLOCK_START_RE.is_match(line) {
            in_block = true;
            result.push(line.to_string());
            continue;
        }

        if in_block || INLINE_VERSION_RE.is_match(line) {
            // Replace semver patterns on this line
            let replaced = SEMVER_RE.replace_all(line, new_version).to_string();
            result.push(replaced);
        } else {
            result.push(line.to_string());
        }
    }

    let mut output = result.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Cargo.toml ===

    #[test]
    fn test_update_cargo_toml_version() {
        let content = r#"[package]
name = "my-crate"
version = "1.0.0"
edition = "2021"

[dependencies]
serde = "1"
"#;
        let updated = update_cargo_toml_version(content, "1.1.0");
        assert!(updated.contains(r#"version = "1.1.0""#));
        assert!(updated.contains(r#"name = "my-crate""#));
        assert!(updated.contains(r#"serde = "1""#));
    }

    #[test]
    fn test_update_cargo_toml_preserves_formatting() {
        let content = "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n\n[dependencies]\nbar = { version = \"2.0.0\", features = [\"a\"] }\n";
        let updated = update_cargo_toml_version(content, "0.2.0");
        assert!(updated.contains("version = \"0.2.0\""));
        // Don't accidentally update dependency versions
        assert!(updated.contains("bar = { version = \"2.0.0\""));
    }

    #[test]
    fn test_update_cargo_toml_no_package_section() {
        let content = "[dependencies]\nfoo = \"1.0\"\n";
        let updated = update_cargo_toml_version(content, "2.0.0");
        assert_eq!(updated, content); // unchanged
    }

    #[test]
    fn test_update_cargo_toml_with_workspace_section() {
        let content = r#"[package]
name = "my-crate"
version = "1.0.0"

[workspace]
members = ["crates/*"]
"#;
        let updated = update_cargo_toml_version(content, "2.0.0");
        assert!(updated.contains(r#"version = "2.0.0""#));
        assert!(updated.contains("[workspace]"));
    }

    // === Cargo.lock ===

    #[test]
    fn test_update_cargo_lock_version() {
        let content = r#"[[package]]
name = "my-crate"
version = "1.0.0"

[[package]]
name = "serde"
version = "1.0.100"
"#;
        let updated = update_cargo_lock_version(content, "my-crate", "1.1.0");
        assert!(updated.contains("name = \"my-crate\"\nversion = \"1.1.0\""));
        assert!(updated.contains("name = \"serde\"\nversion = \"1.0.100\""));
    }

    #[test]
    fn test_update_cargo_lock_no_match() {
        let content = "[[package]]\nname = \"other\"\nversion = \"1.0.0\"\n";
        let updated = update_cargo_lock_version(content, "my-crate", "2.0.0");
        assert!(updated.contains("version = \"1.0.0\"")); // unchanged
    }

    // === package.json ===

    #[test]
    fn test_update_package_json_version() {
        let content = "{\n  \"name\": \"my-pkg\",\n  \"version\": \"1.0.0\"\n}\n";
        let updated = update_package_json_version(content, "1.1.0");
        assert!(updated.contains("\"version\": \"1.1.0\""));
        assert!(updated.contains("\"name\": \"my-pkg\""));
        assert!(updated.ends_with('\n'));
    }

    #[test]
    fn test_update_package_json_preserves_indent() {
        let content = "{\n    \"name\": \"pkg\",\n    \"version\": \"1.0.0\"\n}\n";
        let updated = update_package_json_version(content, "2.0.0");
        assert!(updated.contains("\"version\": \"2.0.0\""));
        // Should use 4-space indent
        assert!(updated.contains("    \""));
    }

    // === package-lock.json ===

    #[test]
    fn test_update_package_lock_json() {
        let content = r#"{
  "name": "my-pkg",
  "version": "1.0.0",
  "lockfileVersion": 3,
  "packages": {
    "": {
      "name": "my-pkg",
      "version": "1.0.0"
    }
  }
}
"#;
        let updated = update_package_lock_json_version(content, "1.1.0");
        // Root version updated
        let parsed: serde_json::Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(parsed["version"], "1.1.0");
        assert_eq!(parsed["packages"][""]["version"], "1.1.0");
    }

    // === Generic annotation updater ===

    #[test]
    fn test_generic_inline_marker() {
        let content =
            "const VERSION = \"1.0.0\"; // x-release-please-version\nconst OTHER = \"hello\";\n";
        let updated = update_generic_version(content, "2.0.0");
        assert!(updated.contains("const VERSION = \"2.0.0\"; // x-release-please-version"));
        assert!(updated.contains("const OTHER = \"hello\";")); // unchanged
    }

    #[test]
    fn test_generic_block_marker() {
        let content = "# x-release-please-start-version\nversion = 1.0.0\n# x-release-please-end\nother = 3.0.0\n";
        let updated = update_generic_version(content, "2.0.0");
        assert!(updated.contains("version = 2.0.0"));
        assert!(updated.contains("other = 3.0.0")); // outside block, unchanged
    }

    #[test]
    fn test_generic_no_markers() {
        let content = "version = 1.0.0\n";
        let updated = update_generic_version(content, "2.0.0");
        assert_eq!(updated, content); // no markers, unchanged
    }

    #[test]
    fn test_generic_preserves_trailing_newline() {
        let content = "v = \"1.0.0\" // x-release-please-version\n";
        let updated = update_generic_version(content, "2.0.0");
        assert!(updated.ends_with('\n'));
    }

    #[test]
    fn test_generic_multiple_inline_markers() {
        let content = "a = \"1.0.0\" // x-release-please-version\nb = \"1.0.0\" // x-release-please-version\n";
        let updated = update_generic_version(content, "3.0.0");
        assert!(updated.contains("a = \"3.0.0\""));
        assert!(updated.contains("b = \"3.0.0\""));
    }
}
