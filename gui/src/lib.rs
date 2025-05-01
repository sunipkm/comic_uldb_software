#![deny(missing_docs)]
//!   
//! # Generic Camera GUI
//! This is the entry point when compiled to WebAssembly.
//!  

mod app;
pub use app::GenCamGUI;

#[cfg(target_arch = "wasm32")]
mod web;
