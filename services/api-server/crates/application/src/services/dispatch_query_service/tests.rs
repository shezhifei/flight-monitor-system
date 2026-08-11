use super::helpers::{build_employee_view_items, build_equipment_view_items};

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
