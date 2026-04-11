#!/usr/bin/env bash
# release.sh — Create GitHub releases from merged release PRs.
#
# Expected environment variables:
#   RUSTLEASE_BIN   — path to the rustlease-please binary
#   TARGET_BRANCH   — target branch
#   GH_TOKEN        — GitHub token for API access
#   GITHUB_OUTPUT   — path to the GitHub Actions output file
set -euo pipefail

# ---------------------------------------------------------------------------
# 1. Find merged release PRs
# ---------------------------------------------------------------------------
echo "::group::Looking for merged release PRs"

# Look for recently merged PRs with the release label
MERGED_PRS=$(gh pr list \
  --base "$TARGET_BRANCH" \
  --state merged \
  --label "autorelease: pending" \
  --json number,title,mergeCommit,headRefName \
  --jq '.' \
  2>/dev/null || echo "[]")

MERGED_COUNT=$(echo "$MERGED_PRS" | jq 'length')
echo "Found $MERGED_COUNT merged release PR(s) with 'autorelease: pending' label."

if [ "$MERGED_COUNT" -eq 0 ]; then
  echo "No merged release PRs found."
  echo "releases_created=false" >> "$GITHUB_OUTPUT"
  echo "releases=[]" >> "$GITHUB_OUTPUT"
  echo "::endgroup::"
  exit 0
fi

echo "::endgroup::"

# ---------------------------------------------------------------------------
# 2. Run CLI to get release info
# ---------------------------------------------------------------------------
echo "::group::Computing release information"

CLI_OUTPUT=$("$RUSTLEASE_BIN" \
  --repo-path . \
  --target-branch "$TARGET_BRANCH" \
  release 2>/dev/null)
echo "$CLI_OUTPUT"

RELEASE_COUNT=$(echo "$CLI_OUTPUT" | jq '.releases | length')

if [ "$RELEASE_COUNT" -eq 0 ]; then
  echo "No releases to create."
  echo "releases_created=false" >> "$GITHUB_OUTPUT"
  echo "releases=[]" >> "$GITHUB_OUTPUT"
  echo "::endgroup::"
  exit 0
fi

echo "::endgroup::"

# ---------------------------------------------------------------------------
# 3. Create GitHub releases for each component
# ---------------------------------------------------------------------------
echo "::group::Creating GitHub releases"

CREATED_RELEASES="[]"

echo "$CLI_OUTPUT" | jq -c '.releases[]' | while read -r release; do
  TAG=$(echo "$release" | jq -r '.tag')
  VERSION=$(echo "$release" | jq -r '.version')
  COMPONENT=$(echo "$release" | jq -r '.component // empty')
  NOTES=$(echo "$release" | jq -r '.release_notes')
  IS_DRAFT=$(echo "$release" | jq -r '.draft')
  IS_PRERELEASE=$(echo "$release" | jq -r '.prerelease')
  SKIP=$(echo "$release" | jq -r '.skip_github_release')

  if [ "$SKIP" = "true" ]; then
    echo "Skipping GitHub release for $TAG (skip_github_release=true)"
    continue
  fi

  RELEASE_NAME="$TAG"
  if [ -n "$COMPONENT" ]; then
    RELEASE_NAME="$COMPONENT $VERSION"
  fi

  echo "Creating release: $TAG ($RELEASE_NAME)"

  # Build gh release create flags
  GH_FLAGS=("$TAG" --title "$RELEASE_NAME" --notes "$NOTES" --target "$TARGET_BRANCH")
  if [ "$IS_DRAFT" = "true" ]; then
    GH_FLAGS+=(--draft)
  fi
  if [ "$IS_PRERELEASE" = "true" ]; then
    GH_FLAGS+=(--prerelease)
  fi

  gh release create "${GH_FLAGS[@]}" || {
    echo "::warning::Failed to create release for $TAG (may already exist)"
    continue
  }

  RELEASE_URL=$(gh release view "$TAG" --json htmlUrl --jq '.htmlUrl' 2>/dev/null || echo "")
  echo "  Created: $RELEASE_URL"
done

echo "::endgroup::"

# ---------------------------------------------------------------------------
# 4. Update PR labels (pending → tagged)
# ---------------------------------------------------------------------------
echo "::group::Updating PR labels"

echo "$MERGED_PRS" | jq -r '.[].number' | while read -r pr_num; do
  echo "Updating labels on PR #$pr_num..."
  gh pr edit "$pr_num" \
    --remove-label "autorelease: pending" \
    --add-label "autorelease: tagged" \
    2>/dev/null || echo "::warning::Failed to update labels on PR #$pr_num"
done

echo "::endgroup::"

# ---------------------------------------------------------------------------
# 5. Set outputs
# ---------------------------------------------------------------------------
FIRST_RELEASE=$(echo "$CLI_OUTPUT" | jq '.releases[0]')
TAG_NAME=$(echo "$FIRST_RELEASE" | jq -r '.tag')
VERSION=$(echo "$FIRST_RELEASE" | jq -r '.version')
MAJOR=$(echo "$VERSION" | cut -d. -f1)
MINOR=$(echo "$VERSION" | cut -d. -f2)
PATCH=$(echo "$VERSION" | cut -d. -f3 | cut -d- -f1)

echo "releases_created=true" >> "$GITHUB_OUTPUT"
echo "release_created=true" >> "$GITHUB_OUTPUT"
echo "tag_name=$TAG_NAME" >> "$GITHUB_OUTPUT"
echo "version=$VERSION" >> "$GITHUB_OUTPUT"
echo "major=$MAJOR" >> "$GITHUB_OUTPUT"
echo "minor=$MINOR" >> "$GITHUB_OUTPUT"
echo "patch=$PATCH" >> "$GITHUB_OUTPUT"

# Get release URLs
UPLOAD_URL=$(gh release view "$TAG_NAME" --json uploadUrl --jq '.uploadUrl' 2>/dev/null || echo "")
HTML_URL=$(gh release view "$TAG_NAME" --json htmlUrl --jq '.htmlUrl' 2>/dev/null || echo "")
echo "upload_url=$UPLOAD_URL" >> "$GITHUB_OUTPUT"
echo "html_url=$HTML_URL" >> "$GITHUB_OUTPUT"

# Paths released (for monorepo matrix builds)
{
  echo "paths_released<<PATHS_EOF"
  echo "$CLI_OUTPUT" | jq -c '[.releases[].path]'
  echo "PATHS_EOF"
} >> "$GITHUB_OUTPUT"

{
  echo "releases<<RELEASES_EOF"
  echo "$CLI_OUTPUT" | jq -c '.releases'
  echo "RELEASES_EOF"
} >> "$GITHUB_OUTPUT"

echo "Release creation complete."
