//! Pure Rust business logic for the Flutter mobile app.
//!
//! This crate intentionally has zero `flutter_rust_bridge` dependency.
//! All FFI exposure lives in `mobile-ffi`.

pub mod api;
pub mod client;
pub mod config;
pub mod dto;
pub mod error;
pub mod offline;
pub mod session;
pub mod signing;
pub mod sse;

pub use client::ApiClient;
pub use config::ApiConfig;
pub use error::CoreError;
pub use session::{SessionManager, TokenBundle};
