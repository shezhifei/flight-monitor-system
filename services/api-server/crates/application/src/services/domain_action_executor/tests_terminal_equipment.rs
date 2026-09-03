//! PR4 缺口接线测试：Terminal.add_*/remove_* 六个分支 + Equipment.assign/release。
//! 共享辅助函数在 `tests` 模块（`build_executor` / 种子函数）。

use super::tests::{build_executor, create_pool, has_pool, insert_test_flight};
use super::DomainActionError;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// PR4 缺口接线：Terminal.add_*/remove_* 六个分支 + Equipment.assign/release。
// 均走「参数映射 + 领域服务调用」；权限由提案主管线验（governed schema.required_permissions）。
// ---------------------------------------------------------------------------

/// 种子：航站楼 + 机位/口/转盘目录行（幂等）。
async fn seed_terminal_member_fixtures(pool: &sqlx::PgPool) {
    sqlx::query(
        "INSERT INTO terminals (terminal_id, code, name, is_active) VALUES ('TM_EXEC_T', 'TE', 'Exec Terminal', TRUE) \
         ON CONFLICT (terminal_id) DO UPDATE SET is_active = TRUE",
    )
    .execute(pool)
    .await
    .expect("insert test terminal");
    sqlx::query(
        "INSERT INTO stands (id, code, is_active, position_lat, position_lng) \
         VALUES ('ST_EXEC_1', 'SE1', TRUE, 0, 0) ON CONFLICT (id) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("insert test stand");
    sqlx::query("INSERT INTO gates (gate_id, code, is_active) VALUES ('GT_EXEC_1', 'GE1', TRUE) ON CONFLICT (gate_id) DO NOTHING")
        .execute(pool)
        .await
        .expect("insert test gate");
    sqlx::query(
        "INSERT INTO baggage_carousels (carousel_id, code, is_active) VALUES ('CR_EXEC_1', 'CE1', TRUE) \
         ON CONFLICT (carousel_id) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("insert test carousel");
    // 清掉上次运行残留的成员关系。
    sqlx::query("DELETE FROM terminal_stands WHERE stand_id = 'ST_EXEC_1'")
        .execute(pool)
        .await
        .expect("clean terminal_stands");
    sqlx::query("DELETE FROM terminal_gates WHERE gate_id = 'GT_EXEC_1'")
        .execute(pool)
        .await
        .expect("clean terminal_gates");
    sqlx::query("DELETE FROM terminal_carousels WHERE carousel_id = 'CR_EXEC_1'")
        .execute(pool)
        .await
        .expect("clean terminal_carousels");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_terminal_add_stand_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_terminal_member_fixtures(&pool).await;

    let executor = build_executor(pool.clone()).await;
    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_EXEC_T",
            "add_stand",
            &json!({"stand_id": "ST_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "expected Ok, got {:?}", res);

    let linked: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM terminal_stands WHERE terminal_id = 'TM_EXEC_T' AND stand_id = 'ST_EXEC_1')",
    )
    .fetch_one(&pool)
    .await
    .expect("check terminal_stands");
    assert!(linked, "stand must be linked into terminal_stands");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_terminal_remove_stand_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_terminal_member_fixtures(&pool).await;
    sqlx::query(
        "INSERT INTO terminal_stands (terminal_id, stand_id) VALUES ('TM_EXEC_T', 'ST_EXEC_1') ON CONFLICT DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("link stand");

    let executor = build_executor(pool.clone()).await;
    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_EXEC_T",
            "remove_stand",
            &json!({"stand_id": "ST_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "expected Ok, got {:?}", res);

    let linked: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM terminal_stands WHERE stand_id = 'ST_EXEC_1')")
        .fetch_one(&pool)
        .await
        .expect("check terminal_stands");
    assert!(!linked, "stand membership must be removed");
}

/// `remove_stand` 有未结束占用 → 领域服务 Conflict(409 明细)，执行器映射 Execution("conflict: ...")。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_terminal_remove_stand_conflict_on_active_occupation() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_terminal_member_fixtures(&pool).await;
    sqlx::query(
        "INSERT INTO terminal_stands (terminal_id, stand_id) VALUES ('TM_EXEC_T', 'ST_EXEC_1') ON CONFLICT DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("link stand");
    insert_test_flight(&pool, "FL_EXEC_OCC").await;
    // stand_occupations.registration 外键到 aircraft，先补一行测试飞机。
    sqlx::query("INSERT INTO aircraft (registration) VALUES ('B-EXEC-OCC') ON CONFLICT (registration) DO NOTHING")
        .execute(&pool)
        .await
        .expect("insert test aircraft");
    sqlx::query(
        "INSERT INTO stand_occupations (id, stand_code, flight_id, registration, starts_at, ends_at, status) \
         VALUES ('OCC_EXEC_1', 'SE1', 'FL_EXEC_OCC', 'B-EXEC-OCC', NOW() - INTERVAL '1 hour', NOW() + INTERVAL '1 hour', 'active') \
         ON CONFLICT (id) DO UPDATE SET ends_at = EXCLUDED.ends_at, status = 'active'",
    )
    .execute(&pool)
    .await
    .expect("insert active occupation");

    let executor = build_executor(pool.clone()).await;
    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_EXEC_T",
            "remove_stand",
            &json!({"stand_id": "ST_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(
        matches!(res, Err(DomainActionError::Execution(ref msg)) if msg.contains("conflict")),
        "expected conflict execution error, got {:?}",
        res
    );

    let linked: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM terminal_stands WHERE stand_id = 'ST_EXEC_1')")
        .fetch_one(&pool)
        .await
        .expect("check terminal_stands");
    assert!(linked, "conflict must not remove the membership");
    sqlx::query("DELETE FROM stand_occupations WHERE id = 'OCC_EXEC_1'")
        .execute(&pool)
        .await
        .expect("clean occupation");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_terminal_member_actions_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    // 缺成员 id，在触碰 DB 前即被参数校验拦截。
    for action in [
        "add_stand",
        "remove_stand",
        "add_gate",
        "remove_gate",
        "add_carousel",
        "remove_carousel",
    ] {
        let res = executor
            .execute_approved_action("Terminal", "TM_EXEC_T", action, &json!({}), "tester")
            .await;
        assert!(
            matches!(res, Err(DomainActionError::Validation(_))),
            "Terminal.{action} with empty args must be Validation, got {:?}",
            res
        );
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_terminal_add_member_terminal_not_found() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_terminal_member_fixtures(&pool).await;
    let executor = build_executor(pool).await;
    // add_* 需先验楼存在且启用：楼不存在 → NotFound。
    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_NONEXISTENT",
            "add_gate",
            &json!({"gate_id": "GT_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))), "got {:?}", res);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_terminal_gate_carousel_member_roundtrip() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_terminal_member_fixtures(&pool).await;
    let executor = build_executor(pool.clone()).await;

    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_EXEC_T",
            "add_gate",
            &json!({"gate_id": "GT_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "add_gate expected Ok, got {:?}", res);
    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_EXEC_T",
            "add_carousel",
            &json!({"carousel_id": "CR_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "add_carousel expected Ok, got {:?}", res);

    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_EXEC_T",
            "remove_gate",
            &json!({"gate_id": "GT_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "remove_gate expected Ok, got {:?}", res);
    let res = executor
        .execute_approved_action(
            "Terminal",
            "TM_EXEC_T",
            "remove_carousel",
            &json!({"carousel_id": "CR_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "remove_carousel expected Ok, got {:?}", res);

    let remaining: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM terminal_gates WHERE gate_id = 'GT_EXEC_1') \
         + (SELECT COUNT(*) FROM terminal_carousels WHERE carousel_id = 'CR_EXEC_1')",
    )
    .fetch_one(&pool)
    .await
    .expect("count remaining memberships");
    assert_eq!(remaining, 0, "gate/carousel memberships must be removed");
}

/// `Terminal.deactivate` 未接执行器（schema execution_mapping=None）→ fail-closed；
/// 小写 "terminal" 对象键已废弃 → unknown action。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_terminal_unmapped_and_legacy_key_fail_closed() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Terminal", "TM1", "deactivate", &json!({}), "tester")
        .await;
    assert!(
        matches!(res, Err(DomainActionError::NotFound(_))),
        "Terminal.deactivate must stay fail-closed, got {:?}",
        res
    );
    let res = executor
        .execute_approved_action("terminal", "TM1", "add_stand", &json!({"stand_id": "ST1"}), "tester")
        .await;
    assert!(
        matches!(res, Err(DomainActionError::NotFound(_))),
        "legacy lowercase terminal key must be unknown action, got {:?}",
        res
    );
}

/// 种子：带设备槽快照的工单 + 可用设备（幂等）。
/// 种子：设备 + 带拖拉机槽位的工单。各测试用独立 id，避免 DB 并发互踩。
async fn seed_equipment_slot_fixtures(pool: &sqlx::PgPool, order_id: &str, eq_id: &str, eq_code: &str) {
    insert_test_flight(pool, order_id).await;
    sqlx::query("DELETE FROM dispatch_order_equipment WHERE dispatch_order_id = $1")
        .bind(order_id)
        .execute(pool)
        .await
        .expect("clean order equipment");
    sqlx::query("UPDATE equipment SET current_dispatch_id = NULL, status = 'available' WHERE id = $1")
        .bind(eq_id)
        .execute(pool)
        .await
        .expect("reset equipment");
    sqlx::query("INSERT INTO equipment (id, code, status) VALUES ($1, $2, 'available') ON CONFLICT (id) DO NOTHING")
        .bind(eq_id)
        .bind(eq_code)
        .execute(pool)
        .await
        .expect("insert test equipment");
    sqlx::query(
        "INSERT INTO dispatch_orders (id, flight_id, task_type, publication_state, status, source_type, equipment_requirement_snapshot, equipment_assignment) \
         VALUES ($1, $1, 'T', 'prepublished', 'pending', 'generated', \
                 '[{\"slot_code\": \"tractor\", \"required_count\": 1}]', '[]') \
         ON CONFLICT (id) DO UPDATE SET \
            status = 'pending', \
            equipment_requirement_snapshot = EXCLUDED.equipment_requirement_snapshot, \
            equipment_assignment = '[]', \
            updated_at = NOW()",
    )
    .bind(order_id)
    .execute(pool)
    .await
    .expect("insert test order");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_equipment_assign_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    // 缺 equipment_id / dispatch_order_id / slot_code，在触碰 DB 前即被参数校验拦截。
    let res = executor
        .execute_approved_action("Equipment", "EQ1", "assign", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
    let res = executor
        .execute_approved_action(
            "Equipment",
            "EQ1",
            "assign",
            &json!({"equipment_id": "EQ1", "dispatch_order_id": "DP1"}),
            "tester",
        )
        .await;
    assert!(
        matches!(res, Err(DomainActionError::Validation(_))),
        "missing slot_code must be Validation, got {:?}",
        res
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_equipment_assign_order_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "Equipment",
            "EQ1",
            "assign",
            &json!({"equipment_id": "EQ1", "dispatch_order_id": "DP_NONEXISTENT", "slot_code": "tractor"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))), "got {:?}", res);
}

/// happy path：assign 落槽（快照列 + 设备行 + 关联表同事务），release 缺省工单时按设备占用反查。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_equipment_assign_and_release_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_equipment_slot_fixtures(&pool, "DP_EXEC_EQ", "EQ_EXEC_1", "EQX1").await;

    let executor = build_executor(pool.clone()).await;
    let res = executor
        .execute_approved_action(
            "Equipment",
            "EQ_EXEC_1",
            "assign",
            &json!({"equipment_id": "EQ_EXEC_1", "dispatch_order_id": "DP_EXEC_EQ", "slot_code": "tractor"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "assign expected Ok, got {:?}", res);

    let current_dispatch: Option<String> =
        sqlx::query_scalar("SELECT current_dispatch_id FROM equipment WHERE id = 'EQ_EXEC_1'")
            .fetch_one(&pool)
            .await
            .expect("fetch equipment dispatch");
    assert_eq!(
        current_dispatch.as_deref(),
        Some("DP_EXEC_EQ"),
        "assign must claim the equipment row"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM equipment WHERE id = 'EQ_EXEC_1'")
        .fetch_one(&pool)
        .await
        .expect("fetch equipment status");
    assert_eq!(status, "in_use");
    let assignment: serde_json::Value =
        sqlx::query_scalar("SELECT equipment_assignment FROM dispatch_orders WHERE id = 'DP_EXEC_EQ'")
            .fetch_one(&pool)
            .await
            .expect("fetch equipment_assignment");
    assert!(
        assignment.as_array().is_some_and(|entries| entries.iter().any(|entry| {
            entry.get("slot_code").and_then(Value::as_str) == Some("tractor")
                && entry.get("equipment_id").and_then(Value::as_str) == Some("EQ_EXEC_1")
        })),
        "snapshot must contain the slot assignment, got {assignment}"
    );
    let link_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM dispatch_order_equipment WHERE dispatch_order_id = 'DP_EXEC_EQ' AND equipment_id = 'EQ_EXEC_1' AND released_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("count equipment links");
    assert_eq!(link_count, 1);

    // release 不传 dispatch_order_id：按 equipment.current_dispatch_id 反查工单。
    let res = executor
        .execute_approved_action(
            "Equipment",
            "EQ_EXEC_1",
            "release",
            &json!({"equipment_id": "EQ_EXEC_1"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "release expected Ok, got {:?}", res);

    let current_dispatch: Option<String> =
        sqlx::query_scalar("SELECT current_dispatch_id FROM equipment WHERE id = 'EQ_EXEC_1'")
            .fetch_one(&pool)
            .await
            .expect("fetch equipment dispatch after release");
    assert!(current_dispatch.is_none(), "release must free the equipment row");
    let assignment: serde_json::Value =
        sqlx::query_scalar("SELECT equipment_assignment FROM dispatch_orders WHERE id = 'DP_EXEC_EQ'")
            .fetch_one(&pool)
            .await
            .expect("fetch equipment_assignment after release");
    assert_eq!(assignment, json!([]), "release must clear the slot assignment");
    let active_links: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM dispatch_order_equipment WHERE dispatch_order_id = 'DP_EXEC_EQ' AND equipment_id = 'EQ_EXEC_1' AND released_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("count active links after release");
    assert_eq!(active_links, 0);
}

/// release 的设备当前未指派到任何工单 → Validation（无工单可反查）。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_equipment_release_without_assignment_validation() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_equipment_slot_fixtures(&pool, "DP_EXEC_EQ2", "EQ_EXEC_2", "EQX2").await;
    let executor = build_executor(pool).await;
    let res = executor
        .execute_approved_action(
            "Equipment",
            "EQ_EXEC_2",
            "release",
            &json!({"equipment_id": "EQ_EXEC_2"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))), "got {:?}", res);
}

/// 设备类型与槽位要求不一致 → Validation（BusinessRuleViolation 映射）。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_equipment_assign_type_mismatch_rejected() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    seed_equipment_slot_fixtures(&pool, "DP_EXEC_EQ3", "EQ_EXEC_3", "EQX3").await;
    sqlx::query(
        "UPDATE dispatch_orders SET equipment_requirement_snapshot = '[{\"slot_code\": \"tractor\", \"equipment_type_id\": \"ET_OTHER\", \"required_count\": 1}]' \
         WHERE id = 'DP_EXEC_EQ3'",
    )
    .execute(&pool)
    .await
    .expect("pin slot equipment type");

    let executor = build_executor(pool).await;
    let res = executor
        .execute_approved_action(
            "Equipment",
            "EQ_EXEC_3",
            "assign",
            &json!({"equipment_id": "EQ_EXEC_3", "dispatch_order_id": "DP_EXEC_EQ3", "slot_code": "tractor"}),
            "tester",
        )
        .await;
    assert!(
        matches!(res, Err(DomainActionError::Validation(ref msg)) if msg.contains("类型")),
        "expected type-mismatch validation, got {:?}",
        res
    );
}
