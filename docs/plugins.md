# Plugins

Plugins post-process releases to handle cross-package concerns like workspace dependency cascading and version linking. They run after individual package releases are computed.

Configure plugins in `release-please-config.json`:

```json
{
  "plugins": [
    "cargo-workspace",
    { "type": "linked-versions", "groupName": "core", "components": ["a", "b"] }
  ]
}
```

## `cargo-workspace`

Coordinates version bumps across Rust workspace members. When a crate is bumped, any workspace crate that depends on it gets a cascading patch bump with updated dependency version references.

**Config:** String `"cargo-workspace"` or object `{ "type": "cargo-workspace", "merge": true }`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `merge` | boolean | `true` | Merge workspace releases into a single PR |

**What it does:**
1. Parses `Cargo.toml` workspace members (including glob patterns)
2. Builds a dependency graph of workspace-internal dependencies (path-based deps only)
3. When crate B is bumped, any crate A that depends on B gets a cascade patch bump
4. Updates `[dependencies] crate-b = { version = "new", path = "..." }` references
5. Uses topological sort to process dependencies in correct order

**Example:**
```
Workspace: crate-a depends on crate-b
Commit bumps crate-b (feat → minor bump)
Result: crate-b 1.0.0 → 1.1.0, crate-a 1.0.0 → 1.0.1 (cascade patch)
crate-a's Cargo.toml dependency on crate-b updated to 1.1.0
```

## `node-workspace`

Coordinates version bumps across npm workspace packages. Similar to `cargo-workspace` but for Node.js monorepos.

**Config:** String `"node-workspace"` or object `{ "type": "node-workspace", "merge": true }`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `merge` | boolean | `true` | Merge workspace releases into a single PR |
| `updatePeerDependencies` | boolean | `false` | Also update peer dependency versions |

**What it does:**
1. Parses `package.json` files for workspace packages listed in the config
2. Identifies inter-package dependencies (`dependencies`, `devDependencies`, `optionalDependencies`)
3. Cascades patch bumps through the dependency graph
4. Preserves version range prefixes (`^`, `~`, `>=`, etc.) when updating

## `linked-versions`

Groups components so they always share the same version. The highest version among the group is used for all components.

**Config:** Object only (requires `groupName` and `components`):

```json
{
  "type": "linked-versions",
  "groupName": "core",
  "components": ["crate-a", "crate-b"],
  "merge": true
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `groupName` | string | (required) | Name of the linked group |
| `components` | array | (required) | Component names to link |
| `merge` | boolean | `true` | Merge linked releases into a single PR |

**What it does:**
1. Finds all releases matching the group's component list
2. Determines the highest version among them
3. Upgrades all group members to that version

**Example:**
```
Group "core": components ["a", "b"]
a has feat commit → 1.1.0 (minor bump)
b has fix commit → 1.0.1 (patch bump)
Result: both a and b release as 1.1.0 (highest wins)
```

## `sentence-case`

Normalizes commit subjects in the changelog to sentence case (first letter uppercase).

**Config:** String `"sentence-case"` or object:

```json
{
  "type": "sentence-case",
  "specialWords": ["gRPC", "OAuth", "API"]
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `specialWords` | array | `[]` | Words to preserve as-is (not sentence-cased) |

**Example:**
```
Commit: "feat: add new feature"
Changelog: "* Add new feature" (first letter capitalized)

Commit: "feat: gRPC support" (with specialWords: ["gRPC"])
Changelog: "* gRPC support" (preserved)
```
