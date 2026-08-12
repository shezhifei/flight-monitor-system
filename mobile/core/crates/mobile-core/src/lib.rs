//! Pure Rust business logic for the Flutter mobile app.
//!
//! This crate intentionally has zero `flutter_rust_bridge` dependency.
//! All FFI exposure lives in `mobile-ffi`.

pub mod config;
pub mod error;
pub mod signing;
pub mod sse;

pub use config::ApiConfig;
pub use error::CoreError;
