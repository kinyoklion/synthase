# Ecosystem Support

Each release type knows which files to update for its ecosystem. Set `release-type` in your config to match your project.

## `simple`

The minimal strategy. Updates only the changelog and an optional version file.

**Files updated:**
- `CHANGELOG.md`
- `version.txt` (only if it already exists; customizable via `version-file`)

**Use when:** Your project doesn't fit any specific ecosystem, or you only need changelog generation.

## `rust`

For Rust projects using Cargo.

**Files updated:**
- `CHANGELOG.md`
- `Cargo.toml` `[package] version` field
- `Cargo.lock` package version entry

**Workspace support:** When the root `Cargo.toml` contains `[workspace]` instead of `[package]`, the strategy finds all workspace members (including glob patterns like `crates/*`) and updates each member's `Cargo.toml` version. All workspace member entries in `Cargo.lock` are also updated.

**Config example:**
```json
{
  "release-type": "rust",
  "packages": {
    ".": {
      "component": "my-crate",
      "package-name": "my-crate"
    }
  }
}
```

## `node`

For Node.js projects using npm.

**Files updated:**
- `CHANGELOG.md`
- `package.json` `version` field
- `package-lock.json` root version and `packages[""].version`
- `npm-shrinkwrap.json` (if exists)

**Indent preservation:** The original indentation (2-space, 4-space, tabs) is detected and preserved when re-serializing JSON files.

## `python`

For Python projects.

**Files updated:**
- `CHANGELOG.md`
- `pyproject.toml` `version` field (in `[project]` or `[tool.poetry]`)
- `setup.py` `version=` argument
- `setup.cfg` `version =` in `[metadata]`

Only files that exist are updated.

## `go`

For Go modules. Version comes from git tags, so no version file is modified.

**Files updated:**
- `CHANGELOG.md`

## `java` / `maven`

For Java projects using Maven. Both `java` and `maven` map to the same strategy.

**Files updated:**
- `CHANGELOG.md`
- `pom.xml` first `<version>` element (the project version)

Dependency `<version>` elements are not modified.

## `helm`

For Helm charts.

**Files updated:**
- `CHANGELOG.md`
- `Chart.yaml` `version` field

The `appVersion` field is not modified.

## `dart`

For Dart/Flutter projects.

**Files updated:**
- `CHANGELOG.md`
- `pubspec.yaml` `version` field

## `ruby`

For Ruby gems.

**Files updated:**
- `CHANGELOG.md`
- `lib/**/version.rb` - the first semver string in quotes (customizable via `version-file`)

## `php`

For PHP projects using Composer.

**Files updated:**
- `CHANGELOG.md`
- `composer.json` `version` field

## `elixir`

For Elixir projects.

**Files updated:**
- `CHANGELOG.md`
- `mix.exs` `version:` field

## `bazel`

For Bazel projects.

**Files updated:**
- `CHANGELOG.md`
- `MODULE.bazel` first `version =` field

Dependency version fields are not modified.

## Extra Files

All strategies support `extra-files` for updating additional files. See the [configuration reference](configuration.md#extra-files) for the annotation marker format.
