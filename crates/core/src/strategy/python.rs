use regex::Regex;
use semver::Version;
use std::path::Path;
use std::sync::LazyLock;

use crate::config::ResolvedConfig;
use crate::error::Result;

use super::{build_changelog_update, build_extra_file_updates, join_pkg_path, FileUpdate, ReleaseStrategy};

/// Python strategy: updates pyproject.toml, setup.py, setup.cfg, and CHANGELOG.md.
pub struct PythonStrategy;

static PYPROJECT_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^(version\s*=\s*")([^"]+)(")"#).unwrap()
});

static SETUP_PY_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)(version\s*=\s*["'])([0-9]+\.[0-9]+\.[0-9]+[^"']*)(["'])"#).unwrap()
});

static SETUP_CFG_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(version\s*=\s*)(.+)$").unwrap()
});

impl ReleaseStrategy for PythonStrategy {
    fn build_updates(
        &self,
        repo_path: &Path,
        pkg_path: &str,
        new_version: &Version,
        changelog_entry: &str,
        config: &ResolvedConfig,
    ) -> Result<Vec<FileUpdate>> {
        let mut updates = Vec::new();
        let version_str = new_version.to_string();

        // Changelog
        if let Some(cl) = build_changelog_update(repo_path, pkg_path, changelog_entry, config)? {
            updates.push(cl);
        }

        // pyproject.toml
        let pyproject_path = join_pkg_path(pkg_path, "pyproject.toml");
        let pyproject_full = repo_path.join(&pyproject_path);
        if pyproject_full.exists() {
            let content = std::fs::read_to_string(&pyproject_full)?;
            let updated = PYPROJECT_VERSION_RE
                .replace(&content, |caps: &regex::Captures| {
                    format!("{}{}{}", &caps[1], version_str, &caps[3])
                })
                .to_string();
            updates.push(FileUpdate {
                path: pyproject_path,
                content: updated,
                create_if_missing: false,
            });
        }

        // setup.py
        let setup_py_path = join_pkg_path(pkg_path, "setup.py");
        let setup_py_full = repo_path.join(&setup_py_path);
        if setup_py_full.exists() {
            let content = std::fs::read_to_string(&setup_py_full)?;
            let updated = SETUP_PY_VERSION_RE
                .replace(&content, |caps: &regex::Captures| {
                    format!("{}{}{}", &caps[1], version_str, &caps[3])
                })
                .to_string();
            updates.push(FileUpdate {
                path: setup_py_path,
                content: updated,
                create_if_missing: false,
            });
        }

        // setup.cfg
        let setup_cfg_path = join_pkg_path(pkg_path, "setup.cfg");
        let setup_cfg_full = repo_path.join(&setup_cfg_path);
        if setup_cfg_full.exists() {
            let content = std::fs::read_to_string(&setup_cfg_full)?;
            let updated = SETUP_CFG_VERSION_RE
                .replace(&content, |caps: &regex::Captures| {
                    format!("{}{}", &caps[1], version_str)
                })
                .to_string();
            updates.push(FileUpdate {
                path: setup_cfg_path,
                content: updated,
                create_if_missing: false,
            });
        }

        // Extra files
        updates.extend(build_extra_file_updates(repo_path, pkg_path, new_version, &config.extra_files)?);

        Ok(updates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestRepo;

    fn python_config() -> ResolvedConfig {
        let mut defaults = crate::config::ReleaserConfig::default();
        defaults.release_type = Some("python".to_string());
        crate::config::resolve_config(&defaults, &crate::config::ReleaserConfig::default())
    }

    #[test]
    fn test_python_pyproject_toml() {
        let repo = TestRepo::new();
        repo.write_file("pyproject.toml", "[project]\nname = \"my-pkg\"\nversion = \"1.0.0\"\n");

        let strategy = PythonStrategy;
        let updates = strategy.build_updates(
            repo.path(), ".", &Version::new(1, 1, 0), "## 1.1.0\n", &python_config(),
        ).unwrap();

        let pyproject = updates.iter().find(|u| u.path == "pyproject.toml").unwrap();
        assert!(pyproject.content.contains("version = \"1.1.0\""));
        assert!(pyproject.content.contains("name = \"my-pkg\""));
    }

    #[test]
    fn test_python_setup_py() {
        let repo = TestRepo::new();
        repo.write_file("setup.py", "from setuptools import setup\nsetup(\n    name='my-pkg',\n    version='1.0.0',\n)\n");

        let strategy = PythonStrategy;
        let updates = strategy.build_updates(
            repo.path(), ".", &Version::new(2, 0, 0), "## 2.0.0\n", &python_config(),
        ).unwrap();

        let setup = updates.iter().find(|u| u.path == "setup.py").unwrap();
        assert!(setup.content.contains("version='2.0.0'"));
    }

    #[test]
    fn test_python_setup_cfg() {
        let repo = TestRepo::new();
        repo.write_file("setup.cfg", "[metadata]\nname = my-pkg\nversion = 1.0.0\n");

        let strategy = PythonStrategy;
        let updates = strategy.build_updates(
            repo.path(), ".", &Version::new(1, 0, 1), "## 1.0.1\n", &python_config(),
        ).unwrap();

        let cfg = updates.iter().find(|u| u.path == "setup.cfg").unwrap();
        assert!(cfg.content.contains("version = 1.0.1"));
    }
}
