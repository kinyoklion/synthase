# Rustlease Please — GitHub Action

A GitHub Action for automated release management using conventional commits. Supports immutable (draft) releases with artifact upload before publishing.

## Quick Start

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

  # Build and upload artifacts only when a release is created
  build:
    needs: release
    if: needs.release.outputs.releases_created == 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo build --release
      - env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release upload "${{ needs.release.outputs.tag_name }}" \
            target/release/my-binary --clobber

  # Take the release out of draft after artifacts are uploaded
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

## How It Works

A single workflow handles everything:

1. **On a normal push to main**: The action creates/updates a release PR with version bumps and changelog.
2. **When a release PR is merged**: The action creates a draft GitHub release with tag and release notes.
3. **Your workflow builds artifacts** and uploads them to the draft release.
4. **The `publish` command** takes the release out of draft, making it immutable.

This ensures releases are never published without their artifacts attached.

## Commands

| Command | Description |
|---------|-------------|
| `release` (default) | Run both phases: create releases from merged PRs, then create/update release PRs |
| `release-pr` | Only create/update release PRs |
| `create-releases` | Only create draft releases from merged PRs |
| `publish` | Take draft releases out of draft (use with `tag-name` input) |

## Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `token` | GitHub token with contents:write and pull-requests:write | `${{ github.token }}` |
| `command` | Command to run (see above) | `release` |
| `target-branch` | Branch to create release PRs against | Repository default branch |
| `tag-name` | Tag name for the publish command | (auto-detect) |
| `config-file` | Path to `release-please-config.json` | `release-please-config.json` |
| `manifest-file` | Path to `.release-please-manifest.json` | `.release-please-manifest.json` |
| `cli-version` | CLI version to use (`build` to compile from source) | Latest published |

## Outputs

| Output | Description |
|--------|-------------|
| `releases_created` | `true` if any GitHub releases were created |
| `release_created` | `true` if a release was created for the root component |
| `prs_created` | `true` if a release PR was created/updated |
| `tag_name` | Tag name of the first release |
| `version` | Version string of the first release |
| `major` | Major version component |
| `minor` | Minor version component |
| `patch` | Patch version component |
| `upload_url` | Upload URL for the first release |
| `html_url` | HTML URL for the first release |
| `paths_released` | JSON array of released paths (for monorepo builds) |
| `releases` | JSON array of all release objects |
| `pr_number` | PR number (when a PR was created/updated) |

## Immutable Releases

The action creates releases in **draft mode** by default (when `draft: true` is set in `release-please-config.json`). This means:

1. The release is created with tag and notes, but not visible to users
2. Your workflow can build and upload artifacts to the draft release
3. The `publish` command makes it public only after artifacts are ready

This guarantees that users never see a release without its artifacts.

## Supported Ecosystems

Simple, Rust, Node, Python, Go, Java/Maven, Helm, Dart, Ruby, PHP, Elixir, Bazel — plus generic annotation-based updates for any file.
