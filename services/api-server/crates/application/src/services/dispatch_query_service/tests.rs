use super::helpers::{build_employee_view_items, build_equipment_view_items};
use super::serialize_orders_with_receipt_summaries;
use crate::test_support::UnwiredRepository;

#[test]
fn build_view_items_do_not_use_unrecoverable_single_item_expect() {
    let source = include_str!("timeline.rs");

    assert!(
        !source.contains(".expect(\"single order item\")"),
        "timeline view builders must not panic with expect(\"single order item\")"
    );
}

#[test]
fn build_employee_and_equipment_view_items_handle_empty_orders() {
    assert!(build_employee_view_items(&[]).is_empty());
    assert!(build_equipment_view_items(&[]).is_empty());
}

#[tokio::test]
async fn serialize_empty_orders_does_not_touch_repository() {
    let repo = UnwiredRepository;
    let payload = serialize_orders_with_receipt_summaries(&repo, &[])
        .await
        .expect("empty list is ok");
    assert!(payload.is_empty());
}
