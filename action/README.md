# Rustlease Please — GitHub Action

A GitHub Action for automated release management using conventional commits. Wraps the `rustlease-please` Rust CLI.

## Usage

### Basic: Create Release PRs

```yaml
name: Release Please
on:
  push:
    branches: [main]

permissions:
  contents: write
  pull-requests: write

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Full history needed for commit analysis

      - uses: ./action
        id: release
        with:
          command: release-pr

      - name: Use release outputs
        if: steps.release.outputs.prs_created == 'true'
        run: |
          echo "PR #${{ steps.release.outputs.pr_number }} created"
          echo "Version: ${{ steps.release.outputs.version }}"
```

### Create GitHub Releases on PR Merge

```yaml
name: Release
on:
  pull_request:
    types: [closed]
    branches: [main]

permissions:
  contents: write
  pull-requests: write

jobs:
  release:
    if: github.event.pull_request.merged == true
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: ./action
        id: release
        with:
          command: release

      - name: Use release outputs
        if: steps.release.outputs.releases_created == 'true'
        run: |
          echo "Released ${{ steps.release.outputs.tag_name }}"
```

### Combined Workflow

```yaml
name: Release Please
on:
  push:
    branches: [main]

permissions:
  contents: write
  pull-requests: write

jobs:
  release-pr:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: ./action
        id: release-pr
        with:
          command: release-pr

      - uses: ./action
        id: release
        with:
          command: release

      - if: steps.release.outputs.releases_created == 'true'
        run: echo "Released ${{ steps.release.outputs.tag_name }}"
```

## Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `token` | GitHub token with contents:write and pull-requests:write | `${{ github.token }}` |
| `command` | `release-pr` or `release` | `release-pr` |
| `target-branch` | Branch to create release PRs against | Repository default branch |
| `config-file` | Path to `release-please-config.json` | `release-please-config.json` |
| `manifest-file` | Path to `.release-please-manifest.json` | `.release-please-manifest.json` |
| `cli-version` | CLI version to use (`build` to compile from source) | `build` |

## Outputs

| Output | Description |
|--------|-------------|
| `releases_created` | `true` if GitHub releases were created |
| `prs_created` | `true` if a release PR was created/updated |
| `releases` | JSON array of release objects |
| `pr_number` | PR number (release-pr mode) |
| `tag_name` | Tag name of first release |
| `version` | Version string of first release |
| `major` | Major version component |
| `minor` | Minor version component |
| `patch` | Patch version component |

## Prerequisites

Your repository needs:
1. `release-please-config.json` — configuration file (use `rustlease-please bootstrap` to create)
2. `.release-please-manifest.json` — version tracking (auto-created)
3. Conventional commit messages (`feat:`, `fix:`, `feat!:`, etc.)

## Supported Ecosystems

Simple, Rust, Node, Python, Go, Java/Maven, Helm, Dart, Ruby, PHP, Elixir, Bazel — plus generic annotation-based updates for any file.

## How It Works

1. **`release-pr`**: Analyzes commits since last release, computes version bumps, generates changelogs, and creates/updates a release PR with all file changes.

2. **`release`**: Finds merged release PRs, creates GitHub releases with tags and release notes, updates PR labels from `autorelease: pending` to `autorelease: tagged`.
