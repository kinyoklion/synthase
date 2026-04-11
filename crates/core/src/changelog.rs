use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::commit::ConventionalCommit;
use crate::config::ChangelogSection;

/// Default changelog sections matching release-please behavior.
pub const DEFAULT_SECTIONS: &[(&str, &str, bool)] = &[
    ("feat", "Features", false),
    ("fix", "Bug Fixes", false),
    ("perf", "Performance Improvements", false),
    ("revert", "Reverts", false),
    ("deps", "Dependencies", false),
    ("docs", "Documentation", true),
    ("style", "Styles", true),
    ("chore", "Miscellaneous Chores", true),
    ("refactor", "Code Refactoring", true),
    ("test", "Tests", true),
    ("build", "Build System", true),
    ("ci", "Continuous Integration", true),
];

/// Options for generating a changelog entry.
pub struct ChangelogOptions {
    /// The new version string (e.g., "1.2.3").
    pub version: String,
    /// The previous tag for compare URL (e.g., "v1.0.0"). None for first release.
    pub previous_tag: Option<String>,
    /// The current tag for compare URL (e.g., "v1.2.3").
    pub current_tag: String,
    /// Release date in YYYY-MM-DD format.
    pub date: String,
    /// GitHub host (default: "https://github.com").
    pub host: String,
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repository: String,
    /// Custom changelog sections. If None, uses DEFAULT_SECTIONS.
    pub changelog_sections: Option<Vec<ChangelogSection>>,
}

/// Generate a markdown changelog entry from conventional commits.
pub fn generate_changelog_entry(
    commits: &[ConventionalCommit],
    options: &ChangelogOptions,
) -> String {
    let sections = build_section_config(&options.changelog_sections);
    let mut output = String::new();

    // Version header
    output.push_str(&format_version_header(options));
    output.push('\n');

    // Collect breaking changes
    let breaking_notes = collect_breaking_changes(commits);
    if !breaking_notes.is_empty() {
        output.push('\n');
        output.push_str("### ⚠ BREAKING CHANGES\n\n");
        for note in &breaking_notes {
            output.push_str(&format!("* {note}\n"));
        }
    }

    // Group commits by section
    let grouped = group_commits_by_section(commits, &sections);

    // Output each section in config order.
    // Hidden sections are included if they have commits (which only happens for breaking commits).
    for (section_name, _is_hidden) in &sections.ordered {
        if let Some(section_commits) = grouped.get(section_name.as_str()) {
            if section_commits.is_empty() {
                continue;
            }
            output.push('\n');
            output.push_str(&format!("### {section_name}\n\n"));
            for commit in section_commits {
                output.push_str(&format_commit_line(commit, options));
                output.push('\n');
            }
        }
    }

    output
}

/// Insert a new changelog entry into existing CHANGELOG.md content.
///
/// Finds the previous version header and inserts above it.
/// If no previous header exists, prepends after a `# Changelog` header.
pub fn update_changelog(existing_content: &str, new_entry: &str) -> String {
    static VERSION_HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\n###? v?[0-9\[]").unwrap());

    if existing_content.trim().is_empty() {
        return format!("# Changelog\n\n{new_entry}");
    }

    // Find the first version header in existing content
    if let Some(m) = VERSION_HEADER_RE.find(existing_content) {
        // Insert before the matched version header (keep the leading newline with the old content)
        let insert_pos = m.start();
        let before = &existing_content[..insert_pos];
        let after = &existing_content[insert_pos..];
        format!("{before}\n{new_entry}{after}")
    } else {
        // No previous version header found — append after existing content
        format!("{}\n{}", existing_content.trim_end(), new_entry)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolved section configuration: type → (section_name, hidden).
struct SectionConfig {
    type_map: HashMap<String, (String, bool)>,
    /// Sections in display order: (section_name, hidden).
    ordered: Vec<(String, bool)>,
}

fn build_section_config(custom: &Option<Vec<ChangelogSection>>) -> SectionConfig {
    let mut type_map = HashMap::new();
    let mut ordered = Vec::new();
    let mut seen_sections = std::collections::HashSet::new();

    if let Some(sections) = custom {
        for s in sections {
            type_map.insert(s.commit_type.clone(), (s.section.clone(), s.hidden));
            if seen_sections.insert(s.section.clone()) {
                ordered.push((s.section.clone(), s.hidden));
            }
        }
    } else {
        for &(commit_type, section_name, hidden) in DEFAULT_SECTIONS {
            type_map.insert(commit_type.to_string(), (section_name.to_string(), hidden));
            if seen_sections.insert(section_name.to_string()) {
                ordered.push((section_name.to_string(), hidden));
            }
        }
    }

    SectionConfig { type_map, ordered }
}

fn format_version_header(options: &ChangelogOptions) -> String {
    let version_part = if let Some(ref prev_tag) = options.previous_tag {
        let compare_url = format!(
            "{}/{}/{}/compare/{}...{}",
            options.host, options.owner, options.repository, prev_tag, options.current_tag
        );
        format!("[{}]({})", options.version, compare_url)
    } else {
        options.version.clone()
    };

    format!("## {} ({})", version_part, options.date)
}

fn collect_breaking_changes(commits: &[ConventionalCommit]) -> Vec<String> {
    let mut notes = Vec::new();
    for commit in commits {
        if !commit.breaking {
            continue;
        }
        if let Some(ref desc) = commit.breaking_description {
            let formatted = if let Some(ref scope) = commit.scope {
                if !scope.is_empty() {
                    format!("**{scope}:** {desc}")
                } else {
                    desc.clone()
                }
            } else {
                desc.clone()
            };
            notes.push(formatted);
        } else {
            // Use the subject as the breaking change description
            let formatted = if let Some(ref scope) = commit.scope {
                if !scope.is_empty() {
                    format!("**{}:** {}", scope, commit.subject)
                } else {
                    commit.subject.clone()
                }
            } else {
                commit.subject.clone()
            };
            notes.push(formatted);
        }
    }
    notes
}

fn group_commits_by_section<'a>(
    commits: &'a [ConventionalCommit],
    sections: &SectionConfig,
) -> HashMap<String, Vec<&'a ConventionalCommit>> {
    let mut grouped: HashMap<String, Vec<&ConventionalCommit>> = HashMap::new();

    for commit in commits {
        if let Some((section_name, hidden)) = sections.type_map.get(&commit.commit_type) {
            // Include hidden-type commits only if they are breaking
            if *hidden && !commit.breaking {
                continue;
            }
            grouped
                .entry(section_name.clone())
                .or_default()
                .push(commit);
        }
        // Commits with unknown types are silently dropped from changelog
    }

    grouped
}

fn format_commit_line(commit: &ConventionalCommit, options: &ChangelogOptions) -> String {
    let mut line = String::from("* ");

    // Scope in bold
    if let Some(ref scope) = commit.scope {
        if !scope.is_empty() {
            line.push_str(&format!("**{scope}:** "));
        }
    }

    // Subject
    line.push_str(&commit.subject);

    // Issue/PR references
    for reference in &commit.references {
        let url = format!(
            "{}/{}/{}/issues/{}",
            options.host, options.owner, options.repository, reference.number
        );
        line.push_str(&format!(" ([#{}]({}))", reference.number, url));
    }

    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::{ConventionalCommit, IssueReference};

    fn make_options() -> ChangelogOptions {
        ChangelogOptions {
            version: "1.2.3".to_string(),
            previous_tag: Some("v1.2.2".to_string()),
            current_tag: "v1.2.3".to_string(),
            date: "2024-01-15".to_string(),
            host: "https://github.com".to_string(),
            owner: "myorg".to_string(),
            repository: "myrepo".to_string(),
            changelog_sections: None,
        }
    }

    fn make_commit(
        commit_type: &str,
        scope: Option<&str>,
        subject: &str,
        breaking: bool,
    ) -> ConventionalCommit {
        ConventionalCommit {
            sha: "abc123".to_string(),
            commit_type: commit_type.to_string(),
            scope: scope.map(|s| s.to_string()),
            subject: subject.to_string(),
            body: None,
            footers: vec![],
            breaking,
            breaking_description: None,
            release_as: None,
            references: vec![],
        }
    }

    fn make_commit_with_ref(
        commit_type: &str,
        subject: &str,
        pr_number: u64,
    ) -> ConventionalCommit {
        ConventionalCommit {
            sha: "abc123".to_string(),
            commit_type: commit_type.to_string(),
            scope: None,
            subject: subject.to_string(),
            body: None,
            footers: vec![],
            breaking: false,
            breaking_description: None,
            release_as: None,
            references: vec![IssueReference {
                prefix: "#".to_string(),
                number: pr_number,
                action: None,
            }],
        }
    }

    // === P3.1: Entry generation ===

    #[test]
    fn test_basic_changelog_entry() {
        let options = make_options();
        let commits = vec![
            make_commit("feat", None, "add new feature", false),
            make_commit("fix", None, "resolve crash", false),
        ];

        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("## [1.2.3]"));
        assert!(entry.contains("(2024-01-15)"));
        assert!(entry.contains("### Features"));
        assert!(entry.contains("* add new feature"));
        assert!(entry.contains("### Bug Fixes"));
        assert!(entry.contains("* resolve crash"));
    }

    #[test]
    fn test_compare_url_in_header() {
        let options = make_options();
        let commits = vec![make_commit("feat", None, "feature", false)];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("[1.2.3](https://github.com/myorg/myrepo/compare/v1.2.2...v1.2.3)"));
    }

    #[test]
    fn test_no_previous_tag_plain_version() {
        let mut options = make_options();
        options.previous_tag = None;
        let commits = vec![make_commit("feat", None, "feature", false)];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("## 1.2.3 (2024-01-15)"));
        assert!(!entry.contains("[1.2.3]"));
    }

    #[test]
    fn test_breaking_changes_section_first() {
        let options = make_options();
        let commits = vec![
            make_commit("feat", None, "normal feature", false),
            make_commit("feat", Some("api"), "breaking feature", true),
        ];
        let entry = generate_changelog_entry(&commits, &options);

        let breaking_pos = entry.find("### ⚠ BREAKING CHANGES").unwrap();
        let features_pos = entry.find("### Features").unwrap();
        assert!(breaking_pos < features_pos);
    }

    #[test]
    fn test_breaking_change_with_description() {
        let options = make_options();
        let mut commit = make_commit("feat", Some("api"), "new api", true);
        commit.breaking_description = Some("removed old endpoint".to_string());
        let commits = vec![commit];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("* **api:** removed old endpoint"));
    }

    #[test]
    fn test_breaking_change_uses_subject_as_fallback() {
        let options = make_options();
        let commit = make_commit("feat", None, "redesign API", true);
        let commits = vec![commit];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("### ⚠ BREAKING CHANGES"));
        assert!(entry.contains("* redesign API"));
    }

    #[test]
    fn test_scope_formatted_as_bold() {
        let options = make_options();
        let commits = vec![make_commit("feat", Some("auth"), "add OAuth", false)];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("* **auth:** add OAuth"));
    }

    #[test]
    fn test_issue_references_linked() {
        let options = make_options();
        let commits = vec![make_commit_with_ref("fix", "fix crash", 42)];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("([#42](https://github.com/myorg/myrepo/issues/42))"));
    }

    #[test]
    fn test_hidden_sections_excluded() {
        let options = make_options();
        let commits = vec![
            make_commit("feat", None, "feature", false),
            make_commit("chore", None, "cleanup", false),
            make_commit("docs", None, "update readme", false),
        ];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("### Features"));
        assert!(!entry.contains("### Miscellaneous Chores"));
        assert!(!entry.contains("### Documentation"));
        assert!(!entry.contains("cleanup"));
        assert!(!entry.contains("update readme"));
    }

    #[test]
    fn test_hidden_type_shown_when_breaking() {
        let options = make_options();
        let commits = vec![make_commit("chore", None, "breaking cleanup", true)];
        let entry = generate_changelog_entry(&commits, &options);
        // Breaking changes section should include it
        assert!(entry.contains("### ⚠ BREAKING CHANGES"));
        // The commit should also appear in its section since it's breaking
        assert!(entry.contains("### Miscellaneous Chores"));
        assert!(entry.contains("* breaking cleanup"));
    }

    #[test]
    fn test_no_releasable_commits_empty_sections() {
        let options = make_options();
        let commits = vec![make_commit("chore", None, "cleanup", false)];
        let entry = generate_changelog_entry(&commits, &options);
        // Should still have the header but no sections
        assert!(entry.contains("## [1.2.3]"));
        assert!(!entry.contains("### "));
    }

    #[test]
    fn test_custom_sections() {
        let mut options = make_options();
        options.changelog_sections = Some(vec![
            ChangelogSection {
                commit_type: "feat".to_string(),
                section: "New Stuff".to_string(),
                hidden: false,
            },
            ChangelogSection {
                commit_type: "fix".to_string(),
                section: "Fixes".to_string(),
                hidden: false,
            },
            ChangelogSection {
                commit_type: "deps".to_string(),
                section: "Dependencies".to_string(),
                hidden: false,
            },
        ]);
        let commits = vec![
            make_commit("feat", None, "a feature", false),
            make_commit("fix", None, "a fix", false),
        ];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("### New Stuff"));
        assert!(entry.contains("### Fixes"));
        assert!(!entry.contains("### Features")); // not using default name
    }

    #[test]
    fn test_multiple_commits_per_section() {
        let options = make_options();
        let commits = vec![
            make_commit("feat", None, "feature one", false),
            make_commit("feat", Some("ui"), "feature two", false),
            make_commit("feat", None, "feature three", false),
        ];
        let entry = generate_changelog_entry(&commits, &options);
        assert!(entry.contains("* feature one"));
        assert!(entry.contains("* **ui:** feature two"));
        assert!(entry.contains("* feature three"));
        // Only one Features header
        assert_eq!(entry.matches("### Features").count(), 1);
    }

    // === P3.2: File updating ===

    #[test]
    fn test_update_empty_changelog() {
        let result = update_changelog("", "## 1.0.0 (2024-01-15)\n\n### Features\n\n* init\n");
        assert!(result.starts_with("# Changelog\n\n## 1.0.0"));
    }

    #[test]
    fn test_update_changelog_prepend() {
        let existing =
            "# Changelog\n\n## [1.0.0](url) (2024-01-01)\n\n### Features\n\n* old feature\n";
        let new_entry = "## [1.1.0](url) (2024-02-01)\n\n### Bug Fixes\n\n* a fix\n";
        let result = update_changelog(existing, new_entry);

        // New entry should come before old
        let new_pos = result.find("1.1.0").unwrap();
        let old_pos = result.find("1.0.0").unwrap();
        assert!(new_pos < old_pos);
        // Old content preserved
        assert!(result.contains("old feature"));
    }

    #[test]
    fn test_update_changelog_with_preamble() {
        let existing =
            "# Changelog\n\nAll notable changes.\n\n## [1.0.0](url) (2024-01-01)\n\n* stuff\n";
        let new_entry = "## [2.0.0](url) (2024-06-01)\n\n* new stuff\n";
        let result = update_changelog(existing, new_entry);

        // Preamble preserved
        assert!(result.contains("All notable changes."));
        // New before old
        let new_pos = result.find("2.0.0").unwrap();
        let old_pos = result.find("1.0.0").unwrap();
        assert!(new_pos < old_pos);
    }

    #[test]
    fn test_update_changelog_no_previous_version() {
        let existing = "# Changelog\n\nSome intro text.\n";
        let new_entry = "## 1.0.0 (2024-01-15)\n\n* first\n";
        let result = update_changelog(existing, new_entry);
        assert!(result.contains("# Changelog"));
        assert!(result.contains("## 1.0.0"));
    }

    #[test]
    fn test_update_changelog_preserves_content() {
        let existing = "# Changelog\n\n## [2.0.0](url) (2024-06-01)\n\n* two\n\n## [1.0.0](url) (2024-01-01)\n\n* one\n";
        let new_entry = "## [3.0.0](url) (2024-12-01)\n\n* three\n";
        let result = update_changelog(existing, new_entry);
        // All versions present in order
        let pos3 = result.find("3.0.0").unwrap();
        let pos2 = result.find("2.0.0").unwrap();
        let pos1 = result.find("1.0.0").unwrap();
        assert!(pos3 < pos2);
        assert!(pos2 < pos1);
    }
}
