# Conventional Commits

Synthase uses [conventional commits](https://www.conventionalcommits.org/) to determine version bumps and generate changelogs.

## Commit Format

```
type(scope): subject

body

footer-key: footer-value
```

- **type** (required): `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`, or any custom type
- **scope** (optional): A scope in parentheses, e.g., `feat(auth): ...`
- **subject** (required): A short description
- **body** (optional): Detailed description, separated from the header by a blank line
- **footers** (optional): Key-value pairs at the end of the body

## Version Bump Rules

With the default versioning strategy:

| Commit | Bump |
|--------|------|
| `feat: ...` | Minor (1.0.0 -> 1.1.0) |
| `fix: ...` | Patch (1.0.0 -> 1.0.1) |
| `perf: ...` | Patch |
| `revert: ...` | Patch |
| `feat!: ...` (breaking) | Major (1.0.0 -> 2.0.0) |
| `chore: ...` | No release |
| `docs: ...` | No release |

The highest bump type across all commits wins. For example, if you have both `fix:` and `feat:` commits, the version gets a minor bump.

## Breaking Changes

Breaking changes trigger a major version bump. They are detected from:

1. **Exclamation mark** in the header: `feat!: redesign API` or `feat(auth)!: remove old flow`
2. **BREAKING CHANGE footer**: 
   ```
   feat: new API
   
   BREAKING CHANGE: removed old endpoint
   ```
3. **BREAKING-CHANGE footer** (with hyphen):
   ```
   feat: update
   
   BREAKING-CHANGE: old format no longer supported
   ```

When a breaking change is detected on a hidden commit type (e.g., `chore!: ...`), the commit still appears in the changelog under its type's section.

## Pre-Major Versions (< 1.0.0)

For versions below 1.0.0, you can control bump behavior:

- `bump-minor-pre-major: true` - Breaking changes bump minor instead of major (0.5.0 -> 0.6.0 instead of 1.0.0)
- `bump-patch-for-minor-pre-major: true` - Features bump patch instead of minor (0.5.0 -> 0.5.1 instead of 0.6.0)

## Release-As Footer

Force a specific version for the next release:

```
fix: something important

Release-As: 2.0.0
```

When multiple commits have `Release-As`, the newest one wins. This overrides all other version calculation.

## Non-Conventional Commits

Commits that don't follow the conventional commit format are silently ignored for version bump purposes. They are not included in the changelog.

## Releasable vs Non-Releasable Types

Only certain commit types trigger a release:

**Releasable** (trigger version bump): `feat`, `fix`, `perf`, `revert`

**Non-releasable** (no version bump): `docs`, `style`, `chore`, `refactor`, `test`, `build`, `ci`

A non-releasable type can still appear in the changelog if it has a breaking change.

## Changelog Format

The generated changelog follows this format:

```markdown
## [1.2.0](https://github.com/owner/repo/compare/v1.1.0...v1.2.0) (2024-01-15)

### ⚠ BREAKING CHANGES

* **scope:** description of breaking change

### Features

* **auth:** add OAuth support (#42)
* add search functionality

### Bug Fixes

* resolve null pointer
```

- Version header includes a compare URL (when a previous tag exists) and date
- Breaking changes section always appears first
- Scopes are formatted in bold: `**scope:**`
- Issue/PR references are auto-linked: `(#42)` becomes `([#42](url))`
- Sections are ordered by the `changelog-sections` config

## Default Changelog Sections

| Commit Type | Section | Visible by Default |
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

Customize with the `changelog-sections` config:

```json
{
  "changelog-sections": [
    { "type": "feat", "section": "New Features" },
    { "type": "fix", "section": "Bugfixes" },
    { "type": "deps", "section": "Dependency Updates" },
    { "type": "chore", "section": "Maintenance", "hidden": true }
  ]
}
```
