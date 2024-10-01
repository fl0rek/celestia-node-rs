#![doc = include_str!("../README.md")]
#![cfg(target_arch = "wasm32")]

pub mod client;
mod commands;
pub mod error;
mod ports;
pub mod utils;
mod worker;
pub mod wrapper;

pub use client::NodeClient;
pub use worker::NodeWorker;

#[cfg(test)]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
