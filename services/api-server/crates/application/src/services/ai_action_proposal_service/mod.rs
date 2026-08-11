//! AI action proposal service (object-action semantics for controlled AI writes).

pub mod error;
pub mod schemas;

mod helpers;
mod noop_repository;
mod service;

#[cfg(test)]
mod tests;

pub use error::AiActionProposalError;
pub use noop_repository::NoopAiProposalRepository;
pub use schemas::{
    ApproveProposalRequest, ExecuteProposalRequest, GenerateProposalRequest, RejectProposalRequest,
    SubmitProposalRequest, ValidateProposalRequest,
};
pub use service::AiActionProposalService;
