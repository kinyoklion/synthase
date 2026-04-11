use regex::Regex;
use semver::Version;
use std::sync::LazyLock;

use crate::commit::ConventionalCommit;
use crate::version::{bump, BumpType};

/// Commit types that trigger a release (visible in changelog by default).
const RELEASABLE_TYPES: &[&str] = &["feat", "fix", "perf", "revert"];

/// A strategy for determining the next version from commits.
pub trait VersioningStrategy {
    /// Determine the next version given the current version and commits since last release.
    ///
    /// Returns `None` if no release should be created (no releasable commits).
    fn bump(&self, current: &Version, commits: &[ConventionalCommit]) -> Option<Version>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Analyze commits to find Release-As override, breaking count, and feature count.
struct CommitAnalysis {
    release_as: Option<String>,
    breaking: usize,
    features: usize,
    has_releasable: bool,
}

fn analyze_commits(commits: &[ConventionalCommit]) -> CommitAnalysis {
    let mut release_as = None;
    let mut breaking = 0;
    let mut features = 0;
    let mut has_releasable = false;

    for commit in commits {
        // Newest commit's Release-As wins (commits are newest-first)
        if release_as.is_none() {
            if let Some(ref ra) = commit.release_as {
                release_as = Some(ra.clone());
            }
        }

        if commit.breaking {
            breaking += 1;
            has_releasable = true;
        }

        if commit.commit_type == "feat" || commit.commit_type == "feature" {
            features += 1;
        }

        if RELEASABLE_TYPES.contains(&commit.commit_type.as_str()) {
            has_releasable = true;
        }

        // A breaking commit is always releasable regardless of type
        if commit.breaking {
            has_releasable = true;
        }
    }

    CommitAnalysis {
        release_as,
        breaking,
        features,
        has_releasable,
    }
}

/// Determine the bump type from commit analysis and pre-major flags.
fn determine_bump_type(
    current: &Version,
    analysis: &CommitAnalysis,
    bump_minor_pre_major: bool,
    bump_patch_for_minor_pre_major: bool,
) -> BumpType {
    let is_pre_major = current.major == 0;

    if analysis.breaking > 0 {
        if is_pre_major && bump_minor_pre_major {
            BumpType::Minor
        } else {
            BumpType::Major
        }
    } else if analysis.features > 0 {
        if is_pre_major && bump_patch_for_minor_pre_major {
            BumpType::Patch
        } else {
            BumpType::Minor
        }
    } else {
        BumpType::Patch
    }
}

// ---------------------------------------------------------------------------
// Default
// ---------------------------------------------------------------------------

/// The default versioning strategy matching release-please behavior.
///
/// Breaking → Major, Feature → Minor, everything else → Patch.
/// Respects pre-major flags for versions < 1.0.0.
pub struct DefaultVersioningStrategy {
    pub bump_minor_pre_major: bool,
    pub bump_patch_for_minor_pre_major: bool,
}

impl DefaultVersioningStrategy {
    pub fn new(bump_minor_pre_major: bool, bump_patch_for_minor_pre_major: bool) -> Self {
        Self {
            bump_minor_pre_major,
            bump_patch_for_minor_pre_major,
        }
    }
}

impl VersioningStrategy for DefaultVersioningStrategy {
    fn bump(&self, current: &Version, commits: &[ConventionalCommit]) -> Option<Version> {
        let analysis = analyze_commits(commits);

        // Release-As override
        if let Some(ref ra) = analysis.release_as {
            return Version::parse(ra).ok();
        }

        // No releasable commits → no release
        if !analysis.has_releasable {
            return None;
        }

        let bump_type = determine_bump_type(
            current,
            &analysis,
            self.bump_minor_pre_major,
            self.bump_patch_for_minor_pre_major,
        );

        Some(bump(current, bump_type))
    }
}

// ---------------------------------------------------------------------------
// Always-bump variants
// ---------------------------------------------------------------------------

/// Always bumps the patch version regardless of commit types.
pub struct AlwaysBumpPatch;

impl VersioningStrategy for AlwaysBumpPatch {
    fn bump(&self, current: &Version, commits: &[ConventionalCommit]) -> Option<Version> {
        let analysis = analyze_commits(commits);
        if let Some(ref ra) = analysis.release_as {
            return Version::parse(ra).ok();
        }
        Some(bump(current, BumpType::Patch))
    }
}

/// Always bumps the minor version regardless of commit types.
pub struct AlwaysBumpMinor;

impl VersioningStrategy for AlwaysBumpMinor {
    fn bump(&self, current: &Version, commits: &[ConventionalCommit]) -> Option<Version> {
        let analysis = analyze_commits(commits);
        if let Some(ref ra) = analysis.release_as {
            return Version::parse(ra).ok();
        }
        Some(bump(current, BumpType::Minor))
    }
}

/// Always bumps the major version regardless of commit types.
pub struct AlwaysBumpMajor;

impl VersioningStrategy for AlwaysBumpMajor {
    fn bump(&self, current: &Version, commits: &[ConventionalCommit]) -> Option<Version> {
        let analysis = analyze_commits(commits);
        if let Some(ref ra) = analysis.release_as {
            return Version::parse(ra).ok();
        }
        Some(bump(current, BumpType::Major))
    }
}

// ---------------------------------------------------------------------------
// Prerelease
// ---------------------------------------------------------------------------

/// Regex to find the last numeric sequence in a prerelease string.
static PRERELEASE_NUMBER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)(?:\D*$)").unwrap());

/// Increment the trailing numeric part of a prerelease string, preserving leading zeros.
///
/// Examples: "alpha.1" → "alpha.2", "beta01" → "beta02", "rc" → "rc.1"
fn bump_prerelease_number(prerelease: &str) -> String {
    if let Some(caps) = PRERELEASE_NUMBER_RE.captures(prerelease) {
        let m = caps.get(1).unwrap();
        let num_str = m.as_str();
        let num: u64 = num_str.parse().unwrap();
        let next = num + 1;
        // Preserve leading-zero padding
        let next_str = if num_str.starts_with('0') && num_str.len() > 1 {
            format!("{:0>width$}", next, width = num_str.len())
        } else {
            next.to_string()
        };
        format!(
            "{}{}{}",
            &prerelease[..m.start()],
            next_str,
            &prerelease[m.end()..]
        )
    } else {
        format!("{prerelease}.1")
    }
}

/// Build a Version with a prerelease string.
fn version_with_prerelease(major: u64, minor: u64, patch: u64, pre: &str) -> Version {
    // Use the semver crate's parser to build a proper Version
    Version::parse(&format!("{major}.{minor}.{patch}-{pre}")).unwrap()
}

/// Prerelease versioning strategy.
///
/// Manages prerelease versions (e.g., 1.2.3-alpha.1 → 1.2.3-alpha.2).
pub struct PrereleaseVersioningStrategy {
    pub prerelease_type: String,
    pub bump_minor_pre_major: bool,
    pub bump_patch_for_minor_pre_major: bool,
}

impl PrereleaseVersioningStrategy {
    pub fn new(
        prerelease_type: impl Into<String>,
        bump_minor_pre_major: bool,
        bump_patch_for_minor_pre_major: bool,
    ) -> Self {
        Self {
            prerelease_type: prerelease_type.into(),
            bump_minor_pre_major,
            bump_patch_for_minor_pre_major,
        }
    }
}

impl VersioningStrategy for PrereleaseVersioningStrategy {
    fn bump(&self, current: &Version, commits: &[ConventionalCommit]) -> Option<Version> {
        let analysis = analyze_commits(commits);

        if let Some(ref ra) = analysis.release_as {
            return Version::parse(ra).ok();
        }

        if !analysis.has_releasable {
            return None;
        }

        let bump_type = determine_bump_type(
            current,
            &analysis,
            self.bump_minor_pre_major,
            self.bump_patch_for_minor_pre_major,
        );

        let has_prerelease = !current.pre.is_empty();

        let new_version = match bump_type {
            BumpType::Patch => {
                if has_prerelease {
                    // Just bump the prerelease number
                    let new_pre = bump_prerelease_number(&current.pre.to_string());
                    version_with_prerelease(current.major, current.minor, current.patch, &new_pre)
                } else {
                    version_with_prerelease(
                        current.major,
                        current.minor,
                        current.patch + 1,
                        &self.prerelease_type,
                    )
                }
            }
            BumpType::Minor => {
                if has_prerelease && current.patch == 0 {
                    // Already at x.y.0-pre, just bump prerelease
                    let new_pre = bump_prerelease_number(&current.pre.to_string());
                    version_with_prerelease(current.major, current.minor, current.patch, &new_pre)
                } else {
                    version_with_prerelease(
                        current.major,
                        current.minor + 1,
                        0,
                        &self.prerelease_type,
                    )
                }
            }
            BumpType::Major => {
                if has_prerelease && current.patch == 0 && current.minor == 0 {
                    // Already at x.0.0-pre, just bump prerelease
                    let new_pre = bump_prerelease_number(&current.pre.to_string());
                    version_with_prerelease(current.major, current.minor, current.patch, &new_pre)
                } else {
                    version_with_prerelease(current.major + 1, 0, 0, &self.prerelease_type)
                }
            }
        };

        Some(new_version)
    }
}

// ---------------------------------------------------------------------------
// Service Pack
// ---------------------------------------------------------------------------

/// Regex for matching service pack prerelease: `sp.N`
static SERVICE_PACK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"sp\.(\d+)").unwrap());

/// Service-pack versioning strategy.
///
/// Always bumps the `sp.N` prerelease suffix, keeping major.minor.patch unchanged.
pub struct ServicePackVersioningStrategy;

impl VersioningStrategy for ServicePackVersioningStrategy {
    fn bump(&self, current: &Version, _commits: &[ConventionalCommit]) -> Option<Version> {
        let pre_str = current.pre.to_string();
        let next_sp = if let Some(caps) = SERVICE_PACK_RE.captures(&pre_str) {
            let sp_num: u64 = caps[1].parse().unwrap();
            sp_num + 1
        } else {
            1
        };

        Some(version_with_prerelease(
            current.major,
            current.minor,
            current.patch,
            &format!("sp.{next_sp}"),
        ))
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create a versioning strategy from its name and configuration.
pub fn create_versioning_strategy(
    name: &str,
    bump_minor_pre_major: bool,
    bump_patch_for_minor_pre_major: bool,
    prerelease_type: Option<&str>,
) -> Box<dyn VersioningStrategy> {
    match name {
        "default" => Box::new(DefaultVersioningStrategy::new(
            bump_minor_pre_major,
            bump_patch_for_minor_pre_major,
        )),
        "always-bump-patch" => Box::new(AlwaysBumpPatch),
        "always-bump-minor" => Box::new(AlwaysBumpMinor),
        "always-bump-major" => Box::new(AlwaysBumpMajor),
        "prerelease" => Box::new(PrereleaseVersioningStrategy::new(
            prerelease_type.unwrap_or("alpha"),
            bump_minor_pre_major,
            bump_patch_for_minor_pre_major,
        )),
        "service-pack" => Box::new(ServicePackVersioningStrategy),
        _ => Box::new(DefaultVersioningStrategy::new(
            bump_minor_pre_major,
            bump_patch_for_minor_pre_major,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_commit(sha: &str, commit_type: &str, breaking: bool) -> ConventionalCommit {
        ConventionalCommit {
            sha: sha.to_string(),
            commit_type: commit_type.to_string(),
            scope: None,
            subject: "test".to_string(),
            body: None,
            footers: vec![],
            breaking,
            breaking_description: None,
            release_as: None,
            references: vec![],
        }
    }

    fn make_release_as_commit(sha: &str, version: &str) -> ConventionalCommit {
        ConventionalCommit {
            sha: sha.to_string(),
            commit_type: "fix".to_string(),
            scope: None,
            subject: "test".to_string(),
            body: None,
            footers: vec![],
            breaking: false,
            breaking_description: None,
            release_as: Some(version.to_string()),
            references: vec![],
        }
    }

    // === Default Strategy ===

    #[test]
    fn test_default_patch_from_fix() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "fix", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 0, 1)));
    }

    #[test]
    fn test_default_minor_from_feat() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 1, 0)));
    }

    #[test]
    fn test_default_major_from_breaking_bang() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "feat", true)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(2, 0, 0)));
    }

    #[test]
    fn test_default_major_from_breaking_footer() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("1.0.0").unwrap();
        // breaking: true simulates either ! or BREAKING CHANGE footer
        let commits = vec![make_commit("a", "chore", true)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(2, 0, 0)));
    }

    #[test]
    fn test_default_highest_bump_wins() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![
            make_commit("a", "fix", false),
            make_commit("b", "feat", false),
            make_commit("c", "fix", false),
        ];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 1, 0)));
    }

    #[test]
    fn test_default_no_releasable_commits() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![
            make_commit("a", "chore", false),
            make_commit("b", "docs", false),
        ];
        assert_eq!(strategy.bump(&v, &commits), None);
    }

    #[test]
    fn test_default_release_as_override() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_release_as_commit("a", "3.0.0")];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(3, 0, 0)));
    }

    // === Pre-major behavior ===

    #[test]
    fn test_default_breaking_at_zero_bumps_major() {
        let strategy = DefaultVersioningStrategy::new(false, false);
        let v = Version::parse("0.5.0").unwrap();
        let commits = vec![make_commit("a", "feat", true)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 0, 0)));
    }

    #[test]
    fn test_default_breaking_at_zero_with_bump_minor_pre_major() {
        let strategy = DefaultVersioningStrategy::new(true, false);
        let v = Version::parse("0.5.0").unwrap();
        let commits = vec![make_commit("a", "feat", true)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(0, 6, 0)));
    }

    #[test]
    fn test_default_feat_at_zero_with_bump_patch_for_minor() {
        let strategy = DefaultVersioningStrategy::new(false, true);
        let v = Version::parse("0.5.0").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(0, 5, 1)));
    }

    #[test]
    fn test_default_both_pre_major_flags() {
        let strategy = DefaultVersioningStrategy::new(true, true);
        let v = Version::parse("0.5.1").unwrap();
        let commits = vec![make_commit("a", "feat", true)];
        // Breaking with bump_minor_pre_major → minor
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(0, 6, 0)));
    }

    // === Always-bump strategies ===

    #[test]
    fn test_always_bump_patch() {
        let strategy = AlwaysBumpPatch;
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 0, 1)));
    }

    #[test]
    fn test_always_bump_minor() {
        let strategy = AlwaysBumpMinor;
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "fix", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 1, 0)));
    }

    #[test]
    fn test_always_bump_major() {
        let strategy = AlwaysBumpMajor;
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "fix", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(2, 0, 0)));
    }

    #[test]
    fn test_always_bump_respects_release_as() {
        let strategy = AlwaysBumpPatch;
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_release_as_commit("a", "5.0.0")];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(5, 0, 0)));
    }

    // === Prerelease strategy ===

    #[test]
    fn test_prerelease_patch_bump_existing_prerelease() {
        let strategy = PrereleaseVersioningStrategy::new("alpha", false, false);
        let v = Version::parse("1.2.3-alpha.1").unwrap();
        let commits = vec![make_commit("a", "fix", false)];
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.2.3-alpha.2").unwrap())
        );
    }

    #[test]
    fn test_prerelease_patch_bump_no_prerelease() {
        let strategy = PrereleaseVersioningStrategy::new("alpha", false, false);
        let v = Version::parse("1.2.3").unwrap();
        let commits = vec![make_commit("a", "fix", false)];
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.2.4-alpha").unwrap())
        );
    }

    #[test]
    fn test_prerelease_minor_bump_at_zero_patch() {
        let strategy = PrereleaseVersioningStrategy::new("alpha", false, false);
        let v = Version::parse("1.2.0-alpha.3").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        // patch == 0, so just bump prerelease
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.2.0-alpha.4").unwrap())
        );
    }

    #[test]
    fn test_prerelease_minor_bump_nonzero_patch() {
        let strategy = PrereleaseVersioningStrategy::new("alpha", false, false);
        let v = Version::parse("1.2.3-alpha.1").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        // patch != 0, so do a real minor bump
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.3.0-alpha").unwrap())
        );
    }

    #[test]
    fn test_prerelease_major_bump_at_zero_minor_patch() {
        let strategy = PrereleaseVersioningStrategy::new("beta", false, false);
        let v = Version::parse("2.0.0-beta.1").unwrap();
        let commits = vec![make_commit("a", "feat", true)];
        // minor == 0 && patch == 0, just bump prerelease
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("2.0.0-beta.2").unwrap())
        );
    }

    #[test]
    fn test_prerelease_major_bump_nonzero_minor() {
        let strategy = PrereleaseVersioningStrategy::new("beta", false, false);
        let v = Version::parse("1.2.0-beta.1").unwrap();
        let commits = vec![make_commit("a", "feat", true)];
        // minor != 0, so do a real major bump
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("2.0.0-beta").unwrap())
        );
    }

    #[test]
    fn test_prerelease_no_existing_prerelease_feat() {
        let strategy = PrereleaseVersioningStrategy::new("rc", false, false);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.1.0-rc").unwrap())
        );
    }

    // === Prerelease number bumping ===

    #[test]
    fn test_bump_prerelease_number_dot_separated() {
        assert_eq!(bump_prerelease_number("alpha.1"), "alpha.2");
        assert_eq!(bump_prerelease_number("alpha.0"), "alpha.1");
        assert_eq!(bump_prerelease_number("alpha.99"), "alpha.100");
    }

    #[test]
    fn test_bump_prerelease_number_leading_zeros() {
        assert_eq!(bump_prerelease_number("beta01"), "beta02");
        assert_eq!(bump_prerelease_number("beta09"), "beta10");
    }

    #[test]
    fn test_bump_prerelease_number_no_number() {
        assert_eq!(bump_prerelease_number("alpha"), "alpha.1");
        assert_eq!(bump_prerelease_number("rc"), "rc.1");
    }

    // === Service Pack ===

    #[test]
    fn test_service_pack_first() {
        let strategy = ServicePackVersioningStrategy;
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![];
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.0.0-sp.1").unwrap())
        );
    }

    #[test]
    fn test_service_pack_increment() {
        let strategy = ServicePackVersioningStrategy;
        let v = Version::parse("1.0.0-sp.3").unwrap();
        let commits = vec![];
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.0.0-sp.4").unwrap())
        );
    }

    // === Factory ===

    #[test]
    fn test_factory_default() {
        let strategy = create_versioning_strategy("default", false, false, None);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 1, 0)));
    }

    #[test]
    fn test_factory_always_bump_patch() {
        let strategy = create_versioning_strategy("always-bump-patch", false, false, None);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "feat", true)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 0, 1)));
    }

    #[test]
    fn test_factory_prerelease() {
        let strategy = create_versioning_strategy("prerelease", false, false, Some("beta"));
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "fix", false)];
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.0.1-beta").unwrap())
        );
    }

    #[test]
    fn test_factory_service_pack() {
        let strategy = create_versioning_strategy("service-pack", false, false, None);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![];
        assert_eq!(
            strategy.bump(&v, &commits),
            Some(Version::parse("1.0.0-sp.1").unwrap())
        );
    }

    #[test]
    fn test_factory_unknown_falls_back_to_default() {
        let strategy = create_versioning_strategy("nonexistent", false, false, None);
        let v = Version::parse("1.0.0").unwrap();
        let commits = vec![make_commit("a", "feat", false)];
        assert_eq!(strategy.bump(&v, &commits), Some(Version::new(1, 1, 0)));
    }
}
