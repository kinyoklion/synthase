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

// Phase 4: Release Strategies / File Updaters
pub mod strategy;
pub mod updater;

// Phase 5: Monorepo Orchestration
pub mod manifest;

// Phase 6
// pub mod plugin;     // P6.*: Plugin system

#[cfg(any(test, feature = "testutil"))]
pub mod testutil;
