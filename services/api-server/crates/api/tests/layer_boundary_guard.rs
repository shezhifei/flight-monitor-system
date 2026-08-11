use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("read source directory") {
        let entry = entry.expect("read dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn strip_test_modules(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;

    while let Some(relative_attr_start) = source[cursor..].find("#[cfg(test)]") {
        let attr_start = cursor + relative_attr_start;
        let after_attr = attr_start + "#[cfg(test)]".len();
        let rest = &source[after_attr..];
        let module_start = after_attr + rest.len() - rest.trim_start().len();
        let module_rest = &source[module_start..];
        if !module_rest.starts_with("mod") {
            output.push_str(&source[cursor..after_attr]);
            cursor = after_attr;
            continue;
        }

        let after_mod = module_start + "mod".len();
        let Some(after_mod_char) = source[after_mod..].chars().next() else {
            break;
        };
        if after_mod_char == '_' || after_mod_char.is_ascii_alphanumeric() {
            output.push_str(&source[cursor..after_attr]);
            cursor = after_attr;
            continue;
        }

        let ident_search = &source[after_mod..];
        let ident_start = after_mod + ident_search.len() - ident_search.trim_start().len();
        let Some(first_ident_char) = source[ident_start..].chars().next() else {
            break;
        };
        if !(first_ident_char == '_' || first_ident_char.is_ascii_alphabetic()) {
            output.push_str(&source[cursor..after_attr]);
            cursor = after_attr;
            continue;
        }

        let mut after_ident = ident_start;
        for value in source[ident_start..].chars() {
            if value == '_' || value.is_ascii_alphanumeric() {
                after_ident += value.len_utf8();
            } else {
                break;
            }
        }

        let after_ident_rest = &source[after_ident..];
        let next_token = after_ident + after_ident_rest.len() - after_ident_rest.trim_start().len();
        let Some(next_char) = source[next_token..].chars().next() else {
            break;
        };
        if next_char == ';' {
            output.push_str(&source[cursor..next_token + next_char.len_utf8()]);
            cursor = next_token + next_char.len_utf8();
            continue;
        }
        if next_char != '{' {
            output.push_str(&source[cursor..after_attr]);
            cursor = after_attr;
            continue;
        }

        let block_start = next_token;
        output.push_str(&source[cursor..attr_start]);

        let mut depth = 0;
        let mut block_end = None;
        for (relative_index, value) in source[block_start..].char_indices() {
            match value {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        block_end = Some(block_start + relative_index + value.len_utf8());
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(end) = block_end else {
            cursor = attr_start;
            break;
        };
        cursor = end;
    }

    output.push_str(&source[cursor..]);
    output
}

fn dependency_keys(cargo_toml: &str) -> Vec<&str> {
    let mut keys = Vec::new();
    let mut in_dependencies = false;

    for line in cargo_toml.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_dependencies = line == "[dependencies]";
            continue;
        }

        if !in_dependencies {
            continue;
        }

        if let Some((key, _)) = line.split_once('=') {
            keys.push(key.trim());
        }
    }

    keys
}

#[test]
fn strip_test_modules_keeps_production_after_inline_test_module() {
    let source = r#"
fn before() {}

#[cfg(test)]
mod tests {
    fn test_only() {
        let _ = "fms_infrastructure::repositories";
    }
}

fn after() {
    let _ = "sqlx::query";
}
"#;

    let production_source = strip_test_modules(source);

    assert!(production_source.contains("fn before()"));
    assert!(production_source.contains("fn after()"));
    assert!(production_source.contains("sqlx::query"));
    assert!(!production_source.contains("fms_infrastructure::repositories"));
}

#[test]
fn strip_test_modules_keeps_production_after_external_test_module_declaration() {
    let source = r#"
#[cfg(test)]
mod tests;

pub fn configure() {
    let _ = "sqlx::query";
}
"#;

    let production_source = strip_test_modules(source);

    assert!(production_source.contains("mod tests;"));
    assert!(production_source.contains("pub fn configure()"));
    assert!(production_source.contains("sqlx::query"));
}

#[test]
fn routes_do_not_depend_on_infrastructure_repositories_or_raw_sql() {
    let routes_dir = crate_root().join("src").join("routes");
    let mut files = Vec::new();
    collect_rs_files(&routes_dir, &mut files);

    let allowed_files = [
        // Test-only route support still constructs concrete repositories.
        Path::new("ai_execution_readiness/tests.rs"),
        Path::new("ai_internal/tests.rs"),
        Path::new("ai_proposals/tests.rs"),
        Path::new("nl_query/tests.rs"),
    ];

    let forbidden_patterns = [
        "fms_infrastructure::repositories",
        "sqlx::query",
        "sqlx::query_as",
        "sqlx::query_scalar",
        "web::Data<PgPool>",
        "Data<PgPool>",
    ];

    let mut violations = Vec::new();
    for file in files {
        let rel = file.strip_prefix(&routes_dir).expect("file under routes");
        if allowed_files.iter().any(|allowed| allowed == &rel) {
            continue;
        }

        let source = fs::read_to_string(&file).expect("read rust source");
        let production_source = strip_test_modules(&source);
        for pattern in forbidden_patterns {
            if production_source.contains(pattern) {
                violations.push(format!("{} contains `{}`", rel.display(), pattern));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "route layer boundary violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn api_crate_cargo_dependencies_do_not_include_infrastructure_boundaries() {
    let cargo_toml = fs::read_to_string(crate_root().join("Cargo.toml")).expect("read Cargo.toml");
    let dependencies = dependency_keys(&cargo_toml);

    for dependency in ["fms-infrastructure", "sqlx", "redis"] {
        assert!(
            !dependencies.contains(&dependency),
            "api crate production dependencies must not include `{dependency}`"
        );
    }
}
