//! Sentence-case plugin: normalizes commit subjects to sentence case in changelogs.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::ManifestConfig;
use crate::error::Result;
use crate::manifest::ComponentRelease;

use super::Plugin;

pub struct SentenceCasePlugin {
    pub special_words: HashSet<String>,
}

impl SentenceCasePlugin {
    pub fn from_config(config: &serde_json::Value) -> Self {
        let special_words = config
            .get("specialWords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        SentenceCasePlugin { special_words }
    }
}

impl Plugin for SentenceCasePlugin {
    fn run(
        &self,
        _repo_path: &Path,
        releases: Vec<ComponentRelease>,
        _manifest_config: &ManifestConfig,
        _manifest_versions: &HashMap<String, String>,
    ) -> Result<Vec<ComponentRelease>> {
        let updated = releases
            .into_iter()
            .map(|mut release| {
                release.changelog_entry =
                    sentence_case_changelog(&release.changelog_entry, &self.special_words);
                release
            })
            .collect();
        Ok(updated)
    }
}

/// Apply sentence casing to commit lines in a changelog entry.
///
/// Converts the first letter of each bullet point to uppercase,
/// while preserving special words.
fn sentence_case_changelog(changelog: &str, special_words: &HashSet<String>) -> String {
    let mut result = String::with_capacity(changelog.len());

    for line in changelog.lines() {
        if line.starts_with("* ") {
            let content = &line[2..];
            // Skip bold scope prefix if present: "**scope:** "
            if let Some(rest) = content.strip_prefix("**") {
                if let Some(after_scope) = rest.find(":** ") {
                    let scope_end = after_scope + 4; // length of ":** "
                    let scope_part = &content[..scope_end + 2]; // include "**"
                    let text = &content[scope_end + 2..];
                    result.push_str("* ");
                    result.push_str(scope_part);
                    result.push_str(&to_sentence_case(text, special_words));
                } else {
                    result.push_str("* ");
                    result.push_str(&to_sentence_case(content, special_words));
                }
            } else {
                result.push_str("* ");
                result.push_str(&to_sentence_case(content, special_words));
            }
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    // Match original trailing newline behavior
    if !changelog.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Convert text to sentence case, preserving special words.
fn to_sentence_case(text: &str, special_words: &HashSet<String>) -> String {
    if text.is_empty() {
        return String::new();
    }

    // Check if the first word is a special word
    let first_word = text.split_whitespace().next().unwrap_or("");
    if special_words.contains(first_word) {
        return text.to_string();
    }

    let mut chars = text.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentence_case_basic() {
        let input = "## 1.0.0\n\n### Features\n\n* add new feature\n* fix something\n";
        let result = sentence_case_changelog(input, &HashSet::new());
        assert!(result.contains("* Add new feature"));
        assert!(result.contains("* Fix something"));
    }

    #[test]
    fn test_sentence_case_with_scope() {
        let input = "### Features\n\n* **auth:** add OAuth support\n";
        let result = sentence_case_changelog(input, &HashSet::new());
        assert!(result.contains("* **auth:** Add OAuth support"));
    }

    #[test]
    fn test_sentence_case_preserves_special_words() {
        let mut special = HashSet::new();
        special.insert("gRPC".to_string());

        let input = "### Features\n\n* gRPC support added\n";
        let result = sentence_case_changelog(input, &special);
        assert!(result.contains("* gRPC support added")); // preserved
    }

    #[test]
    fn test_sentence_case_already_uppercase() {
        let input = "### Features\n\n* Already uppercase\n";
        let result = sentence_case_changelog(input, &HashSet::new());
        assert!(result.contains("* Already uppercase"));
    }

    #[test]
    fn test_sentence_case_preserves_headers() {
        let input = "## 1.0.0 (2024-01-01)\n\n### Features\n\n* add thing\n";
        let result = sentence_case_changelog(input, &HashSet::new());
        assert!(result.contains("## 1.0.0 (2024-01-01)")); // header unchanged
        assert!(result.contains("### Features")); // section header unchanged
    }
}
