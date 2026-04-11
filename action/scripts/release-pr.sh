#!/usr/bin/env bash
# release-pr.sh — Create or update a release PR from rustlease-please CLI output.
#
# Expected environment variables:
#   RUSTLEASE_BIN   — path to the rustlease-please binary
#   TARGET_BRANCH   — target branch for the release PR
#   GH_TOKEN        — GitHub token for API access
#   GITHUB_OUTPUT   — path to the GitHub Actions output file
set -euo pipefail

# ---------------------------------------------------------------------------
# 1. Run the CLI in dry-run mode to get the release plan
# ---------------------------------------------------------------------------
echo "::group::Running rustlease-please release-pr"
CLI_OUTPUT=$("$RUSTLEASE_BIN" \
  --repo-path . \
  --target-branch "$TARGET_BRANCH" \
  --dry-run \
  release-pr 2>/dev/null)
echo "$CLI_OUTPUT"
echo "::endgroup::"

# ---------------------------------------------------------------------------
# 2. Parse the CLI output
# ---------------------------------------------------------------------------
RELEASE_COUNT=$(echo "$CLI_OUTPUT" | jq '.releases | length')

if [ "$RELEASE_COUNT" -eq 0 ]; then
  echo "No releases to create."
  echo "releases_created=false" >> "$GITHUB_OUTPUT"
  echo "prs_created=false" >> "$GITHUB_OUTPUT"
  echo "releases=[]" >> "$GITHUB_OUTPUT"
  exit 0
fi

echo "Found $RELEASE_COUNT release(s) to create."

# Extract PR info
PR_TITLE=$(echo "$CLI_OUTPUT" | jq -r '.pull_requests[0].title')
PR_BODY=$(echo "$CLI_OUTPUT" | jq -r '.pull_requests[0].body')
PR_BRANCH=$(echo "$CLI_OUTPUT" | jq -r '.pull_requests[0].branch')

# Extract release info for outputs
FIRST_RELEASE=$(echo "$CLI_OUTPUT" | jq '.releases[0]')
TAG_NAME=$(echo "$FIRST_RELEASE" | jq -r '.tag')
VERSION=$(echo "$FIRST_RELEASE" | jq -r '.new_version')
MAJOR=$(echo "$VERSION" | cut -d. -f1)
MINOR=$(echo "$VERSION" | cut -d. -f2)
PATCH=$(echo "$VERSION" | cut -d. -f3 | cut -d- -f1)

# ---------------------------------------------------------------------------
# 3. Create or update the release branch with file changes
# ---------------------------------------------------------------------------
echo "::group::Applying file changes"

# Configure git
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

# Clean any working tree changes from the build step (e.g., Cargo.lock modified by cargo build)
git checkout -- . 2>/dev/null || true
git clean -fd 2>/dev/null || true

# Check if the release branch already exists
BRANCH_EXISTS=$(git ls-remote --heads origin "$PR_BRANCH" | wc -l)

if [ "$BRANCH_EXISTS" -gt 0 ]; then
  echo "Release branch $PR_BRANCH already exists, updating..."
  git fetch origin "$PR_BRANCH"
  git checkout "$PR_BRANCH"
  git reset --hard "origin/$TARGET_BRANCH"
else
  echo "Creating release branch $PR_BRANCH..."
  git checkout -b "$PR_BRANCH"
fi

# Apply file changes from CLI output
echo "$CLI_OUTPUT" | jq -r '.pull_requests[0].files[] | @base64' | while read -r file_b64; do
  FILE_PATH=$(echo "$file_b64" | base64 -d | jq -r '.path')
  FILE_CONTENT=$(echo "$file_b64" | base64 -d | jq -r '.content')
  CREATE_IF_MISSING=$(echo "$file_b64" | base64 -d | jq -r '.create_if_missing')

  if [ -f "$FILE_PATH" ] || [ "$CREATE_IF_MISSING" = "true" ]; then
    mkdir -p "$(dirname "$FILE_PATH")"
    echo "$FILE_CONTENT" > "$FILE_PATH"
    echo "  Updated: $FILE_PATH"
  fi
done

# Commit and push
git add -A
if git diff --cached --quiet; then
  echo "No file changes to commit."
else
  git commit -m "$PR_TITLE"
  git push origin "$PR_BRANCH" --force
  echo "Pushed changes to $PR_BRANCH"
fi

echo "::endgroup::"

# ---------------------------------------------------------------------------
# 4. Create or update the PR
# ---------------------------------------------------------------------------
# Matching release-please behavior:
# - If an OPEN PR exists for this branch → always UPDATE it
# - If no open PR exists, check for MERGED PRs with 'autorelease: pending'
#   → if any exist, skip creation (the 'release' command should run first)
# - Otherwise → CREATE a new PR
echo "::group::Managing release PR"

# Ensure required labels exist (create if missing)
for LABEL in "autorelease: pending" "autorelease: tagged"; do
  gh label create "$LABEL" --color "ededed" --force 2>/dev/null || true
done

# Check for existing open PR on this branch
EXISTING_PR=$(gh pr list \
  --head "$PR_BRANCH" \
  --base "$TARGET_BRANCH" \
  --state open \
  --json number \
  --jq '.[0].number // empty' \
  2>/dev/null || true)

if [ -n "$EXISTING_PR" ]; then
  # Update the existing open PR
  echo "Updating existing PR #$EXISTING_PR..."
  gh pr edit "$EXISTING_PR" \
    --title "$PR_TITLE" \
    --body "$PR_BODY"
  PR_NUMBER="$EXISTING_PR"
  echo "Updated PR #$PR_NUMBER"
else
  # No open PR — check if we should create one, or if a merged PR is pending release
  PENDING_MERGED=$(gh pr list \
    --base "$TARGET_BRANCH" \
    --state merged \
    --label "autorelease: pending" \
    --json number \
    --jq 'length' \
    2>/dev/null || echo "0")

  if [ "$PENDING_MERGED" -gt 0 ]; then
    echo "Found $PENDING_MERGED merged release PR(s) with 'autorelease: pending' label."
    echo "Skipping PR creation — run the 'release' command first to tag these releases."
    echo "releases_created=false" >> "$GITHUB_OUTPUT"
    echo "prs_created=false" >> "$GITHUB_OUTPUT"
    echo "releases=[]" >> "$GITHUB_OUTPUT"
    echo "::endgroup::"
    git checkout "$TARGET_BRANCH" 2>/dev/null || git checkout - 2>/dev/null || true
    exit 0
  fi

  echo "Creating new release PR..."
  PR_URL=$(gh pr create \
    --head "$PR_BRANCH" \
    --base "$TARGET_BRANCH" \
    --title "$PR_TITLE" \
    --body "$PR_BODY" \
    --label "autorelease: pending")
  PR_NUMBER=$(echo "$PR_URL" | grep -oE '[0-9]+$')
  echo "Created PR #$PR_NUMBER: $PR_URL"
fi

echo "::endgroup::"

# ---------------------------------------------------------------------------
# 5. Switch back to original branch
# ---------------------------------------------------------------------------
git checkout "$TARGET_BRANCH" 2>/dev/null || git checkout - 2>/dev/null || true

# ---------------------------------------------------------------------------
# 6. Set outputs
# ---------------------------------------------------------------------------
echo "releases_created=false" >> "$GITHUB_OUTPUT"
echo "prs_created=true" >> "$GITHUB_OUTPUT"
echo "pr_number=$PR_NUMBER" >> "$GITHUB_OUTPUT"
echo "tag_name=$TAG_NAME" >> "$GITHUB_OUTPUT"
echo "version=$VERSION" >> "$GITHUB_OUTPUT"
echo "major=$MAJOR" >> "$GITHUB_OUTPUT"
echo "minor=$MINOR" >> "$GITHUB_OUTPUT"
echo "patch=$PATCH" >> "$GITHUB_OUTPUT"

# Multi-line JSON output
{
  echo "releases<<RELEASES_EOF"
  echo "$CLI_OUTPUT" | jq -c '.releases'
  echo "RELEASES_EOF"
} >> "$GITHUB_OUTPUT"

echo "Release PR processing complete."
