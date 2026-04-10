use regex::Regex;
use semver::Version;
use std::fmt;
use std::sync::LazyLock;

/// Default separator between component and version in a tag.
pub const DEFAULT_SEPARATOR: &str = "-";

/// Regex for parsing version tags.
///
/// Matches: `component-v1.2.3`, `v1.2.3`, `1.2.3`, `my-lib/v1.0.0-alpha.1`, etc.
pub(crate) static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:(?P<component>.+?)(?P<separator>[^a-zA-Z0-9]))?(?P<v>v)?(?P<version>\d+\.\d+\.\d+.*)$",
    )
    .unwrap()
});

/// A structured tag name combining an optional component, separator, v-prefix, and version.
#[derive(Debug, Clone, PartialEq)]
pub struct TagName {
    /// The version portion of the tag.
    pub version: Version,
    /// Optional component/package name (e.g., "my-lib").
    pub component: Option<String>,
    /// Separator between component and version (default: "-").
    pub separator: String,
    /// Whether to include a "v" prefix before the version.
    pub include_v: bool,
}

impl TagName {
    /// Create a new TagName.
    pub fn new(
        version: Version,
        component: Option<String>,
        separator: impl Into<String>,
        include_v: bool,
    ) -> Self {
        TagName {
            version,
            component,
            separator: separator.into(),
            include_v,
        }
    }

    /// Parse a tag string into a TagName.
    ///
    /// Returns `None` if the string doesn't match the version tag pattern
    /// or the version portion isn't valid semver.
    pub fn parse(tag_str: &str) -> Option<TagName> {
        let caps = TAG_RE.captures(tag_str)?;

        let version_str = caps.name("version")?.as_str();
        let version = Version::parse(version_str).ok()?;

        let component = caps
            .name("component")
            .map(|m| m.as_str().to_string())
            .filter(|s| !s.is_empty());
        let separator = caps
            .name("separator")
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| DEFAULT_SEPARATOR.to_string());
        let include_v = caps.name("v").is_some();

        Some(TagName {
            version,
            component,
            separator,
            include_v,
        })
    }

    /// Build a TagName from resolved configuration values.
    pub fn from_config(
        version: Version,
        component: Option<String>,
        include_component_in_tag: bool,
        separator: &str,
        include_v: bool,
    ) -> Self {
        TagName {
            version,
            component: if include_component_in_tag {
                component
            } else {
                None
            },
            separator: separator.to_string(),
            include_v,
        }
    }
}

impl fmt::Display for TagName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref component) = self.component {
            write!(f, "{}{}", component, self.separator)?;
        }
        if self.include_v {
            write!(f, "v")?;
        }
        write!(f, "{}", self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Parsing tests ---

    #[test]
    fn test_parse_simple_v_tag() {
        let tag = TagName::parse("v1.2.3").unwrap();
        assert_eq!(tag.version, Version::new(1, 2, 3));
        assert!(tag.include_v);
        assert!(tag.component.is_none());
    }

    #[test]
    fn test_parse_tag_without_v() {
        let tag = TagName::parse("1.2.3").unwrap();
        assert_eq!(tag.version, Version::new(1, 2, 3));
        assert!(!tag.include_v);
        assert!(tag.component.is_none());
    }

    #[test]
    fn test_parse_component_with_dash_separator() {
        let tag = TagName::parse("my-lib-v1.2.3").unwrap();
        assert_eq!(tag.version, Version::new(1, 2, 3));
        assert_eq!(tag.component.as_deref(), Some("my-lib"));
        assert_eq!(tag.separator, "-");
        assert!(tag.include_v);
    }

    #[test]
    fn test_parse_component_with_slash_separator() {
        let tag = TagName::parse("my-lib/v1.0.0").unwrap();
        assert_eq!(tag.version, Version::new(1, 0, 0));
        assert_eq!(tag.component.as_deref(), Some("my-lib"));
        assert_eq!(tag.separator, "/");
        assert!(tag.include_v);
    }

    #[test]
    fn test_parse_component_without_v() {
        let tag = TagName::parse("my-lib-1.2.3").unwrap();
        assert_eq!(tag.version, Version::new(1, 2, 3));
        assert_eq!(tag.component.as_deref(), Some("my-lib"));
        assert!(!tag.include_v);
    }

    #[test]
    fn test_parse_prerelease_tag() {
        let tag = TagName::parse("v1.0.0-alpha.1").unwrap();
        assert_eq!(tag.version, Version::parse("1.0.0-alpha.1").unwrap());
        assert!(tag.include_v);
    }

    #[test]
    fn test_parse_component_with_prerelease() {
        let tag = TagName::parse("foo-v1.0.0-beta.2").unwrap();
        assert_eq!(tag.version, Version::parse("1.0.0-beta.2").unwrap());
        assert_eq!(tag.component.as_deref(), Some("foo"));
    }

    #[test]
    fn test_parse_non_version_tag_returns_none() {
        assert!(TagName::parse("some-random-tag").is_none());
        assert!(TagName::parse("release-candidate").is_none());
        assert!(TagName::parse("").is_none());
    }

    // --- Display/toString tests ---

    #[test]
    fn test_display_simple_v() {
        let tag = TagName::new(Version::new(1, 2, 3), None, "-", true);
        assert_eq!(tag.to_string(), "v1.2.3");
    }

    #[test]
    fn test_display_without_v() {
        let tag = TagName::new(Version::new(1, 2, 3), None, "-", false);
        assert_eq!(tag.to_string(), "1.2.3");
    }

    #[test]
    fn test_display_with_component() {
        let tag = TagName::new(
            Version::new(1, 2, 3),
            Some("my-lib".to_string()),
            "-",
            true,
        );
        assert_eq!(tag.to_string(), "my-lib-v1.2.3");
    }

    #[test]
    fn test_display_with_slash_separator() {
        let tag = TagName::new(
            Version::new(1, 0, 0),
            Some("my-lib".to_string()),
            "/",
            true,
        );
        assert_eq!(tag.to_string(), "my-lib/v1.0.0");
    }

    #[test]
    fn test_display_component_without_v() {
        let tag = TagName::new(
            Version::new(1, 2, 3),
            Some("my-lib".to_string()),
            "-",
            false,
        );
        assert_eq!(tag.to_string(), "my-lib-1.2.3");
    }

    #[test]
    fn test_display_prerelease() {
        let tag = TagName::new(
            Version::parse("1.0.0-alpha.1").unwrap(),
            None,
            "-",
            true,
        );
        assert_eq!(tag.to_string(), "v1.0.0-alpha.1");
    }

    // --- Round-trip tests ---

    #[test]
    fn test_roundtrip_simple() {
        let original = "v1.2.3";
        let parsed = TagName::parse(original).unwrap();
        assert_eq!(parsed.to_string(), original);
    }

    #[test]
    fn test_roundtrip_component() {
        let original = "my-lib-v1.2.3";
        let parsed = TagName::parse(original).unwrap();
        assert_eq!(parsed.to_string(), original);
    }

    #[test]
    fn test_roundtrip_slash() {
        let original = "my-lib/v1.0.0";
        let parsed = TagName::parse(original).unwrap();
        assert_eq!(parsed.to_string(), original);
    }

    #[test]
    fn test_roundtrip_no_v() {
        let original = "1.2.3";
        let parsed = TagName::parse(original).unwrap();
        assert_eq!(parsed.to_string(), original);
    }

    // --- from_config tests ---

    #[test]
    fn test_from_config_with_component() {
        let tag = TagName::from_config(
            Version::new(1, 2, 3),
            Some("my-lib".to_string()),
            true,
            "-",
            true,
        );
        assert_eq!(tag.to_string(), "my-lib-v1.2.3");
    }

    #[test]
    fn test_from_config_component_excluded() {
        let tag = TagName::from_config(
            Version::new(1, 2, 3),
            Some("my-lib".to_string()),
            false, // include_component_in_tag = false
            "-",
            true,
        );
        assert_eq!(tag.to_string(), "v1.2.3");
        assert!(tag.component.is_none());
    }

    #[test]
    fn test_from_config_no_v() {
        let tag = TagName::from_config(Version::new(1, 2, 3), None, false, "-", false);
        assert_eq!(tag.to_string(), "1.2.3");
    }
}
