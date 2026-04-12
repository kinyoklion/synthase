# GitHub Action

## Overview

The synthase GitHub Action automates release management:

1. **On every push to main**: Creates or updates a release PR with version bumps and changelog
2. **When a release PR is merged**: Creates a draft GitHub release with tag and release notes
3. **After artifacts are uploaded**: Publishes the release (takes it out of draft)

## Commands

| Command | Description |
|---------|-------------|
| `release` (default) | Run both phases: create releases for merged PRs, then create/update release PRs |
| `release-pr` | Only create/update release PRs |
| `create-releases` | Only create draft releases from merged PRs |
| `publish` | Take draft releases out of draft |

## Inputs

| Input | Default | Description |
|-------|---------|-------------|
| `token` | `${{ github.token }}` | GitHub token (needs `contents:write` and `pull-requests:write`) |
| `command` | `release` | Command to run |
| `target-branch` | (repo default) | Branch to target for releases |
| `tag-name` | | Tag name for the `publish` command |
| `config-file` | `synthase-config.json` | Path to config file |
| `manifest-file` | `.synthase-manifest.json` | Path to manifest |
| `cli-version` | (latest published) | CLI version, or `build` to compile from source |

## Outputs

| Output | Description |
|--------|-------------|
| `releases_created` | `true` if any GitHub releases were created |
| `release_created` | `true` if the root component was released |
| `prs_created` | `true` if a release PR was created/updated |
| `tag_name` | Tag name of the first release |
| `version` | Version string of the first release |
| `major` | Major version component |
| `minor` | Minor version component |
| `patch` | Patch version component |
| `upload_url` | Upload URL for the first release |
| `html_url` | HTML URL for the first release |
| `paths_released` | JSON array of released paths (for monorepo matrix builds) |
| `releases` | JSON array of all release objects |
| `pr_number` | PR number (when a PR was created/updated) |

## Workflow Patterns

### Simple (no artifacts)

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
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: kinyoklion/synthase@v0
        id: release
```

### With Artifacts (Immutable Releases)

For projects that need to build and upload artifacts before making a release public:

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
      - uses: kinyoklion/synthase@v0
        id: release

  build:
    needs: release
    if: needs.release.outputs.releases_created == 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: make build
      - env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release upload "${{ needs.release.outputs.tag_name }}" \
            dist/my-binary --clobber

  publish:
    needs: [release, build]
    if: needs.release.outputs.releases_created == 'true'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: kinyoklion/synthase@v0
        with:
          command: publish
          tag-name: ${{ needs.release.outputs.tag_name }}
```

### Monorepo

For monorepos, use `paths_released` to determine which components need builds:

```yaml
  release:
    steps:
      - uses: kinyoklion/synthase@v0
        id: release

  build-a:
    needs: release
    if: contains(fromJSON(needs.release.outputs.paths_released || '[]'), 'packages/a')
    steps:
      - run: echo "Build package A"
```

## Immutable Releases

When `draft: true` is set in `synthase-config.json`, the action creates releases in draft mode. This means:

1. The release is created with a tag and release notes but is not visible to users
2. Your workflow builds and uploads artifacts to the draft release
3. The `publish` command makes it public

This guarantees that users never see a release without its artifacts. GitHub does not create a git tag for draft releases by default, so the action explicitly creates the tag via the API to ensure the CLI can find the release boundary on subsequent runs.

## Self-Hosting

When using the action from the same repository (e.g., to release synthase itself), use `cli-version: build` to compile from source:

```yaml
- uses: ./action
  with:
    cli-version: build
```

This avoids the chicken-and-egg problem where the version annotation points to a release that doesn't have a binary yet.

## Label Lifecycle

The action manages PR labels to track state:

1. **Release PR created** - labeled `autorelease: pending`
2. **Release PR merged** - still `autorelease: pending` (awaiting release creation)
3. **Release created** - label changed to `autorelease: tagged`

The `release-pr` phase checks for merged PRs with `autorelease: pending` and will not create a new release PR until the release is tagged. This prevents duplicate PRs.
