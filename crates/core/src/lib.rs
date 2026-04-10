// rustlease-please: core library for automated release management

// Phase 1: Core Foundations
pub mod commit;
pub mod config;
pub mod error;
pub mod git;
pub mod version;

// Phase 2
// pub mod versioning; // P2.1-P2.2: Versioning strategies
// pub mod tag;        // P2.3: Tag name generation/parsing

// Phase 3
// pub mod changelog;  // P3.1-P3.2: Changelog generation

// Phase 4
// pub mod strategy;   // P4.*: Release strategies / file updaters

// Phase 5
// pub mod manifest;   // P5.*: Monorepo orchestration

// Phase 6
// pub mod plugin;     // P6.*: Plugin system

#[cfg(test)]
pub(crate) mod testutil;
