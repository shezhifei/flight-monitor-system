//! 派工协作仓储的数据库集成测试。
//!
//! Focused on the unread-count arithmetic: `dispatch_chat_messages.seq_no` is a
//! table-global `BIGSERIAL`, so any unread formula built on `MAX(seq_no) -
//! last_read_seq` silently counts other groups' traffic. Every test here
//! interleaves two groups so that regression cannot pass unnoticed.

use fms_domain::models::dispatch_collaboration::{DispatchChatMessageCursor, NewDispatchChatMessage};
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_infrastructure::repositories::pg_dispatch_collaboration_repository::PgDispatchCollaborationRepository;
use serde_json::json;
use sqlx::PgPool;

async fn seed_user(pool: &PgPool, id: &str, username: &str) {
    sqlx::query("INSERT INTO users (id, username) VALUES ($1, $2)")
        .bind(id)
        .bind(username)
        .execute(pool)
        .await
        .expect("seed user");
}

async fn seed_group(pool: &PgPool, group_id: &str, flight_id: &str) {
    sqlx::query("INSERT INTO dispatch_chat_groups (group_id, flight_id, group_name) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(flight_id)
        .bind(format!("{flight_id} 保障群"))
        .execute(pool)
        .await
        .expect("seed group");
}

async fn seed_member(pool: &PgPool, group_id: &str, user_id: &str, last_read_seq: i64) {
    sqlx::query(
        r#"
        INSERT INTO dispatch_chat_group_members (id, group_id, user_id, is_assignee, last_read_seq)
        VALUES ($1, $2, $3, TRUE, $4)
        "#,
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(group_id)
    .bind(user_id)
    .bind(last_read_seq)
    .execute(pool)
    .await
    .expect("seed member");
}

fn new_message(group_id: &str, sender: &str, content: &str, client_msg_id: Option<&str>) -> NewDispatchChatMessage {
    NewDispatchChatMessage {
        message_id: ulid::Ulid::new().to_string(),
        group_id: group_id.to_string(),
        sender_user_id: Some(sender.to_string()),
        dispatch_order_id: None,
        event_id: None,
        message_type: "text".to_string(),
        content: content.to_string(),
        is_at_all: false,
        metadata: json!({}),
        client_msg_id: client_msg_id.map(str::to_string),
    }
}

/// Two groups sharing the global sequence. `unread` for group A must count only
/// A's own rows above the cursor.
#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn unread_count_ignores_other_groups_traffic(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_user(&pool, "reader", "读者").await;
    seed_user(&pool, "sender", "发送者").await;
    seed_group(&pool, "group-a", "flight-a").await;
    seed_group(&pool, "group-b", "flight-b").await;
    for group in ["group-a", "group-b"] {
        seed_member(&pool, group, "reader", 0).await;
        seed_member(&pool, group, "sender", 0).await;
    }

    // Interleaved so group A's max seq_no (5) is far above its own message count (2).
    repo.insert_message(&new_message("group-a", "sender", "a1", None))
        .await
        .unwrap();
    for i in 0..3 {
        repo.insert_message(&new_message("group-b", "sender", &format!("b{i}"), None))
            .await
            .unwrap();
    }
    let last_a = repo
        .insert_message(&new_message("group-a", "sender", "a2", None))
        .await
        .unwrap();
    assert_eq!(
        last_a.seq_no, 5,
        "seq_no is global, so group A ends up at 5 with only 2 messages"
    );

    assert_eq!(
        repo.count_group_unread("group-a", "reader").await.unwrap(),
        2,
        "group A unread must count A's rows, not the global sequence gap"
    );
    assert_eq!(repo.count_group_unread("group-b", "reader").await.unwrap(), 3);
    assert_eq!(repo.count_total_unread("reader").await.unwrap(), 5);

    let groups = repo.list_user_groups("reader", "active", 50, 0).await.unwrap();
    let unread_a = groups
        .items
        .iter()
        .find(|item| item.group_id == "group-a")
        .expect("group A listed")
        .unread_count;
    assert_eq!(unread_a, 2, "group list badge must match the per-group count");

    let group_a = repo
        .get_group_for_user("group-a", "reader")
        .await
        .unwrap()
        .expect("group A visible to member");
    assert_eq!(
        group_a.unread_count, 2,
        "opening a group must show the same badge as the list"
    );

    let by_flight = repo
        .get_group_for_user_by_flight("flight-a", "reader")
        .await
        .unwrap()
        .expect("group A resolvable by flight");
    assert_eq!(by_flight.unread_count, 2);
}

#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn deactivated_members_remain_listed_as_read_only(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_user(&pool, "reader", "读者").await;
    seed_user(&pool, "sender", "发送者").await;
    seed_group(&pool, "group-a", "flight-a").await;
    seed_member(&pool, "group-a", "reader", 0).await;
    seed_member(&pool, "group-a", "sender", 0).await;
    repo.insert_message(&new_message("group-a", "sender", "hello", None))
        .await
        .unwrap();

    repo.deactivate_members_except("group-a", &["sender".to_string()])
        .await
        .unwrap();

    let groups = repo.list_user_groups("reader", "active", 50, 0).await.unwrap();
    let listed = groups
        .items
        .iter()
        .find(|item| item.group_id == "group-a")
        .expect("read-only member still sees the group");
    assert!(listed.read_only, "deactivated membership is exposed as read_only");
    assert_eq!(listed.unread_count, 1);

    let fanout = repo
        .count_unread_for_group_members("group-a")
        .await
        .unwrap();
    assert!(
        fanout.iter().any(|entry| entry.user_id == "reader"),
        "read-only members still receive live unread fan-out"
    );
}

/// A member's own messages are never unread for that member, so a skipped
/// auto-mark-on-send cannot inflate their own badge.
#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn own_messages_do_not_count_as_unread(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_user(&pool, "alice", "Alice").await;
    seed_user(&pool, "bob", "Bob").await;
    seed_group(&pool, "group-a", "flight-a").await;
    seed_member(&pool, "group-a", "alice", 0).await;
    seed_member(&pool, "group-a", "bob", 0).await;

    repo.insert_message(&new_message("group-a", "alice", "第一条", None))
        .await
        .unwrap();
    repo.insert_message(&new_message("group-a", "alice", "第二条", None))
        .await
        .unwrap();
    repo.insert_message(&new_message("group-a", "bob", "收到", None))
        .await
        .unwrap();

    assert_eq!(
        repo.count_group_unread("group-a", "alice").await.unwrap(),
        1,
        "alice has only bob's message unread"
    );
    assert_eq!(repo.count_group_unread("group-a", "bob").await.unwrap(), 2);
}

/// The batch query that replaced the 2×M per-message round trips must agree
/// with the single-member queries it stands in for.
#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn batch_member_unread_matches_per_member_counts(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_user(&pool, "alice", "Alice").await;
    seed_user(&pool, "bob", "Bob").await;
    seed_user(&pool, "carol", "Carol").await;
    seed_group(&pool, "group-a", "flight-a").await;
    seed_group(&pool, "group-b", "flight-b").await;
    seed_member(&pool, "group-a", "alice", 0).await;
    seed_member(&pool, "group-a", "bob", 0).await;
    seed_member(&pool, "group-a", "carol", 0).await;
    // Only bob and carol also sit in the second group, so their totals must
    // exceed their group-A counts.
    seed_member(&pool, "group-b", "bob", 0).await;
    seed_member(&pool, "group-b", "carol", 0).await;

    repo.insert_message(&new_message("group-a", "alice", "a1", None))
        .await
        .unwrap();
    repo.insert_message(&new_message("group-b", "carol", "b1", None))
        .await
        .unwrap();
    repo.insert_message(&new_message("group-a", "bob", "a2", None))
        .await
        .unwrap();

    let batch = repo.count_unread_for_group_members("group-a").await.unwrap();
    assert_eq!(batch.len(), 3, "every active member of group A is reported");

    for entry in &batch {
        let expected_group = repo.count_group_unread("group-a", &entry.user_id).await.unwrap();
        let expected_total = repo.count_total_unread(&entry.user_id).await.unwrap();
        assert_eq!(entry.unread_count, expected_group, "group unread for {}", entry.user_id);
        assert_eq!(entry.unread_total, expected_total, "total unread for {}", entry.user_id);
    }

    let bob = batch.iter().find(|entry| entry.user_id == "bob").expect("bob reported");
    assert_eq!(bob.unread_count, 1, "bob has alice's message unread in group A");
    assert_eq!(bob.unread_total, 2, "bob also has carol's message unread in group B");
}

/// A retried send resolves to the stored row instead of inserting a duplicate.
#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn insert_message_deduplicates_by_client_msg_id(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_user(&pool, "alice", "Alice").await;
    seed_group(&pool, "group-a", "flight-a").await;
    seed_group(&pool, "group-b", "flight-b").await;
    seed_member(&pool, "group-a", "alice", 0).await;
    seed_member(&pool, "group-b", "alice", 0).await;

    let first = repo
        .insert_message(&new_message("group-a", "alice", "只发一次", Some("key-1")))
        .await
        .unwrap();

    // Same key, fresh message_id: exactly what a client retry looks like.
    let retry = repo
        .insert_message(&new_message("group-a", "alice", "只发一次", Some("key-1")))
        .await
        .unwrap();
    assert_eq!(retry.message_id, first.message_id, "retry must resolve to the stored row");
    assert_eq!(retry.seq_no, first.seq_no);
    assert_eq!(retry.client_msg_id.as_deref(), Some("key-1"), "the key is echoed back");

    // The key is scoped per group, and unkeyed sends are never deduplicated.
    repo.insert_message(&new_message("group-b", "alice", "另一个群", Some("key-1")))
        .await
        .unwrap();
    repo.insert_message(&new_message("group-a", "alice", "无幂等键", None))
        .await
        .unwrap();
    repo.insert_message(&new_message("group-a", "alice", "无幂等键", None))
        .await
        .unwrap();

    let page = repo
        .list_group_messages("group-a", 50, DispatchChatMessageCursor::Latest)
        .await
        .unwrap();
    assert_eq!(page.total, 3, "one deduplicated message plus two unkeyed sends");

    let found = repo
        .find_message_by_client_id("group-a", "key-1")
        .await
        .unwrap()
        .expect("keyed message findable");
    assert_eq!(found.message_id, first.message_id);
    assert!(repo
        .find_message_by_client_id("group-a", "missing")
        .await
        .unwrap()
        .is_none());
}

/// The reconnect direction: everything the client has not seen yet, oldest first.
#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn list_group_messages_after_seq_returns_the_gap_ascending(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_user(&pool, "alice", "Alice").await;
    seed_group(&pool, "group-a", "flight-a").await;
    seed_group(&pool, "group-b", "flight-b").await;
    seed_member(&pool, "group-a", "alice", 0).await;

    let mut seqs = Vec::new();
    for i in 0..4 {
        // Group B traffic between each pair keeps group A's seq_no non-contiguous.
        let message = repo
            .insert_message(&new_message("group-a", "alice", &format!("a{i}"), None))
            .await
            .unwrap();
        seqs.push(message.seq_no);
        repo.insert_message(&new_message("group-b", "alice", &format!("b{i}"), None))
            .await
            .unwrap();
    }

    let gap = repo
        .list_group_messages("group-a", 50, DispatchChatMessageCursor::After(seqs[1]))
        .await
        .unwrap();
    assert_eq!(
        gap.items.iter().map(|item| item.seq_no).collect::<Vec<_>>(),
        vec![seqs[2], seqs[3]],
        "gap-fill returns only this group's newer rows, ascending"
    );
    assert_eq!(gap.after_seq, Some(seqs[1]));
    assert!(!gap.has_more);
    assert_eq!(gap.next_after_seq, None);

    // A truncated gap must hand back where to continue, not where to scroll back.
    let truncated = repo
        .list_group_messages("group-a", 1, DispatchChatMessageCursor::After(seqs[0]))
        .await
        .unwrap();
    assert_eq!(truncated.items.len(), 1);
    assert_eq!(truncated.items[0].seq_no, seqs[1]);
    assert!(truncated.has_more);
    assert_eq!(truncated.next_after_seq, Some(seqs[1]));
    assert_eq!(truncated.next_before_seq, None);

    // Scroll-back still walks the other way and stays chronological.
    let older = repo
        .list_group_messages("group-a", 2, DispatchChatMessageCursor::Before(seqs[3]))
        .await
        .unwrap();
    assert_eq!(
        older.items.iter().map(|item| item.seq_no).collect::<Vec<_>>(),
        vec![seqs[1], seqs[2]]
    );
    assert_eq!(older.next_before_seq, Some(seqs[1]));
    assert_eq!(older.next_after_seq, None);
}

/// The read cursor moves forward only, and reports whether it actually moved.
#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn mark_group_read_reports_advance_and_never_moves_backwards(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_user(&pool, "alice", "Alice").await;
    seed_group(&pool, "group-a", "flight-a").await;
    seed_member(&pool, "group-a", "alice", 0).await;

    let advanced = repo
        .mark_group_read("group-a", "alice", 3)
        .await
        .unwrap()
        .expect("active member has a cursor");
    assert!(advanced.advanced(), "0 -> 3 is a real advance");
    assert_eq!(advanced.previous_last_read_seq, 0);
    assert_eq!(advanced.member.last_read_seq, 3);

    let repeated = repo.mark_group_read("group-a", "alice", 3).await.unwrap().unwrap();
    assert!(!repeated.advanced(), "re-reading the same seq must not look like news");
    assert_eq!(repeated.previous_last_read_seq, 3);

    let backwards = repo.mark_group_read("group-a", "alice", 1).await.unwrap().unwrap();
    assert!(!backwards.advanced());
    assert_eq!(backwards.member.last_read_seq, 3, "cursor never moves backwards");

    assert!(
        repo.mark_group_read("group-a", "ghost", 5).await.unwrap().is_none(),
        "a non-member has no cursor to move"
    );
}

/// A member whose `users` row is missing must still get their cursor moved; the
/// username join is decoration, not a filter.
#[sqlx::test(migrations = "tests/migrations_dispatch_chat")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn mark_group_read_works_without_a_users_row(pool: PgPool) {
    let repo = PgDispatchCollaborationRepository::new(pool.clone());
    seed_group(&pool, "group-a", "flight-a").await;
    seed_member(&pool, "group-a", "orphan", 0).await;

    let update = repo
        .mark_group_read("group-a", "orphan", 7)
        .await
        .unwrap()
        .expect("cursor moves even without a users row");
    assert!(update.advanced());
    assert_eq!(update.member.last_read_seq, 7);
    assert_eq!(update.member.username, None);
}
