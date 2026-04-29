#!/usr/bin/env bash
# create-releases.sh — Create GitHub releases from merged release PRs.
#
# Unlike calling the CLI to recompute versions (which can produce incorrect
# results when tags don't exist yet), this script extracts version and release
# notes directly from the merged PR — the PR is the source of truth.
#
# This matches release-please's approach: createReleases() finds merged PRs
# and extracts version from the PR title, notes from the PR body.
#
# Expected environment variables:
#   TARGET_BRANCH   — target branch
#   GH_TOKEN        — GitHub token for API access
#   GITHUB_OUTPUT   — path to the GitHub Actions output file
set -euo pipefail

# ---------------------------------------------------------------------------
# 1. Find merged release PRs
# ---------------------------------------------------------------------------
echo "::group::Looking for merged release PRs"

MERGED_PRS=$(gh pr list \
  --base "$TARGET_BRANCH" \
  --state merged \
  --label "autorelease: pending" \
  --json number,title,body,mergeCommit,headRefName \
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
# 2. Extract release info from each merged PR
# ---------------------------------------------------------------------------
echo "::group::Creating GitHub releases"

# Load config to get draft/prerelease settings
CONFIG_FILE="synthase-config.json"
if [ ! -f "$CONFIG_FILE" ]; then
  CONFIG_FILE="release-please-config.json"
fi
IS_DRAFT="false"
IS_PRERELEASE="false"
if [ -f "$CONFIG_FILE" ]; then
  IS_DRAFT=$(jq -r '.draft // false' "$CONFIG_FILE")
  IS_PRERELEASE=$(jq -r '.prerelease // false' "$CONFIG_FILE")
fi

FIRST_TAG=""
FIRST_VERSION=""
ALL_RELEASES="[]"
ALL_PATHS="[]"

echo "$MERGED_PRS" | jq -c '.[]' | while read -r pr; do
  PR_NUM=$(echo "$pr" | jq -r '.number')
  PR_TITLE=$(echo "$pr" | jq -r '.title')
  PR_BODY=$(echo "$pr" | jq -r '.body')
  MERGE_SHA=$(echo "$pr" | jq -r '.mergeCommit.oid')

  echo "Processing PR #$PR_NUM: $PR_TITLE"

  # Parse version from PR title
  # Formats: "chore(main): release <component> <version>" or "chore(main): release <version>"
  # Extract everything after "release " — could be "component version" or just "version"
  RELEASE_PART=$(echo "$PR_TITLE" | sed -n 's/.*release[[:space:]]*//p')

  if [ -z "$RELEASE_PART" ]; then
    echo "::warning::Could not parse release info from PR title: $PR_TITLE"
    continue
  fi

  # Check if it's "component version" or just "version"
  WORD_COUNT=$(echo "$RELEASE_PART" | wc -w)
  if [ "$WORD_COUNT" -ge 2 ]; then
    COMPONENT=$(echo "$RELEASE_PART" | awk '{print $1}')
    VERSION=$(echo "$RELEASE_PART" | awk '{print $2}')
  else
    COMPONENT=""
    VERSION="$RELEASE_PART"
  fi

  echo "  Component: ${COMPONENT:-<none>}"
  echo "  Version: $VERSION"

  # Build tag name from component + version
  # Match the tag format from the config
  if [ -n "$COMPONENT" ]; then
    TAG="${COMPONENT}-v${VERSION}"
  else
    TAG="v${VERSION}"
  fi

  # Check config for tag format overrides
  if [ -f "$CONFIG_FILE" ]; then
    INCLUDE_V=$(jq -r '."include-v-in-tag" // true' "$CONFIG_FILE")
    TAG_SEP=$(jq -r '."tag-separator" // "-"' "$CONFIG_FILE")
    if [ "$INCLUDE_V" = "false" ]; then
      if [ -n "$COMPONENT" ]; then
        TAG="${COMPONENT}${TAG_SEP}${VERSION}"
      else
        TAG="${VERSION}"
      fi
    fi
  fi

  # Extract release notes from PR body
  # The body has format: header\n---\n<changelog>\n---\nfooter
  # For single-component releases, the changelog is between the two --- markers
  # For multi-component, it's in <details> blocks
  NOTES=$(echo "$PR_BODY" | awk '/^---$/{n++; next} n==1' | sed '/^$/N;/^\n$/d')

  if [ -z "$NOTES" ]; then
    NOTES="Release $VERSION"
  fi

  RELEASE_NAME="$TAG"
  if [ -n "$COMPONENT" ]; then
    RELEASE_NAME="$COMPONENT $VERSION"
  fi

  echo "  Tag: $TAG"
  echo "  Creating release: $RELEASE_NAME"

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

  # For draft releases, GitHub does not create a git tag until the release
  # is published. We must create the tag explicitly so the CLI can find the
  # release boundary on subsequent runs.
  if [ "$IS_DRAFT" = "true" ] && [ -n "$MERGE_SHA" ]; then
    gh api "repos/{owner}/{repo}/git/refs" \
      -f ref="refs/tags/$TAG" \
      -f sha="$MERGE_SHA" 2>/dev/null || echo "::warning::Tag $TAG may already exist"
    echo "  Created git tag $TAG at $MERGE_SHA (for draft release)"
  fi

  RELEASE_URL=$(gh release view "$TAG" --json htmlUrl --jq '.htmlUrl' 2>/dev/null || echo "")
  echo "  Created: $RELEASE_URL"

  # Track first release for outputs
  if [ -z "$FIRST_TAG" ]; then
    FIRST_TAG="$TAG"
    FIRST_VERSION="$VERSION"
  fi
done

echo "::endgroup::"

# ---------------------------------------------------------------------------
# 3. Update PR labels (pending → tagged)
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
# 4. Set outputs
# ---------------------------------------------------------------------------

# Re-extract first release info (variables from while loop are in subshell)
FIRST_PR=$(echo "$MERGED_PRS" | jq -c '.[0]')
FIRST_TITLE=$(echo "$FIRST_PR" | jq -r '.title')
RELEASE_PART=$(echo "$FIRST_TITLE" | sed -n 's/.*release[[:space:]]*//p')
WORD_COUNT=$(echo "$RELEASE_PART" | wc -w)
if [ "$WORD_COUNT" -ge 2 ]; then
  COMPONENT=$(echo "$RELEASE_PART" | awk '{print $1}')
  VERSION=$(echo "$RELEASE_PART" | awk '{print $2}')
else
  COMPONENT=""
  VERSION="$RELEASE_PART"
fi

if [ -n "$COMPONENT" ]; then
  TAG_NAME="${COMPONENT}-v${VERSION}"
else
  TAG_NAME="v${VERSION}"
fi

# Re-check config for tag format
if [ -f "$CONFIG_FILE" ]; then
  INCLUDE_V=$(jq -r '."include-v-in-tag" // true' "$CONFIG_FILE")
  TAG_SEP=$(jq -r '."tag-separator" // "-"' "$CONFIG_FILE")
  if [ "$INCLUDE_V" = "false" ]; then
    if [ -n "$COMPONENT" ]; then
      TAG_NAME="${COMPONENT}${TAG_SEP}${VERSION}"
    else
      TAG_NAME="${VERSION}"
    fi
  fi
fi

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

echo "Release creation complete."
