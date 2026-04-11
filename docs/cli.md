# CLI Usage

## Installation

Download the pre-built binary from [GitHub Releases](https://github.com/kinyoklion/rustlease-please/releases), or build from source:

```bash
cargo install --path crates/cli
```

## Global Options

| Flag | Default | Description |
|------|---------|-------------|
| `--repo-path <PATH>` | `.` | Path to the git repository |
| `--target-branch <BRANCH>` | `main` | Target branch for releases |
| `--dry-run` | off | Compute changes without writing files |
| `--help` | | Show help |
| `--version` | | Show version |

## Commands

### `release-pr`

Compute the next release and output PR information as JSON.

```bash
rustlease-please release-pr
rustlease-please --dry-run release-pr
rustlease-please --repo-path /path/to/repo release-pr
```

**What it does:**
1. Reads `release-please-config.json` and `.release-please-manifest.json`
2. Walks git history from HEAD to the last release tag
3. Parses conventional commits to determine version bump
4. Generates changelog entries
5. Computes file updates (Cargo.toml, package.json, etc.)
6. Outputs structured JSON to stdout

**Without `--dry-run`:** Writes updated files to disk (changelog, version files, manifest).

**With `--dry-run`:** Only outputs JSON, no files modified.

**JSON output format:**
```json
{
  "releases": [
    {
      "component": "my-package",
      "path": ".",
      "current_version": "1.0.0",
      "new_version": "1.1.0",
      "tag": "my-package-v1.1.0",
      "changelog_entry": "## [1.1.0](...) (2024-01-15)\n\n### Features\n...",
      "draft": false,
      "prerelease": false,
      "skip_github_release": false
    }
  ],
  "pull_requests": [
    {
      "title": "chore(main): release my-package 1.1.0",
      "body": ":robot: I have created a release ...",
      "branch": "release-please--branches--main",
      "files": [
        {
          "path": "CHANGELOG.md",
          "content": "...",
          "create_if_missing": true
        }
      ]
    }
  ]
}
```

### `release`

Output release information for creating GitHub releases.

```bash
rustlease-please release
```

**JSON output format:**
```json
{
  "releases": [
    {
      "component": "my-package",
      "path": ".",
      "version": "1.1.0",
      "tag": "my-package-v1.1.0",
      "release_notes": "### Features\n...",
      "draft": true,
      "prerelease": false,
      "skip_github_release": false
    }
  ]
}
```

### `bootstrap`

Initialize release-please configuration for a repository.

```bash
rustlease-please bootstrap --release-type rust --initial-version 0.1.0 --component my-crate
```

| Flag | Default | Description |
|------|---------|-------------|
| `--release-type <TYPE>` | `simple` | Release strategy |
| `--initial-version <VERSION>` | `0.0.0` | Starting version |
| `--component <NAME>` | (none) | Component/package name |

**Creates:**
- `release-please-config.json`
- `.release-please-manifest.json`

Skips creation if config file already exists.
