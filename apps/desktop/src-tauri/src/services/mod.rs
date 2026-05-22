//! Service layer.
//!
//! Each submodule owns a clearly-bounded responsibility. Commands compose
//! services; services do not compose commands.

pub mod canon;
pub mod crypto;
pub mod git;
pub mod llm;
pub mod manuscript;
pub mod storage;
pub mod structure;
pub mod vector;
pub mod voice;
