# Rustlease Please

A Rust reimplementation of [release-please](https://github.com/googleapis/release-please) for automated release management using [conventional commits](https://www.conventionalcommits.org/).

Unlike release-please, the core release logic operates entirely on local git repositories with no GitHub API dependency. A GitHub Action layer consumes the CLI's structured JSON output to create PRs and releases.

## Features

- Automated version bumping from conventional commit messages
- Changelog generation with configurable sections
- 12 ecosystem strategies (Rust, Node, Python, Go, Java, Helm, Dart, Ruby, PHP, Elixir, Bazel, and Simple)
- Monorepo support with per-package configuration
- Workspace plugins (Cargo, Node) for dependency cascading
- Immutable (draft) release support with artifact upload before publishing
- Compatible with release-please `release-please-config.json` and `.release-please-manifest.json` formats
- Local-first: testable without GitHub

## Quick Start (GitHub Action)

Add a workflow to your repository:

```yaml
name: Release
on:
  push:
    branches: [main]

permissions:
  contents: write
  pull-requests: write

jobs:
  release:
    runs-on: ubuntu-latest
    outputs:
      releases_created: ${{ steps.release.outputs.releases_created }}
      tag_name: ${{ steps.release.outputs.tag_name }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: kinyoklion/rustlease-please@v0
        id: release

  # Optional: build and upload artifacts when a release is created
  build:
    needs: release
    if: needs.release.outputs.releases_created == 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo "Build your project here"
      - env:
          GH_TOKEN: ${{ github.token }}
        run: gh release upload "${{ needs.release.outputs.tag_name }}" my-artifact

  # Optional: publish the draft release after artifacts are uploaded
  publish:
    needs: [release, build]
    if: needs.release.outputs.releases_created == 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: kinyoklion/rustlease-please@v0
        with:
          command: publish
          tag-name: ${{ needs.release.outputs.tag_name }}
```

Add a configuration file:

```json
{
  "release-type": "node",
  "packages": {
    ".": {}
  }
}
```

And a manifest to track the current version:

```json
{
  ".": "1.0.0"
}
```

Or use the CLI to bootstrap:

```bash
rustlease-please bootstrap --release-type node --initial-version 1.0.0
```

## Quick Start (CLI)

```bash
# Compute the next release (dry run)
rustlease-please --dry-run release-pr

# Apply changes to the working tree
rustlease-please release-pr

# Output release information
rustlease-please release
```

## Supported Ecosystems

| Release Type | Files Updated |
|---|---|
| `simple` | CHANGELOG.md, version.txt |
| `rust` | Cargo.toml (workspace members), Cargo.lock, CHANGELOG.md |
| `node` | package.json, package-lock.json, CHANGELOG.md |
| `python` | pyproject.toml, setup.py, setup.cfg, CHANGELOG.md |
| `go` | CHANGELOG.md (version from tags) |
| `java` / `maven` | pom.xml, CHANGELOG.md |
| `helm` | Chart.yaml, CHANGELOG.md |
| `dart` | pubspec.yaml, CHANGELOG.md |
| `ruby` | lib/**/version.rb, CHANGELOG.md |
| `php` | composer.json, CHANGELOG.md |
| `elixir` | mix.exs, CHANGELOG.md |
| `bazel` | MODULE.bazel, CHANGELOG.md |

Any file can also be updated using `extra-files` with [annotation markers](docs/configuration.md#extra-files).

## Documentation

- [Configuration Reference](docs/configuration.md) - All config options with defaults
- [CLI Usage](docs/cli.md) - Commands, flags, and JSON output
- [GitHub Action](docs/github-action.md) - Action inputs, outputs, and workflow patterns
- [Ecosystems](docs/ecosystems.md) - Per-ecosystem file update details
- [Conventional Commits](docs/conventional-commits.md) - Commit format and changelog generation
- [Plugins](docs/plugins.md) - Workspace and version linking plugins

## Compatibility

Rustlease-please reads the same configuration files as release-please:

- `release-please-config.json` - Release configuration
- `.release-please-manifest.json` - Version tracking

Most configuration options are compatible. See the [configuration reference](docs/configuration.md) for details.

## License

Apache License 2.0. See [LICENSE](LICENSE) for the full text.

This project is inspired by [release-please](https://github.com/googleapis/release-please) by Google. See [NOTICE](NOTICE) for attribution details.
