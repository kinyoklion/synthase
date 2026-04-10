pub use semver::Version;

/// The type of version bump to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpType {
    Major,
    Minor,
    Patch,
}

/// Apply a version bump, returning the new version.
///
/// Resets lower components to zero and clears any pre-release/build metadata.
pub fn bump(version: &Version, bump_type: BumpType) -> Version {
    match bump_type {
        BumpType::Major => Version::new(version.major + 1, 0, 0),
        BumpType::Minor => Version::new(version.major, version.minor + 1, 0),
        BumpType::Patch => Version::new(version.major, version.minor, version.patch + 1),
    }
}

/// Returns true if the version is pre-major (< 1.0.0).
pub fn is_pre_major(version: &Version) -> bool {
    version.major == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bump_major() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(bump(&v, BumpType::Major), Version::new(2, 0, 0));
    }

    #[test]
    fn test_bump_minor() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(bump(&v, BumpType::Minor), Version::new(1, 3, 0));
    }

    #[test]
    fn test_bump_patch() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(bump(&v, BumpType::Patch), Version::new(1, 2, 4));
    }

    #[test]
    fn test_bump_major_from_zero() {
        let v = Version::parse("0.5.3").unwrap();
        assert_eq!(bump(&v, BumpType::Major), Version::new(1, 0, 0));
    }

    #[test]
    fn test_bump_strips_prerelease() {
        let v = Version::parse("1.2.3-alpha.1").unwrap();
        assert_eq!(bump(&v, BumpType::Patch), Version::new(1, 2, 4));
        assert_eq!(bump(&v, BumpType::Minor), Version::new(1, 3, 0));
        assert_eq!(bump(&v, BumpType::Major), Version::new(2, 0, 0));
    }

    #[test]
    fn test_is_pre_major() {
        assert!(is_pre_major(&Version::parse("0.1.0").unwrap()));
        assert!(is_pre_major(&Version::parse("0.99.99").unwrap()));
        assert!(!is_pre_major(&Version::parse("1.0.0").unwrap()));
        assert!(!is_pre_major(&Version::parse("2.0.0").unwrap()));
    }

    #[test]
    fn test_version_parsing() {
        assert!(Version::parse("1.2.3").is_ok());
        assert!(Version::parse("0.0.0").is_ok());
        assert!(Version::parse("1.2.3-alpha.1").is_ok());
        assert!(Version::parse("1.2.3+build.123").is_ok());
        assert!(Version::parse("1.2.3-beta.1+build.456").is_ok());
        assert!(Version::parse("not-a-version").is_err());
    }

    #[test]
    fn test_version_comparison() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.1.0").unwrap();
        let v3 = Version::parse("2.0.0").unwrap();
        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1 < v3);
    }
}
