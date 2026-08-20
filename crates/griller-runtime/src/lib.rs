//! `griller-runtime` implements everything that touches the world: process
//! execution, git interaction, repository discovery, evidence collectors, and
//! a JSON evidence store.
//!
//! Everything here is testable **without an LLM**. The dependency direction is
//! one-way: `griller-runtime` depends on `griller-core`, never the reverse.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod attack;
pub mod collectors;
pub mod error;
pub mod execution;
pub mod git;
pub mod identity;
pub mod manifest;
pub mod store;
pub mod vocabulary;
pub mod worktree;
