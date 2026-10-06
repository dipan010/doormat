//! doormat-core: spatial document graph engine for credential detection.
//!
//! This crate provides the core detection pipeline that analyzes 2D grid
//! documents (spreadsheets) and infers typed relationships between cells
//! based on spatial proximity, content features, and scoring heuristics.

/// Candidate space reduction and cell classification.
pub mod candidates;
/// Inline, formula, and comment credential detection.
pub mod detection;
/// Feature extraction: normalization, entropy, header matching.
pub mod features;
/// Relationship inference and split-password detection.
pub mod inference;
/// Pipeline orchestration and deduplication.
pub mod pipeline;
/// BFS region detection for spatially adjacent candidates.
pub mod regions;
/// Candidate scoring with distance, content, and context bonuses.
pub mod scoring;
/// Spatial distance table and neighbor queries.
pub mod spatial;
/// Columnar cell store with Arrow-backed immutable columns.
pub mod store;
/// Core types: enums, structs, constants, and feature-flag bitmasks.
pub mod types;

/// Returns the crate version string.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
