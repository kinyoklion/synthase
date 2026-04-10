// rustlease-please: core library for automated release management

// Phase 1: Core Foundations
pub mod commit;
pub mod config;
pub mod error;
pub mod git;
pub mod version;

// Phase 2: Version Calculation
pub mod tag;
pub mod versioning;

// Phase 3: Changelog Generation
pub mod changelog;

// Phase 4
// pub mod strategy;   // P4.*: Release strategies / file updaters

// Phase 5
// pub mod manifest;   // P5.*: Monorepo orchestration

// Phase 6
// pub mod plugin;     // P6.*: Plugin system

#[cfg(test)]
pub(crate) mod testutil;
