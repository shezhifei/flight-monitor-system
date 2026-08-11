// Re-export organizer: the original `snapshot` content has been split into
// focused sub-modules. The historical `super::snapshot` import path (declared
// via `#[path = "snapshot.rs"] mod snapshot;` inside `service.rs`) continues
// to resolve via these sub-module declarations.
//
// Each sub-module contributes an `impl DispatchFrontendReplanService` block
// containing the methods that belong to that concern. Methods that must be
// callable from sibling sub-modules are declared `pub(super)` so they are
// visible across the `snapshot` module tree.

#[path = "snapshot_anchors.rs"]
mod snapshot_anchors;
#[path = "snapshot_build.rs"]
mod snapshot_build;
#[path = "snapshot_candidates.rs"]
mod snapshot_candidates;
#[path = "snapshot_impact.rs"]
mod snapshot_impact;
#[path = "snapshot_mining.rs"]
mod snapshot_mining;
#[path = "snapshot_order.rs"]
mod snapshot_order;
#[path = "snapshot_slots.rs"]
mod snapshot_slots;
