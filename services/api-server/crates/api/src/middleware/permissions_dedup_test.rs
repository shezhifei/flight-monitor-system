#[cfg(test)]
mod dedup_assertions {
    use std::fs;
    use std::path::Path;

    fn count_auth_helpers_in_rs(content: &str) -> usize {
        content.matches("fn ensure_grant").count()
            + content.matches("fn ensure_authenticated").count()
            + content.matches("fn has_resource_wildcard").count()
    }

    #[test]
    fn no_duplicated_auth_helpers_in_top_level_routes() {
        let routes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
        let targets = [
            "ai_copilot.rs",
            "business_case_workflows.rs",
            "workflow_forms.rs",
            "archive.rs",
            "kpi.rs",
            "reference.rs",
        ];
        let mut total = 0;
        for name in targets {
            let path = routes_dir.join(name);
            let content = fs::read_to_string(&path).unwrap_or_default();
            total += count_auth_helpers_in_rs(&content);
        }
        assert_eq!(
            total, 0,
            "found {total} duplicated auth helper definitions in top-level routes — use middleware::permissions::PermissionCheck instead"
        );
    }
}
