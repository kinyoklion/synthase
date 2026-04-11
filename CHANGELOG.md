# Changelog

## [0.6.0](https://github.com/kinyoklion/rustlease-please/compare/rustlease-please-v0.5.0...rustlease-please-v0.6.0) (2026-04-11)

### Features

* unify action into single-workflow with immutable release support

### Bug Fixes

* create git tag for draft releases
* move x-release-please-version annotation to same line as version
* release-pr.sh must not overwrite releases_created output

## [0.6.0](https://github.com/kinyoklion/rustlease-please/compare/rustlease-please-v0.5.0...rustlease-please-v0.6.0) (2026-04-11)

### Features

* unify action into single-workflow with immutable release support

## [0.5.0](https://github.com/kinyoklion/rustlease-please/compare/rustlease-please-v0.4.0...rustlease-please-v0.5.0) (2026-04-11)

### Features

* download pre-built binary instead of building from source

## 0.3.0 (2026-04-11)

### Features

* implement Phase 8 GitHub Action
* add node-workspace and sentence-case plugins
* add Java, Ruby, PHP, Elixir, and Bazel release strategies
* add Python, Go, Helm, and Dart release strategies
* implement release and bootstrap CLI commands
* implement Phase 6 plugin system
* implement Phase 9 integration test suite
* implement CLI release-pr command with JSON output
* implement Phase 5 monorepo orchestration
* implement Phase 4 MVP file updaters
* implement Phase 3 changelog generation
* implement Phase 2 version calculation
* implement Phase 1 core foundations

### Bug Fixes

* fix YAML syntax in release.yml workflow condition
* only block PR creation on merged pending PRs, not updates
* update workspace member Cargo.tomls and populate changelog URLs
* sync Cargo.lock with local Cargo.toml version
* use rfind instead of filter().next_back() for clippy
* prevent duplicate release PR after merge
* pass CI checks — cargo fmt, clippy, and label creation
* clean working tree before branch checkout in release-pr
* create autorelease labels before applying to PR

## 0.2.0 (2026-04-11)

### Features

* implement Phase 8 GitHub Action
* add node-workspace and sentence-case plugins
* add Java, Ruby, PHP, Elixir, and Bazel release strategies
* add Python, Go, Helm, and Dart release strategies
* implement release and bootstrap CLI commands
* implement Phase 6 plugin system
* implement Phase 9 integration test suite
* implement CLI release-pr command with JSON output
* implement Phase 5 monorepo orchestration
* implement Phase 4 MVP file updaters
* implement Phase 3 changelog generation
* implement Phase 2 version calculation
* implement Phase 1 core foundations

### Bug Fixes

* clean working tree before branch checkout in release-pr
* create autorelease labels before applying to PR
