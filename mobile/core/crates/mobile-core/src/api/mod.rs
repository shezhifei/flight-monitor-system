//! Domain-level API wrappers (plan §4 feeds from these).
//!
//! P1: auth / device / workbench / dispatch main flow.
//! P2: chat (dispatch collaboration), notifications, shift handover.
//! P3: business cases, mobile operations event feed.

pub mod auth;
pub mod business_case;
pub mod chat;
pub mod dispatch;
pub mod handover;
pub mod mobile;
pub mod notification;
pub mod operations;
