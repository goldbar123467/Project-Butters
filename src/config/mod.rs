//! Configuration Module
//!
//! Loads and validates configuration from TOML files.

pub mod loader;

pub use loader::{
    Config, load_config,
};
