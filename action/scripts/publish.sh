#!/usr/bin/env bash
# publish.sh — Take draft GitHub releases out of draft.
#
# Expected environment variables:
#   GH_TOKEN        — GitHub token for API access
#   TAG_NAME        — tag of the release to publish (optional; auto-detects if empty)
#   GITHUB_OUTPUT   — path to the GitHub Actions output file
set -euo pipefail

echo "::group::Publishing releases"

if [ -n "${TAG_NAME:-}" ]; then
  # Publish a specific release by tag
  echo "Publishing release: $TAG_NAME"
  gh release edit "$TAG_NAME" --draft=false
  echo "Published $TAG_NAME"
else
  # Find and publish all draft releases created by this action
  echo "No tag specified — finding draft releases to publish..."
  DRAFTS=$(gh release list --json tagName,isDraft --jq '[.[] | select(.isDraft)] | .[].tagName' 2>/dev/null || true)

  if [ -z "$DRAFTS" ]; then
    echo "No draft releases found."
    echo "::endgroup::"
    exit 0
  fi

  echo "$DRAFTS" | while read -r tag; do
    echo "Publishing draft release: $tag"
    gh release edit "$tag" --draft=false
    echo "Published $tag"
  done
fi

echo "::endgroup::"
