# Synthase End-to-End Test Plan

## Overview

This plan defines a series of test repositories that exercise synthase's full release lifecycle through GitHub Actions. Each test repo is created under the `kinyoklion-2` GitHub organization, configured with synthase, and walked through a complete release cycle including validation at every step.

Tests are run sequentially — complete one before moving to the next. Each test builds confidence in a specific release strategy before adding complexity.

## General Validation Checklist

At each stage, validate these items as applicable:

### Release PR Validation
- [ ] PR title matches pattern: `chore(main): release <component> <version>`
- [ ] PR body contains changelog with correct sections (Features, Bug Fixes, etc.)
- [ ] PR has `autorelease: pending` label
- [ ] PR branch is `synthase--branches--main`
- [ ] CHANGELOG.md is updated with new entry
- [ ] Version file(s) updated to new version (Cargo.toml, package.json, etc.)
- [ ] Manifest file (`.synthase-manifest.json`) updated with new version
- [ ] Extra-files updated (if configured)

### Release PR Update Validation
- [ ] Same PR number (no duplicate PR created)
- [ ] PR title updated with new version (if bump type changed)
- [ ] PR body updated with additional changelog entries
- [ ] File diffs include the new commits' changes

### Release Validation (after merge)
- [ ] GitHub release created (draft if `draft: true`)
- [ ] Release tag matches expected format (e.g., `my-app-v1.0.0`)
- [ ] Git tag exists (even for draft releases)
- [ ] Release notes match changelog entry
- [ ] PR label changed from `autorelease: pending` to `autorelease: tagged`
- [ ] Artifacts uploaded to release (if applicable)
- [ ] Release published (taken out of draft, if applicable)
- [ ] No duplicate release PR created after merge

### Post-Release Validation
- [ ] Next push creates new release PR (not duplicate of old one)
- [ ] New PR version is correct (incremented from released version)
- [ ] Commit boundary is correct (only commits after the release tag)

---

## Test 1: Simple Strategy

**Purpose:** Validate the most basic release flow — changelog-only with a version.txt file and a build artifact.

### Repository Setup

**Repo name:** `kinyoklion-2/synthase-test-simple`

**Files:**
```
synthase-config.json
.synthase-manifest.json
version.txt
build.sh
.github/workflows/release.yml
```

**`synthase-config.json`:**
```json
{
  "release-type": "simple",
  "draft": true,
  "packages": {
    ".": {
      "component": "my-app"
    }
  }
}
```

**`.synthase-manifest.json`:**
```json
{
  ".": "0.0.0"
}
```

**`version.txt`:**
```
0.0.0
```

**`build.sh`:**
```bash
#!/bin/bash
echo "my-app $(cat version.txt)" > my-app.txt
```

**`.github/workflows/release.yml`:**
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
      - name: Build artifact
        run: |
          bash build.sh
          echo "Built: $(cat my-app.txt)"
      - name: Upload to release
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release upload "${{ needs.release.outputs.tag_name }}" \
            my-app.txt --clobber

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

### Test Steps

#### Step 1.1: Initial setup commit
- Push the initial repo with all files above
- **Expected:** Workflow runs, creates release PR for `0.1.0` (feat from initial commit)
- **Validate:** Release PR checklist (title, body, label, version.txt updated, CHANGELOG.md created)

#### Step 1.2: Merge initial release PR
- Merge the release PR
- **Expected:** Workflow runs, creates draft release `my-app-v0.1.0`, builds artifact, publishes
- **Validate:** Release checklist (tag, release notes, artifact `my-app.txt` attached, label changed, release published)

#### Step 1.3: Add a feature
- Push commit: `feat: add greeting`
- Add file `greeting.txt` with content "hello"
- **Expected:** New release PR for `0.2.0` (minor bump from feat)
- **Validate:** Release PR checklist, version.txt shows `0.2.0`, CHANGELOG has new entry below old one

#### Step 1.4: Add a fix (update existing PR)
- Push commit: `fix: correct greeting`
- Change `greeting.txt` to "hello world"
- **Expected:** Existing PR updated (not a new PR), still `0.2.0` (feat > fix)
- **Validate:** PR update checklist — same PR number, body now has both feat and fix entries

#### Step 1.5: Merge second release PR
- Merge the release PR
- **Expected:** Draft release `my-app-v0.2.0`, artifact uploaded, published
- **Validate:** Release checklist, CHANGELOG has both `0.1.0` and `0.2.0` entries, artifact content says `my-app 0.2.0`

#### Step 1.6: Add a breaking change
- Push commit: `feat!: redesign app`
- **Expected:** New release PR for `1.0.0` (major bump from breaking change)
- **Validate:** PR has `⚠ BREAKING CHANGES` section in body

---

## Test 2: Rust Strategy

**Purpose:** Validate Rust/Cargo workspace release flow including Cargo.toml and Cargo.lock updates, extra-files, and bump-minor-pre-major.

### Repository Setup

**Repo name:** `kinyoklion-2/synthase-test-rust`

**Structure:**
```
synthase-config.json
.synthase-manifest.json
version.txt
Cargo.toml           (workspace root)
Cargo.lock
crates/
  core/
    Cargo.toml       ([package] name = "my-lib")
    src/lib.rs
  cli/
    Cargo.toml       ([package] name = "my-cli", depends on my-lib)
    src/main.rs
.github/workflows/release.yml
```

**`synthase-config.json`:**
```json
{
  "release-type": "rust",
  "draft": true,
  "bump-minor-pre-major": true,
  "extra-files": ["version.txt"],
  "packages": {
    ".": {
      "component": "my-lib",
      "package-name": "my-lib"
    }
  }
}
```

**`.synthase-manifest.json`:**
```json
{
  ".": "0.0.0"
}
```

**`version.txt`:**
```
0.0.0 # x-synthase-version
```

**`Cargo.toml`** (workspace root):
```toml
[workspace]
members = ["crates/*"]
resolver = "2"
```

**`crates/core/Cargo.toml`:**
```toml
[package]
name = "my-lib"
version = "0.0.0"
edition = "2021"
```

**`crates/core/src/lib.rs`:**
```rust
pub fn hello() -> &'static str {
    "hello"
}
```

**`crates/cli/Cargo.toml`:**
```toml
[package]
name = "my-cli"
version = "0.0.0"
edition = "2021"

[dependencies]
my-lib = { path = "../core", version = "0.0.0" }
```

**`crates/cli/src/main.rs`:**
```rust
fn main() {
    println!("{}", my_lib::hello());
}
```

**Workflow:** Same pattern as Test 1, but build step does `cargo build --release` and uploads `target/release/my-cli` as `my-cli-linux-x64`.

### Test Steps

#### Step 2.1: Initial setup
- Push repo
- **Expected:** Release PR for `0.1.0`
- **Validate:**
  - `crates/core/Cargo.toml` version = `"0.1.0"`
  - `crates/cli/Cargo.toml` version = `"0.1.0"`
  - Cargo.lock entries updated for both crates
  - `version.txt` updated to `0.1.0 # x-synthase-version`
  - CHANGELOG.md created

#### Step 2.2: Merge initial release
- **Validate:** Tag `my-lib-v0.1.0`, draft release created, artifact uploaded, release published, label lifecycle

#### Step 2.3: Feature + fix → PR update
- `feat: add parser` (add new function to lib.rs)
- Creates PR for `0.2.0`
- `fix: handle edge case` (fix in lib.rs)
- Updates same PR (still `0.2.0`)
- **Validate:** Same PR number, both entries in changelog

#### Step 2.4: Merge and validate second release
- **Validate:** Tag `my-lib-v0.2.0`, both Cargo.toml files at `0.2.0`, version.txt at `0.2.0`

#### Step 2.5: Breaking change with bump-minor-pre-major
- `feat!: redesign API`
- **Expected:** PR for `0.3.0` (not `1.0.0` because `bump-minor-pre-major: true` and version < 1.0.0)
- **Validate:** Version is `0.3.0`, breaking changes section present

---

## Test 3: Node Strategy

**Purpose:** Validate Node.js release flow with package.json and package-lock.json updates, including indent preservation.

### Repository Setup

**Repo name:** `kinyoklion-2/synthase-test-node`

**Files:**
```
synthase-config.json
.synthase-manifest.json
package.json
package-lock.json
index.js
.github/workflows/release.yml
```

**`synthase-config.json`:**
```json
{
  "release-type": "node",
  "draft": true,
  "packages": {
    ".": {}
  }
}
```

**`package.json`** (use 2-space indent):
```json
{
  "name": "synthase-test-node",
  "version": "0.0.0",
  "description": "Test repo for synthase node releases",
  "main": "index.js"
}
```

**`package-lock.json`:**
```json
{
  "name": "synthase-test-node",
  "version": "0.0.0",
  "lockfileVersion": 3,
  "packages": {
    "": {
      "name": "synthase-test-node",
      "version": "0.0.0"
    }
  }
}
```

**`index.js`:**
```js
module.exports = { greeting: "hello" };
```

**Workflow:** Same pattern. Build step creates a tarball (`tar czf synthase-test-node.tar.gz *.js package.json`).

### Test Steps

#### Step 3.1: Initial setup
- **Validate:** PR updates `package.json` version to `0.1.0`, `package-lock.json` root version and `packages[""].version` to `0.1.0`, indent preserved

#### Step 3.2: Merge and validate release
- **Validate:** Tag `v0.1.0` (no component prefix — root package with no component set), release with artifact

#### Step 3.3: Feature + fix → PR update
- `feat: add farewell` → PR for `0.2.0`
- `fix: fix greeting` → same PR updated
- **Validate:** PR update checklist

#### Step 3.4: Merge and validate second release
- **Validate:** Tag `v0.2.0`, package.json at `0.2.0`, CHANGELOG has both entries

---

## Test 4: Node Workspace (Monorepo)

**Purpose:** Validate monorepo with multiple Node packages, inter-package dependencies, and the node-workspace plugin for dependency cascading.

### Repository Setup

**Repo name:** `kinyoklion-2/synthase-test-node-workspace`

**Structure:**
```
synthase-config.json
.synthase-manifest.json
packages/
  core/
    package.json     (name: @test/core)
    index.js
  cli/
    package.json     (name: @test/cli, depends on @test/core)
    index.js
  utils/
    package.json     (name: @test/utils, no deps on other packages)
    index.js
.github/workflows/release.yml
```

**`synthase-config.json`:**
```json
{
  "release-type": "node",
  "draft": true,
  "plugins": ["node-workspace"],
  "packages": {
    "packages/core": {
      "component": "core"
    },
    "packages/cli": {
      "component": "cli"
    },
    "packages/utils": {
      "component": "utils"
    }
  }
}
```

**`.synthase-manifest.json`:**
```json
{
  "packages/core": "0.0.0",
  "packages/cli": "0.0.0",
  "packages/utils": "0.0.0"
}
```

**`packages/core/package.json`:**
```json
{
  "name": "@test/core",
  "version": "0.0.0"
}
```

**`packages/cli/package.json`:**
```json
{
  "name": "@test/cli",
  "version": "0.0.0",
  "dependencies": {
    "@test/core": "^0.0.0"
  }
}
```

**`packages/utils/package.json`:**
```json
{
  "name": "@test/utils",
  "version": "0.0.0"
}
```

**Workflow:** Same pattern but no build artifacts (simplify for monorepo testing).

### Test Steps

#### Step 4.1: Initial setup
- Push all files
- **Expected:** Single release PR with all three packages bumped to `0.1.0`
- **Validate:** All three `package.json` files updated, manifest has all three paths at `0.1.0`, PR body has collapsible details per component

#### Step 4.2: Merge and validate
- **Expected:** Three tags created: `core-v0.1.0`, `cli-v0.1.0`, `utils-v0.1.0`
- **Validate:** Three GitHub releases, all three PR labels updated

#### Step 4.3: Change only core package
- Push: `feat: add parser` touching only `packages/core/index.js`
- **Expected:**
  - core bumped to `0.2.0` (feat → minor)
  - cli cascade-bumped to `0.1.1` (depends on core) with `@test/core` dep updated to `^0.2.0`
  - utils unchanged (no dependency on core, no code changes)
- **Validate:** PR diffs show core and cli updated, utils absent from changes

#### Step 4.4: Merge and validate
- **Expected:** Tags `core-v0.2.0` and `cli-v0.1.1` created, no tag for utils
- **Validate:** Only two releases created, manifest updated for core and cli only

#### Step 4.5: Change only utils
- Push: `fix: utility fix` touching only `packages/utils/index.js`
- **Expected:** PR bumps utils to `0.1.1`, core and cli unaffected
- **Validate:** Only utils in PR changes

#### Step 4.6: Merge and validate
- **Validate:** Tag `utils-v0.1.1`, no tags for core or cli

---

## Test 5: Python Strategy

**Purpose:** Validate Python release with pyproject.toml updates.

### Repository Setup

**Repo name:** `kinyoklion-2/synthase-test-python`

**`synthase-config.json`:**
```json
{
  "release-type": "python",
  "draft": true,
  "packages": {
    ".": {
      "component": "my-python-pkg"
    }
  }
}
```

**`.synthase-manifest.json`:**
```json
{
  ".": "0.0.0"
}
```

**`pyproject.toml`:**
```toml
[project]
name = "my-python-pkg"
version = "0.0.0"
description = "Test Python package"
```

**`my_python_pkg/__init__.py`:**
```python
"""My Python package."""
```

**Workflow:** Same pattern. Build step creates a wheel (`python -m build` or just `tar czf`).

### Test Steps

#### Step 5.1: Initial setup → PR for `0.1.0`
- **Validate:** pyproject.toml version updated to `0.1.0`, CHANGELOG created

#### Step 5.2: Merge → release
- **Validate:** Tag `my-python-pkg-v0.1.0`, release with notes

#### Step 5.3: Feature → PR for `0.2.0` → merge → release
- **Validate:** pyproject.toml at `0.2.0`, tag `my-python-pkg-v0.2.0`

---

## Execution Order

| # | Repo | Strategy | Key Validations |
|---|------|----------|-----------------|
| 1 | `synthase-test-simple` | `simple` | Full lifecycle, artifacts, draft→publish, PR updates, breaking changes |
| 2 | `synthase-test-rust` | `rust` | Workspace Cargo.toml/lock, extra-files with x-synthase-version, bump-minor-pre-major |
| 3 | `synthase-test-node` | `node` | package.json/lock, indent preservation, root package (no component) |
| 4 | `synthase-test-node-workspace` | `node` + plugin | Monorepo, dependency cascade, selective releases, collapsible PR details |
| 5 | `synthase-test-python` | `python` | pyproject.toml updates |

## Validation Commands

```bash
# Check open PRs
gh pr list --repo kinyoklion-2/<repo> --state open

# Check PR details
gh pr view <number> --repo kinyoklion-2/<repo> --json title,labels,body

# Check PR diff
gh pr diff <number> --repo kinyoklion-2/<repo>

# Check releases
gh release list --repo kinyoklion-2/<repo>

# Check release details
gh release view <tag> --repo kinyoklion-2/<repo> --json isDraft,assets,tagName

# Check tags
git ls-remote --tags origin

# Check release assets
gh release view <tag> --repo kinyoklion-2/<repo> --json assets --jq '.assets[].name'

# Check PR labels
gh pr view <number> --repo kinyoklion-2/<repo> --json labels --jq '.labels[].name'
```

## Common Issues to Watch For

1. **Duplicate PRs:** If the merged-PR guard fails, a new PR is created instead of updating
2. **Missing artifacts:** If `releases_created` output is overwritten by release-pr.sh, build/publish jobs skip
3. **Wrong version:** If no tag exists, CLI may recompute the same version
4. **Draft tag missing:** GitHub doesn't create tags for draft releases; the action must create them explicitly
5. **Label not found:** Labels must be created before first use (action handles this)
6. **Cargo.lock conflicts:** Build step may modify Cargo.lock; working tree must be cleaned before branch checkout
7. **Version annotation not on same line:** `x-synthase-version` must be on the same line as the version string
8. **Node workspace cascade:** Verify dependency version prefixes (^, ~) are preserved during cascade updates
