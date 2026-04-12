# Configuration Reference

Synthase uses two JSON files in the repository root:

- `synthase-config.json` - Release configuration
- `.synthase-manifest.json` - Current version tracking

## Manifest File

The manifest is a simple JSON object mapping package paths to version strings:

```json
{
  ".": "1.2.3",
  "packages/foo": "0.5.0"
}
```

This file is automatically updated by release PRs. You typically create it once (via `synthase bootstrap` or manually) and let the tool manage it.

## Config File

### Minimal Example

```json
{
  "release-type": "rust",
  "packages": {
    ".": {}
  }
}
```

### Full Example

```json
{
  "$schema": "https://json-schema.org/draft-07/schema#",
  "release-type": "rust",
  "versioning": "default",
  "bump-minor-pre-major": true,
  "draft": true,
  "changelog-sections": [
    { "type": "feat", "section": "Features" },
    { "type": "fix", "section": "Bug Fixes" },
    { "type": "perf", "section": "Performance", "hidden": false }
  ],
  "extra-files": ["action/action.yml"],
  "plugins": ["cargo-workspace"],
  "packages": {
    ".": {
      "component": "my-crate",
      "package-name": "my-crate"
    }
  }
}
```

## Package Configuration

Fields can be set at the root level (defaults for all packages) or per-package (overrides). Per-package values take priority.

### Release Strategy

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `release-type` | string | `"node"` | Ecosystem strategy. One of: `simple`, `rust`, `node`, `python`, `go`, `java`, `maven`, `helm`, `dart`, `ruby`, `php`, `elixir`, `bazel` |
| `component` | string | (from path) | Component name for tagging and PR titles |
| `package-name` | string | | Package name (used for Cargo.lock, etc.) |

### Versioning

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `versioning` | string | `"default"` | Strategy: `default`, `always-bump-patch`, `always-bump-minor`, `always-bump-major`, `prerelease`, `service-pack` |
| `bump-minor-pre-major` | boolean | `false` | Breaking changes bump minor (not major) when version < 1.0.0 |
| `bump-patch-for-minor-pre-major` | boolean | `false` | Features bump patch (not minor) when version < 1.0.0 |
| `prerelease-type` | string | `"alpha"` | Prerelease identifier (for `prerelease` versioning) |
| `initial-version` | string | | Version for first release when no prior tags exist |
| `release-as` | string | | Force the next version (deprecated: prefer `Release-As` commit footer) |

### Tags

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `include-component-in-tag` | boolean | `true` | Include component name in tag (e.g., `my-lib-v1.0.0` vs `v1.0.0`) |
| `include-v-in-tag` | boolean | `true` | Include `v` prefix in tag (e.g., `v1.0.0` vs `1.0.0`) |
| `tag-separator` | string | `"-"` | Separator between component and version |

### Changelog

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `changelog-path` | string | `"CHANGELOG.md"` | Path to changelog file |
| `changelog-type` | string | `"default"` | Changelog generator type |
| `changelog-host` | string | `"https://github.com"` | Host for compare URLs (useful for GitHub Enterprise) |
| `changelog-sections` | array | (see below) | Custom section mapping |
| `skip-changelog` | boolean | `false` | Skip changelog generation |

### Pull Requests

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `pull-request-title-pattern` | string | `"chore(${branch}): release${component} ${version}"` | PR title template. Variables: `${branch}`, `${component}`, `${version}` |
| `pull-request-header` | string | `:robot: I have created a release *beep* *boop*` | PR body header |
| `pull-request-footer` | string | (link to synthase) | PR body footer |
| `separate-pull-requests` | boolean | `false` | Create separate PRs per package in monorepos |
| `draft-pull-request` | boolean | `false` | Create PRs in draft mode |

### Release Options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `draft` | boolean | `false` | Create GitHub releases as drafts |
| `prerelease` | boolean | `false` | Mark GitHub releases as prereleases |
| `skip-github-release` | boolean | `false` | Skip creating GitHub releases |

### Extra Files

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `extra-files` | array | `[]` | Additional files to update with the new version |
| `version-file` | string | | Custom version file path (used by `simple` and `ruby` strategies) |
| `exclude-paths` | array | `[]` | Paths to exclude from commit detection |

Extra files can be specified as:

**Simple string** - uses annotation markers:
```json
{ "extra-files": ["src/version.h"] }
```

The file must contain `x-synthase-version` annotations. See [annotation markers](#annotation-markers).

**Typed object** - targets specific values:
```json
{
  "extra-files": [
    { "type": "json", "path": "config.json", "jsonpath": "$.version" },
    { "type": "yaml", "path": "config.yaml", "jsonpath": "$.version" },
    { "type": "toml", "path": "config.toml", "jsonpath": "$.package.version" },
    { "type": "xml", "path": "pom.xml", "xpath": "//version" }
  ]
}
```

> Note: Typed extra-file updates (JSON/YAML/TOML/XML with jsonpath/xpath) are not yet implemented. Use annotation markers for now.

## Annotation Markers

For `extra-files` using the generic updater, add markers to your files:

### Inline Marker

Place `x-synthase-version` on the same line as the version:

```
const VERSION = "1.0.0"; // x-synthase-version
```

### Block Markers

Wrap a section with start/end markers:

```
# x-synthase-start-version
version = 1.0.0
# x-synthase-end
```

All semantic version patterns between the markers are replaced with the new version.

## Manifest-Level Options

These options can only be set at the root level (not per-package):

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `packages` | object | (required) | Map of package paths to package configs |
| `bootstrap-sha` | string | | Only consider commits after this SHA |
| `last-release-sha` | string | | Override last release detection |
| `plugins` | array | `[]` | Plugin configurations (see [plugins](plugins.md)) |
| `label` | string | `"autorelease: pending"` | Comma-separated labels for pending release PRs |
| `release-label` | string | `"autorelease: tagged"` | Comma-separated labels for released PRs |
| `group-pull-request-title-pattern` | string | `"chore: release ${branch}"` | Title pattern for grouped release PRs |
| `release-search-depth` | number | `400` | How many releases back to search |
| `commit-search-depth` | number | `500` | How many commits back to search |
| `signoff` | string | | Signed-off-by annotation text |
| `sequential-calls` | boolean | `false` | Execute releases sequentially |
| `always-update` | boolean | `false` | Always update PRs even without changes |

## Default Changelog Sections

When `changelog-sections` is not configured, these defaults are used:

| Commit Type | Section | Visible |
|---|---|---|
| `feat` | Features | Yes |
| `fix` | Bug Fixes | Yes |
| `perf` | Performance Improvements | Yes |
| `revert` | Reverts | Yes |
| `deps` | Dependencies | Yes |
| `docs` | Documentation | No |
| `style` | Styles | No |
| `chore` | Miscellaneous Chores | No |
| `refactor` | Code Refactoring | No |
| `test` | Tests | No |
| `build` | Build System | No |
| `ci` | Continuous Integration | No |

Hidden sections are included in the changelog only when commits have breaking changes.
