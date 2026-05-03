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

# ---------------------------------------------------------------------------
# Helper: build tag name from component, version, and config
# ---------------------------------------------------------------------------
build_tag() {
  local component="$1"
  local version="$2"
  local tag
  if [ -n "$component" ]; then
    tag="${component}-v${version}"
  else
    tag="v${version}"
  fi
  if [ -f "$CONFIG_FILE" ]; then
    local include_v tag_sep
    include_v=$(jq -r '."include-v-in-tag" // true' "$CONFIG_FILE")
    tag_sep=$(jq -r '."tag-separator" // "-"' "$CONFIG_FILE")
    if [ "$include_v" = "false" ]; then
      if [ -n "$component" ]; then
        tag="${component}${tag_sep}${version}"
      else
        tag="${version}"
      fi
    fi
  fi
  echo "$tag"
}

# ---------------------------------------------------------------------------
# Helper: create one GitHub release for a component+version
# ---------------------------------------------------------------------------
create_one_release() {
  local component="$1"
  local version="$2"
  local notes="$3"
  local merge_sha="$4"

  local tag
  tag=$(build_tag "$component" "$version")

  local release_name
  if [ -n "$component" ]; then
    release_name="$component $version"
  else
    release_name="$tag"
  fi

  echo "  Component: ${component:-<none>}"
  echo "  Version: $version"
  echo "  Tag: $tag"
  echo "  Creating release: $release_name"

  local gh_flags=("$tag" --title "$release_name" --notes "$notes" --target "$TARGET_BRANCH")
  [ "$IS_DRAFT" = "true" ] && gh_flags+=(--draft)
  [ "$IS_PRERELEASE" = "true" ] && gh_flags+=(--prerelease)

  gh release create "${gh_flags[@]}" || {
    echo "::warning::Failed to create release for $tag (may already exist)"
    return
  }

  # For draft releases GitHub doesn't create git tags automatically; create explicitly.
  if [ "$IS_DRAFT" = "true" ] && [ -n "$merge_sha" ]; then
    gh api "repos/{owner}/{repo}/git/refs" \
      -f ref="refs/tags/$tag" \
      -f sha="$merge_sha" 2>/dev/null || echo "::warning::Tag $tag may already exist"
    echo "  Created git tag $tag at $merge_sha (for draft release)"
  fi

  local release_url
  release_url=$(gh release view "$tag" --json htmlUrl --jq '.htmlUrl' 2>/dev/null || echo "")
  echo "  Created: $release_url"
}

echo "$MERGED_PRS" | jq -c '.[]' | while read -r pr; do
  PR_NUM=$(echo "$pr" | jq -r '.number')
  PR_TITLE=$(echo "$pr" | jq -r '.title')
  PR_BODY=$(echo "$pr" | jq -r '.body')
  MERGE_SHA=$(echo "$pr" | jq -r '.mergeCommit.oid')

  echo "Processing PR #$PR_NUM: $PR_TITLE"

  # Determine single-package vs multi-package PR.
  # Single-package titles: "chore(main): release [component] X.Y.Z"
  # Multi-package titles:  "chore: release main" (no semver in title)
  RELEASE_PART=$(echo "$PR_TITLE" | sed -n 's/.*release[[:space:]]*//p')

  # Extract candidate version (last word of RELEASE_PART for single-pkg)
  WORD_COUNT=$(echo "$RELEASE_PART" | wc -w)
  if [ "$WORD_COUNT" -ge 2 ]; then
    CANDIDATE_VERSION=$(echo "$RELEASE_PART" | awk '{print $NF}')
  else
    CANDIDATE_VERSION="$RELEASE_PART"
  fi

  # If the candidate doesn't look like a semver, treat as multi-package PR
  if ! echo "$CANDIDATE_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]'; then
    echo "  Multi-package PR detected, parsing components from body..."

    # Extract "COMPONENT: VERSION" pairs from <details><summary> tags
    SUMMARIES=$(echo "$PR_BODY" | grep -o '<details><summary>[^<]*</summary>' \
      | sed 's|<details><summary>||;s|</summary>||')

    if [ -z "$SUMMARIES" ]; then
      echo "::warning::No <details> blocks found in multi-package PR body, skipping"
      continue
    fi

    echo "$SUMMARIES" | while IFS= read -r summary; do
      COMP=$(echo "$summary" | awk -F': ' '{print $1}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
      VER=$(echo "$summary"  | awk -F': ' '{print $2}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
      [ -z "$COMP" ] || [ -z "$VER" ] && continue

      # Extract notes: content inside the matching <details> block
      COMP_NOTES=$(echo "$PR_BODY" | awk \
        "/<details><summary>${COMP}: ${VER}<\\/summary>/{found=1; next} found && /<\\/details>/{found=0; next} found{print}")
      [ -z "$COMP_NOTES" ] && COMP_NOTES="Release $VER"

      create_one_release "$COMP" "$VER" "$COMP_NOTES" "$MERGE_SHA"
    done

  else
    # Single-package PR
    if [ "$WORD_COUNT" -ge 2 ]; then
      COMPONENT=$(echo "$RELEASE_PART" | awk '{print $1}')
      VERSION=$(echo "$RELEASE_PART" | awk '{print $2}')
    else
      COMPONENT=""
      VERSION="$RELEASE_PART"
    fi

    # Extract release notes (between the two --- markers in the body)
    NOTES=$(echo "$PR_BODY" | awk '/^---$/{n++; next} n==1' | sed '/^$/N;/^\n$/d')
    [ -z "$NOTES" ] && NOTES="Release $VERSION"

    create_one_release "$COMPONENT" "$VERSION" "$NOTES" "$MERGE_SHA"
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
FIRST_BODY=$(echo "$FIRST_PR" | jq -r '.body')
RELEASE_PART=$(echo "$FIRST_TITLE" | sed -n 's/.*release[[:space:]]*//p')
WORD_COUNT=$(echo "$RELEASE_PART" | wc -w)
if [ "$WORD_COUNT" -ge 2 ]; then
  CANDIDATE_VERSION=$(echo "$RELEASE_PART" | awk '{print $NF}')
else
  CANDIDATE_VERSION="$RELEASE_PART"
fi

if ! echo "$CANDIDATE_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]'; then
  # Multi-package: build all tag names (space-separated) from <details> blocks
  ALL_TAGS=""
  while IFS= read -r summary; do
    COMP=$(echo "$summary" | awk -F': ' '{print $1}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    VER=$(echo "$summary"  | awk -F': ' '{print $2}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    [ -z "$COMP" ] || [ -z "$VER" ] && continue
    t=$(build_tag "$COMP" "$VER")
    ALL_TAGS="${ALL_TAGS} ${t}"
  done < <(echo "$FIRST_BODY" | grep -o '<details><summary>[^<]*</summary>' \
    | sed 's|<details><summary>||;s|</summary>||')
  TAG_NAME=$(echo "$ALL_TAGS" | xargs)

  # Use first component for scalar outputs (version, major, minor, patch)
  FIRST_SUMMARY=$(echo "$FIRST_BODY" | grep -o '<details><summary>[^<]*</summary>' \
    | head -1 | sed 's|<details><summary>||;s|</summary>||')
  COMPONENT=$(echo "$FIRST_SUMMARY" | awk -F': ' '{print $1}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
  VERSION=$(echo "$FIRST_SUMMARY"  | awk -F': ' '{print $2}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
else
  if [ "$WORD_COUNT" -ge 2 ]; then
    COMPONENT=$(echo "$RELEASE_PART" | awk '{print $1}')
    VERSION=$(echo "$RELEASE_PART" | awk '{print $2}')
  else
    COMPONENT=""
    VERSION="$RELEASE_PART"
  fi
  TAG_NAME=$(build_tag "$COMPONENT" "$VERSION")
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

# Get release URLs (use first tag for upload_url/html_url outputs)
FIRST_TAG_NAME=$(echo "$TAG_NAME" | awk '{print $1}')
UPLOAD_URL=$(gh release view "$FIRST_TAG_NAME" --json uploadUrl --jq '.uploadUrl' 2>/dev/null || echo "")
HTML_URL=$(gh release view "$FIRST_TAG_NAME" --json htmlUrl --jq '.htmlUrl' 2>/dev/null || echo "")
echo "upload_url=$UPLOAD_URL" >> "$GITHUB_OUTPUT"
echo "html_url=$HTML_URL" >> "$GITHUB_OUTPUT"

echo "Release creation complete."
