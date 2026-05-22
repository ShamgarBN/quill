//! Service layer.
//!
//! Each submodule owns a clearly-bounded responsibility. Commands compose
//! services; services do not compose commands.

pub mod crypto;
pub mod git;
pub mod storage;
