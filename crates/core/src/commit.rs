use regex::Regex;
use std::sync::LazyLock;

/// A parsed conventional commit.
#[derive(Debug, Clone, PartialEq)]
pub struct ConventionalCommit {
    /// Git commit SHA.
    pub sha: String,
    /// Commit type (e.g., "feat", "fix", "chore").
    pub commit_type: String,
    /// Optional scope (e.g., "auth" from "feat(auth): ...").
    pub scope: Option<String>,
    /// The subject line (message after "type(scope): ").
    pub subject: String,
    /// The commit body (everything after the first blank line, excluding footers and extended changelog block).
    pub body: Option<String>,
    /// Parsed footers.
    pub footers: Vec<Footer>,
    /// Whether this commit contains a breaking change.
    pub breaking: bool,
    /// Description of the breaking change, if any.
    pub breaking_description: Option<String>,
    /// Version override from `Release-As` footer.
    pub release_as: Option<String>,
    /// Issue/PR references found in the commit.
    pub references: Vec<IssueReference>,
    /// Extended changelog description from BEGIN_EXTENDED_CHANGELOG / END_EXTENDED_CHANGELOG block.
    pub extended_description: Option<String>,
}

/// A commit message footer (key-value pair).
#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub key: String,
    pub value: String,
}

/// A reference to an issue or pull request.
#[derive(Debug, Clone, PartialEq)]
pub struct IssueReference {
    pub prefix: String,
    pub number: u64,
    pub action: Option<String>,
}

// --- Regex patterns ---

static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<type>[a-zA-Z]+)(?:\((?P<scope>[^)]*)\))?(?P<breaking>!)?:\s*(?P<subject>.+)$")
        .unwrap()
});

static FOOTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<key>[A-Za-z][A-Za-z0-9 -]*)(?::\s*| #)(?P<value>.*)$").unwrap()
});

static ISSUE_REF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(?P<number>\d+)").unwrap());

static BREAKING_FOOTER_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^BREAKING[- ]CHANGE$").unwrap());

static RELEASE_AS_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^Release-As$").unwrap());

/// Parse a git commit message as a conventional commit.
///
/// Returns `None` if the message does not follow the conventional commit format.
pub fn parse_conventional_commit(sha: &str, message: &str) -> Option<ConventionalCommit> {
    let first_line = message.lines().next().unwrap_or("");
    let caps = HEADER_RE.captures(first_line)?;

    let commit_type = caps["type"].to_string();
    let scope = caps.name("scope").map(|m| m.as_str().to_string());
    let has_breaking_marker = caps.name("breaking").is_some();
    let subject = caps["subject"].trim().to_string();

    // Split body from header: everything after the first blank line
    let raw_body_text = extract_body(message);

    // Extract and strip BEGIN/END_EXTENDED_CHANGELOG block before further parsing
    let (extended_description, body_text) = match raw_body_text {
        Some(ref text) => {
            let (desc, stripped) = extract_extended_changelog(text);
            let stripped_opt = if stripped.trim().is_empty() {
                None
            } else {
                Some(stripped)
            };
            (desc, stripped_opt)
        }
        None => (None, None),
    };

    // Parse footers from the body
    let (body, footers) = parse_body_and_footers(body_text.as_deref());

    // Detect breaking changes
    let mut breaking = has_breaking_marker;
    let mut breaking_description = None;

    for footer in &footers {
        if BREAKING_FOOTER_KEY.is_match(&footer.key) {
            breaking = true;
            if breaking_description.is_none() && !footer.value.is_empty() {
                breaking_description = Some(footer.value.clone());
            }
        }
    }

    // Also check body for BREAKING CHANGE/BREAKING-CHANGE lines not captured as footers
    if !breaking {
        if let Some(ref b) = body_text {
            for line in b.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("BREAKING CHANGE:")
                    || trimmed.starts_with("BREAKING-CHANGE:")
                {
                    breaking = true;
                    let desc = trimmed
                        .split_once(':')
                        .map(|x| x.1)
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    if breaking_description.is_none() {
                        breaking_description = desc;
                    }
                }
            }
        }
    }

    // Detect Release-As
    let release_as = footers
        .iter()
        .rfind(|f| RELEASE_AS_KEY.is_match(&f.key))
        .map(|f| f.value.trim().to_string());

    // Extract issue references from subject, body, and footers
    let mut references = Vec::new();
    extract_issue_refs(&subject, None, &mut references);
    if let Some(ref b) = body {
        extract_issue_refs(b, None, &mut references);
    }
    for footer in &footers {
        let is_action_footer = footer.key.eq_ignore_ascii_case("closes")
            || footer.key.eq_ignore_ascii_case("fixes")
            || footer.key.eq_ignore_ascii_case("resolves")
            || footer.key.eq_ignore_ascii_case("refs");

        let action = if is_action_footer {
            Some(footer.key.clone())
        } else {
            None
        };

        // Extract #N references from footer value
        extract_issue_refs(&footer.value, action.clone(), &mut references);

        // If the footer value is a bare number (from "key #N" syntax), treat as a ref
        if is_action_footer {
            if let Ok(number) = footer.value.trim().parse::<u64>() {
                // Only add if not already captured by the #N extraction above
                let already_found = references
                    .iter()
                    .any(|r| r.number == number && r.action.as_deref() == action.as_deref());
                if !already_found {
                    references.push(IssueReference {
                        prefix: "#".to_string(),
                        number,
                        action,
                    });
                }
            }
        }
    }

    Some(ConventionalCommit {
        sha: sha.to_string(),
        commit_type,
        scope,
        subject,
        body,
        footers,
        breaking,
        breaking_description,
        release_as,
        references,
        extended_description,
    })
}

/// Extract the body portion of a commit message (everything after the first blank line).
fn extract_body(message: &str) -> Option<String> {
    // Find the first blank line (separates header from body)
    let mut lines = message.lines();
    lines.next(); // skip header

    // Skip until we find a blank line, then collect the rest
    let mut found_blank = false;
    let mut body_lines = Vec::new();

    for line in lines {
        if !found_blank {
            if line.trim().is_empty() {
                found_blank = true;
            }
            continue;
        }
        body_lines.push(line);
    }

    if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n"))
    }
}

/// Parse body text into a prose body and structured footers.
///
/// Footers are key-value lines at the end of the body, following the
/// conventional commits spec (e.g., `BREAKING CHANGE: desc` or `Closes #123`).
fn parse_body_and_footers(body_text: Option<&str>) -> (Option<String>, Vec<Footer>) {
    let body_text = match body_text {
        Some(t) if !t.trim().is_empty() => t,
        _ => return (None, Vec::new()),
    };

    let lines: Vec<&str> = body_text.lines().collect();

    // Walk backwards to find where footers begin.
    // Footers are contiguous key-value lines at the end of the body.
    // A blank line or a non-footer line terminates the footer section going backwards.
    let mut footer_start = lines.len();
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            // Blank line: footers can be preceded by a blank line
            break;
        }
        if FOOTER_RE.is_match(line) {
            footer_start = i;
        } else {
            // Non-footer, non-blank line — stop looking
            break;
        }
    }

    let mut footers = Vec::new();
    for line in &lines[footer_start..] {
        let trimmed = line.trim();
        if let Some(caps) = FOOTER_RE.captures(trimmed) {
            footers.push(Footer {
                key: caps["key"].to_string(),
                value: caps["value"].to_string(),
            });
        }
    }

    // Body is everything before the footer section
    let body_part: Vec<&str> = lines[..footer_start].to_vec();
    let body = body_part.join("\n");
    let body = if body.trim().is_empty() {
        None
    } else {
        Some(body)
    };

    (body, footers)
}

/// Extract the extended changelog description from a body, returning (description, stripped_body).
///
/// Scans for a `BEGIN_EXTENDED_CHANGELOG` / `END_EXTENDED_CHANGELOG` block. The content
/// between the markers becomes the extended description; the entire block (including markers)
/// is removed from the returned body string.
fn extract_extended_changelog(body: &str) -> (Option<String>, String) {
    const BEGIN: &str = "BEGIN_EXTENDED_CHANGELOG";
    const END: &str = "END_EXTENDED_CHANGELOG";

    let Some(start) = body.find(BEGIN) else {
        return (None, body.to_string());
    };

    let after_begin = &body[start + BEGIN.len()..];
    // Skip to the start of the next line
    let content_offset = after_begin
        .find('\n')
        .map(|i| i + 1)
        .unwrap_or(after_begin.len());
    let content_and_rest = &after_begin[content_offset..];

    let Some(end_rel) = content_and_rest.find(END) else {
        return (None, body.to_string());
    };

    let raw_desc = content_and_rest[..end_rel].trim().to_string();
    let extended = if raw_desc.is_empty() {
        None
    } else {
        Some(raw_desc)
    };

    // Strip the entire block (from BEGIN up to end of the END line) from body
    let end_abs = start + BEGIN.len() + content_offset + end_rel + END.len();
    let after_end = &body[end_abs..];
    // Consume a trailing newline if present so we don't leave a blank line artifact
    let after_end = after_end.strip_prefix('\n').unwrap_or(after_end);
    let stripped = format!("{}{}", body[..start].trim_end(), after_end);
    let stripped = stripped.trim_end().to_string();

    (extended, stripped)
}

/// Extract `#N` references from text.
fn extract_issue_refs(text: &str, action: Option<String>, refs: &mut Vec<IssueReference>) {
    for caps in ISSUE_REF_RE.captures_iter(text) {
        if let Ok(number) = caps["number"].parse::<u64>() {
            refs.push(IssueReference {
                prefix: "#".to_string(),
                number,
                action: action.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_feat() {
        let commit = parse_conventional_commit("abc123", "feat: add new feature").unwrap();
        assert_eq!(commit.commit_type, "feat");
        assert_eq!(commit.scope, None);
        assert_eq!(commit.subject, "add new feature");
        assert!(!commit.breaking);
        assert!(commit.body.is_none());
        assert!(commit.footers.is_empty());
    }

    #[test]
    fn test_fix_with_scope() {
        let commit = parse_conventional_commit("abc123", "fix(auth): resolve login crash").unwrap();
        assert_eq!(commit.commit_type, "fix");
        assert_eq!(commit.scope.as_deref(), Some("auth"));
        assert_eq!(commit.subject, "resolve login crash");
    }

    #[test]
    fn test_breaking_with_bang() {
        let commit = parse_conventional_commit("abc123", "feat!: redesign API").unwrap();
        assert!(commit.breaking);
        assert_eq!(commit.commit_type, "feat");
    }

    #[test]
    fn test_breaking_with_scope_and_bang() {
        let commit =
            parse_conventional_commit("abc123", "feat(api)!: remove old endpoints").unwrap();
        assert!(commit.breaking);
        assert_eq!(commit.scope.as_deref(), Some("api"));
    }

    #[test]
    fn test_breaking_change_footer() {
        let msg = "feat: new API\n\nBREAKING CHANGE: removed old endpoint";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert!(commit.breaking);
        assert_eq!(
            commit.breaking_description.as_deref(),
            Some("removed old endpoint")
        );
    }

    #[test]
    fn test_breaking_hyphen_footer() {
        let msg = "feat: new API\n\nBREAKING-CHANGE: removed old endpoint";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert!(commit.breaking);
        assert_eq!(
            commit.breaking_description.as_deref(),
            Some("removed old endpoint")
        );
    }

    #[test]
    fn test_release_as_footer() {
        let msg = "fix: something\n\nRelease-As: 2.0.0";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert_eq!(commit.release_as.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn test_body_and_footers() {
        let msg = "feat: add feature\n\nThis is the body.\nIt has multiple lines.\n\nCloses: #42\nRefs: #100";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert!(commit.body.as_ref().unwrap().contains("This is the body."));
        assert_eq!(commit.footers.len(), 2);
        assert_eq!(commit.footers[0].key, "Closes");
        assert_eq!(commit.footers[0].value, "#42");
        assert_eq!(commit.footers[1].key, "Refs");
        assert_eq!(commit.footers[1].value, "#100");
    }

    #[test]
    fn test_issue_references_in_subject() {
        let commit = parse_conventional_commit("abc123", "fix: resolve crash (#42)").unwrap();
        assert_eq!(commit.references.len(), 1);
        assert_eq!(commit.references[0].number, 42);
        assert_eq!(commit.references[0].action, None);
    }

    #[test]
    fn test_issue_references_from_footer_action() {
        // Using "key: value" syntax
        let msg = "fix: resolve crash\n\nCloses: #42";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        let close_refs: Vec<_> = commit
            .references
            .iter()
            .filter(|r| r.action.as_deref() == Some("Closes"))
            .collect();
        assert_eq!(close_refs.len(), 1);
        assert_eq!(close_refs[0].number, 42);
    }

    #[test]
    fn test_issue_references_from_footer_hash_syntax() {
        // Using "key #value" syntax (conventional commits spec)
        let msg = "fix: resolve crash\n\nCloses #42";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert_eq!(commit.footers.len(), 1);
        assert_eq!(commit.footers[0].key, "Closes");
        assert_eq!(commit.footers[0].value, "42");
        // The number should also be extractable as a reference
        // (the footer value is just "42" since # is part of the separator)
    }

    #[test]
    fn test_extended_changelog_extracted() {
        let msg = "feat: add parser\n\nBEGIN_EXTENDED_CHANGELOG\nThis is the extended description.\nIt has two lines.\nEND_EXTENDED_CHANGELOG";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert_eq!(
            commit.extended_description.as_deref(),
            Some("This is the extended description.\nIt has two lines.")
        );
        // Should not appear in body
        assert!(commit.body.is_none());
    }

    #[test]
    fn test_extended_changelog_with_body_before() {
        let msg = "feat: add parser\n\nSome context about the change.\n\nBEGIN_EXTENDED_CHANGELOG\nExtended text.\nEND_EXTENDED_CHANGELOG";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert_eq!(
            commit.extended_description.as_deref(),
            Some("Extended text.")
        );
        assert!(commit.body.as_deref().unwrap().contains("Some context"));
        assert!(!commit
            .body
            .as_deref()
            .unwrap()
            .contains("BEGIN_EXTENDED_CHANGELOG"));
    }

    #[test]
    fn test_extended_changelog_with_footer() {
        let msg = "feat: add parser\n\nBEGIN_EXTENDED_CHANGELOG\nExtended text.\nEND_EXTENDED_CHANGELOG\n\nCloses: #42";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert_eq!(
            commit.extended_description.as_deref(),
            Some("Extended text.")
        );
        assert_eq!(commit.footers.len(), 1);
        assert_eq!(commit.footers[0].key, "Closes");
    }

    #[test]
    fn test_no_extended_changelog_block() {
        let commit = parse_conventional_commit("abc123", "feat: add parser").unwrap();
        assert!(commit.extended_description.is_none());
    }

    #[test]
    fn test_non_conventional_returns_none() {
        assert!(parse_conventional_commit("abc123", "random commit message").is_none());
        assert!(parse_conventional_commit("abc123", "Update README.md").is_none());
        assert!(parse_conventional_commit("abc123", "").is_none());
    }

    #[test]
    fn test_all_standard_types() {
        for t in &[
            "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
            "revert",
        ] {
            let msg = format!("{t}: do something");
            let commit = parse_conventional_commit("sha", &msg);
            assert!(commit.is_some(), "should parse type: {t}");
            assert_eq!(commit.unwrap().commit_type, *t);
        }
    }

    #[test]
    fn test_empty_scope() {
        let commit = parse_conventional_commit("abc123", "feat(): empty scope").unwrap();
        assert_eq!(commit.scope.as_deref(), Some(""));
    }

    #[test]
    fn test_multiple_issue_references() {
        let msg =
            "fix: resolve crashes (#1, #2)\n\nSome body text about #3.\n\nFixes: #4\nCloses: #5";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        let numbers: Vec<u64> = commit.references.iter().map(|r| r.number).collect();
        assert!(numbers.contains(&1), "subject ref #1");
        assert!(numbers.contains(&2), "subject ref #2");
        assert!(numbers.contains(&3), "body ref #3");
        assert!(numbers.contains(&4), "footer ref #4");
        assert!(numbers.contains(&5), "footer ref #5");
    }

    #[test]
    fn test_body_only_no_footers() {
        let msg = "feat: add feature\n\nThis is just a body.\nNo footers here.";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert!(commit.body.is_some());
        assert!(commit.footers.is_empty());
    }

    #[test]
    fn test_breaking_change_in_body_text() {
        let msg = "feat: something\n\nSome context.\nBREAKING CHANGE: this is breaking";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert!(commit.breaking);
        assert_eq!(
            commit.breaking_description.as_deref(),
            Some("this is breaking")
        );
    }

    #[test]
    fn test_release_as_last_wins() {
        // When multiple Release-As footers, last one wins
        let msg = "fix: thing\n\nRelease-As: 3.0.0\nRelease-As: 2.0.0";
        let commit = parse_conventional_commit("abc123", msg).unwrap();
        assert_eq!(commit.release_as.as_deref(), Some("2.0.0"));
    }
}
