#!/usr/bin/env bash
# tag_test.sh — self-contained tests for build_tag (no test framework needed).
#
# Usage: bash action/scripts/lib/tag_test.sh
# Requires: jq
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/tag.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FAILED=0

# assert <description> <config-json> <component> <version> <expected-tag>
assert() {
  local desc="$1" cfg="$2" comp="$3" ver="$4" expect="$5"
  if [ -n "$cfg" ]; then
    CONFIG_FILE="$TMP/cfg.json"
    printf '%s' "$cfg" > "$CONFIG_FILE"
  else
    CONFIG_FILE="$TMP/missing.json"  # nonexistent → defaults
  fi
  local got
  got="$(build_tag "$comp" "$ver")"
  if [ "$got" = "$expect" ]; then
    echo "ok   - $desc ($got)"
  else
    echo "FAIL - $desc: got '$got', expected '$expect'"
    FAILED=1
  fi
}

# Regression for kinyoklion/wrk#44: tags carry the component prefix, so the
# config must opt in for create-releases.sh to write matching tags.
assert "include-component-in-tag:true keeps prefix" '{"include-component-in-tag":true}' wrk 0.1.9 "wrk-v0.1.9"

# Explicit false must be honored (the jq `// true` bug treated false as unset).
assert "include-component-in-tag:false drops prefix" '{"include-component-in-tag":false}' wrk 0.1.9 "v0.1.9"

# Defaults match the CLI: component & v both default to true.
assert "default keeps component"        '{}' wrk 0.1.9 "wrk-v0.1.9"
assert "no config file → defaults"      ''   wrk 0.1.9 "wrk-v0.1.9"
assert "no component word"              '{}' ''  2.0.0 "v2.0.0"

# Other tag-format flags still work and compose with component handling.
assert "include-v-in-tag:false"                    '{"include-v-in-tag":false}' wrk 0.1.9 "wrk-0.1.9"
assert "include-v:false + component:false"         '{"include-v-in-tag":false,"include-component-in-tag":false}' wrk 0.1.9 "0.1.9"
assert "custom tag-separator"                      '{"tag-separator":"/"}' my-lib 1.0.0 "my-lib/v1.0.0"

if [ "$FAILED" -ne 0 ]; then
  echo "TESTS FAILED"
  exit 1
fi
echo "All tag tests passed."
