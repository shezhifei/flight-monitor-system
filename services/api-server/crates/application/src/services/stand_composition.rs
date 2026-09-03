//! 机位组成（`Stand.composed_of`）共用业务校验。
//!
//! 资源管理（`TerminalResourceService`）与派工资源（`DispatchResourceService`）
//! 的机位写入路径都必须执行同一组不变量；此前两边各有一份实现（terminal 只挡
//! 自引用与一层互指，dispatch 才有成环 DFS + 双父），这里抽成一份纯函数，
//! 两个服务都以「内存机位快照」调用，单测不需要数据库。
//!
//! 不变量（全部 `DomainError::Conflict` → HTTP 409）：
//! - 子机位重复引用 / 自引用；
//! - 子机位不存在或已停用；
//! - 组成关系成环（含 A→B→C→A 的嵌套环）；
//! - 同一子机位被两个父机位引用（双父）。
//!
//! 占用拦截不在本期范围（见 2026-08-27 设计 §2「规则」）。

use std::collections::{HashMap, HashSet};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Stand;
use serde_json::Value;

/// 从 `attributes` 里取 `composed_of` 子机位 code 列表（去空白、保序、不去重）。
pub fn composed_of_codes(attributes: &Value) -> Vec<String> {
    attributes
        .get("composed_of")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_string)
        .collect()
}

/// 校验「把 `stand_code` 的组成改写为 `composed_of`」这一写操作。
///
/// `stands` 是当前全部机位的内存快照（应含停用机位；被更新的机位自身以旧值
/// 出现也无妨 —— 它的组成会被候选值覆盖）。返回 `Err(Conflict)` 表示 409。
pub fn validate_stand_composition(
    stand_code: &str,
    composed_of: &[String],
    stands: &[Stand],
) -> Result<(), DomainError> {
    if composed_of.is_empty() {
        return Ok(());
    }

    let root = stand_code.trim().to_ascii_lowercase();
    let child_codes = composed_of
        .iter()
        .map(|code| code.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    if child_codes.len() != child_codes.iter().collect::<HashSet<_>>().len() {
        return Err(DomainError::Conflict("机位 composed_of 不应重复引用同一子机位".into()));
    }
    if child_codes.iter().any(|code| code == &root) {
        return Err(DomainError::Conflict("机位 composed_of 不能引用自身".into()));
    }

    // 子机位必须存在且未停用（业务外键，无物理 FK）。
    let stand_by_code = |code: &str| stands.iter().find(|stand| stand.code.trim().eq_ignore_ascii_case(code));
    for (child_key, child_display) in child_codes.iter().zip(composed_of.iter()) {
        let child = stand_by_code(child_key)
            .ok_or_else(|| DomainError::Conflict(format!("组成机位 {child_display} 不存在")))?;
        if !child.is_active {
            return Err(DomainError::Conflict(format!("组成机位 {child_display} 已停用")));
        }
    }

    // 组成图：候选值覆盖同 code 节点，嵌套环与两点环一样拒绝。
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for stand in stands {
        let code = stand.code.trim().to_ascii_lowercase();
        let nested = composed_of_codes(&stand.attributes)
            .into_iter()
            .map(|value| value.to_ascii_lowercase())
            .collect();
        graph.insert(code, nested);
    }
    graph.insert(root.clone(), child_codes.clone());

    fn reaches(
        graph: &HashMap<String, Vec<String>>,
        current: &str,
        target: &str,
        visiting: &mut HashSet<String>,
    ) -> bool {
        if current == target {
            return true;
        }
        if !visiting.insert(current.to_string()) {
            return false;
        }
        let found = graph
            .get(current)
            .into_iter()
            .flatten()
            .any(|next| reaches(graph, next, target, visiting));
        visiting.remove(current);
        found
    }

    for child_code in &child_codes {
        if reaches(&graph, child_code, &root, &mut HashSet::new()) {
            return Err(DomainError::Conflict(format!(
                "机位组成关系会形成环: {stand_code} -> {child_code}"
            )));
        }
    }

    // 双父：同一子机位不得被另一个父机位引用。
    for stand in stands {
        let peer = stand.code.trim().to_ascii_lowercase();
        if peer == root {
            continue;
        }
        let peer_children = composed_of_codes(&stand.attributes);
        if child_codes
            .iter()
            .any(|code| peer_children.iter().any(|existing| existing.eq_ignore_ascii_case(code)))
        {
            return Err(DomainError::Conflict(format!(
                "组成机位已被另一父机位 {} 引用",
                stand.code.trim()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stand(code: &str, composed_of: &[&str], is_active: bool) -> Stand {
        Stand {
            id: format!("id-{code}"),
            code: code.to_string(),
            name: None,
            terminal: None,
            area: None,
            position_lat: 0.0,
            position_lng: 0.0,
            stand_type: None,
            size_category: None,
            is_active,
            attributes: if composed_of.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({ "composed_of": composed_of })
            },
            created_at: None,
        }
    }

    fn composed(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn conflict_message(result: Result<(), DomainError>) -> String {
        match result {
            Err(DomainError::Conflict(message)) => message,
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn accepts_flat_composition() {
        let stands = vec![
            stand("316", &[], true),
            stand("316L", &[], true),
            stand("316R", &[], true),
        ];
        assert!(validate_stand_composition("316", &composed(&["316L", "316R"]), &stands).is_ok());
    }

    #[test]
    fn rejects_self_reference_and_duplicates() {
        let stands = vec![stand("316", &[], true), stand("316L", &[], true)];
        let self_ref = validate_stand_composition("316", &composed(&["316"]), &stands);
        assert!(self_ref.is_err());
        assert!(conflict_message(self_ref).contains("自身"));

        let dup = validate_stand_composition("316", &composed(&["316L", "316L"]), &stands);
        assert!(conflict_message(dup).contains("重复"));
    }

    #[test]
    fn rejects_missing_or_inactive_child() {
        let stands = vec![stand("316", &[], true), stand("316L", &[], false)];
        let missing = validate_stand_composition("316", &composed(&["317L"]), &stands);
        assert!(conflict_message(missing).contains("不存在"));

        let inactive = validate_stand_composition("316", &composed(&["316L"]), &stands);
        assert!(conflict_message(inactive).contains("已停用"));
    }

    #[test]
    fn rejects_direct_and_nested_cycles() {
        // 两点环：316 -> 316L，而 316L 已含 316。
        let stands = vec![stand("316", &[], true), stand("316L", &["316"], true)];
        let direct = validate_stand_composition("316", &composed(&["316L"]), &stands);
        assert!(conflict_message(direct).contains("环"));

        // 嵌套环：316 -> 316L -> 316R -> 316。
        let stands = vec![
            stand("316", &[], true),
            stand("316L", &["316R"], true),
            stand("316R", &["316"], true),
        ];
        let nested = validate_stand_composition("316", &composed(&["316L"]), &stands);
        assert!(conflict_message(nested).contains("环"));
    }

    #[test]
    fn rejects_second_parent() {
        let stands = vec![
            stand("316", &[], true),
            stand("315", &["316L"], true),
            stand("316L", &[], true),
        ];
        let result = validate_stand_composition("316", &composed(&["316L"]), &stands);
        assert!(conflict_message(result).contains("另一父机位 315"));
    }

    #[test]
    fn allows_update_that_only_replaces_own_children() {
        // 更新 316 自身时，快照里 316 仍是旧值；候选值覆盖后不应误报环/双父。
        let stands = vec![
            stand("316", &["316L", "316R"], true),
            stand("316L", &[], true),
            stand("316R", &[], true),
        ];
        assert!(validate_stand_composition("316", &composed(&["316L"]), &stands).is_ok());
    }

    #[test]
    fn empty_composition_is_always_ok() {
        let stands = vec![stand("316", &["316L"], true)];
        assert!(validate_stand_composition("316", &composed(&[]), &stands).is_ok());
        assert!(validate_stand_composition("316", &composed(&[]), &stands).is_ok());
    }

    #[test]
    fn composed_of_codes_reads_attributes_array() {
        let attributes = serde_json::json!({"composed_of": [" 316L ", "", "316R"]});
        assert_eq!(
            composed_of_codes(&attributes),
            vec!["316L".to_string(), "316R".to_string()]
        );
        assert!(composed_of_codes(&serde_json::json!({})).is_empty());
    }
}
