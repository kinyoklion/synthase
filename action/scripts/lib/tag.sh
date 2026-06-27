#!/usr/bin/env bash
# tag.sh — tag formatting shared by the action scripts.
#
# This is sourced, not executed. It defines build_tag().

# Build a release tag from a component + version, honoring the config's
# tag-format flags so the tags written by create-releases.sh match the tags
# the CLI looks for when locating the release boundary on subsequent runs:
#
#   <component><separator><v?><version>     (component dropped when opted out)
#
# Defaults match the CLI (crates/core/src/config.rs): include-component-in-tag
# and include-v-in-tag both default to true, tag-separator defaults to "-".
#
# Reads the config path from $CONFIG_FILE (optional; falls back to defaults
# when unset or missing).
build_tag() {
  local component="$1" version="$2"
  local include_component="true" include_v="true" tag_sep="-"
  if [ -n "${CONFIG_FILE:-}" ] && [ -f "$CONFIG_FILE" ]; then
    # A missing key yields the string "null". We deliberately do NOT use jq's
    # `// default` operator: it treats an explicit `false` as empty and would
    # clobber it with the default, silently re-enabling an opted-out flag.
    include_component=$(jq -r '."include-component-in-tag"' "$CONFIG_FILE")
    [ "$include_component" = "null" ] && include_component="true"
    include_v=$(jq -r '."include-v-in-tag"' "$CONFIG_FILE")
    [ "$include_v" = "null" ] && include_v="true"
    tag_sep=$(jq -r '."tag-separator"' "$CONFIG_FILE")
    [ "$tag_sep" = "null" ] && tag_sep="-"
  fi
  if [ "$include_component" = "false" ]; then
    component=""
  fi
  local v_prefix="v"
  [ "$include_v" = "false" ] && v_prefix=""
  if [ -n "$component" ]; then
    printf '%s%s%s%s' "$component" "$tag_sep" "$v_prefix" "$version"
  else
    printf '%s%s' "$v_prefix" "$version"
  fi
}
