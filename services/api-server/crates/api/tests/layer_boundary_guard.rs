use std::collections::BTreeSet;
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

/// 收集 `#[cfg(test)] mod <ident>;` 形式的外部模块声明名。
///
/// 这种声明本身就是"该文件只在 test 构建中编译"的权威事实：cfg(test) 门控的模块
/// 不进生产构建，所以其中不可能存在越界的生产代码。内联 `mod x { .. }` 由
/// `strip_test_modules` 处理，此处只认 `;` 结尾的外部文件声明。
fn cfg_test_module_declarations(source: &str) -> Vec<String> {
    const ATTR: &str = "#[cfg(test)]";
    let mut names = Vec::new();
    let mut cursor = 0;

    while let Some(relative_attr) = source[cursor..].find(ATTR) {
        let after_attr = cursor + relative_attr + ATTR.len();
        cursor = after_attr;

        let attr_rest = &source[after_attr..];
        let module_start = after_attr + attr_rest.len() - attr_rest.trim_start().len();
        if !source[module_start..].starts_with("mod") {
            continue;
        }

        // `mod` 必须是完整 token，不能只是 `module` 之类标识符的前缀。
        let after_mod = module_start + "mod".len();
        match source[after_mod..].chars().next() {
            Some(value) if value == '_' || value.is_ascii_alphanumeric() => continue,
            None => continue,
            Some(_) => {}
        }

        let ident_rest = &source[after_mod..];
        let ident_start = after_mod + ident_rest.len() - ident_rest.trim_start().len();
        match source[ident_start..].chars().next() {
            Some(value) if value == '_' || value.is_ascii_alphabetic() => {}
            _ => continue,
        }

        let mut ident_end = ident_start;
        for value in source[ident_start..].chars() {
            if value == '_' || value.is_ascii_alphanumeric() {
                ident_end += value.len_utf8();
            } else {
                break;
            }
        }

        let ident_tail = &source[ident_end..];
        let next_token = ident_end + ident_tail.len() - ident_tail.trim_start().len();
        if source[next_token..].starts_with(';') {
            names.push(source[ident_start..ident_end].to_string());
            cursor = next_token + 1;
        }
    }

    names
}

/// 把 `#[cfg(test)] mod <ident>;` 声明解析成磁盘上的测试专属文件集合。
///
/// 取代手工维护的文件名白名单：白名单会随新增测试文件静默失效——`ai_internal` 下
/// 新增的 `*_tests.rs` 就因为命名与 `tests.rs` 不同而漏进了扫描范围。推导出来的
/// 集合不会漂移。
fn collect_cfg_test_module_files(root: &Path) -> BTreeSet<PathBuf> {
    let mut declaring_files = Vec::new();
    collect_rs_files(root, &mut declaring_files);

    let mut test_only = BTreeSet::new();
    for declaring in &declaring_files {
        let source = fs::read_to_string(declaring).expect("read rust source");

        // `foo/mod.rs` 的子模块在 `foo/`；`foo.rs` 的子模块同样在 `foo/`。
        let submodule_dir = if declaring.file_name().and_then(|value| value.to_str()) == Some("mod.rs") {
            declaring.parent().expect("mod.rs has a parent").to_path_buf()
        } else {
            declaring.with_extension("")
        };

        for name in cfg_test_module_declarations(&source) {
            let as_file = submodule_dir.join(format!("{name}.rs"));
            if as_file.is_file() {
                test_only.insert(as_file);
                continue;
            }
            // 目录模块：整棵子树都只在 test 构建中编译。
            let as_dir = submodule_dir.join(&name);
            if as_dir.is_dir() {
                let mut nested = Vec::new();
                collect_rs_files(&as_dir, &mut nested);
                test_only.extend(nested);
            }
        }
    }

    test_only
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
fn cfg_test_module_declarations_finds_external_test_files() {
    let source = r#"
use actix_web::web;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ontology_actions_tests;

pub mod ontology_actions;
"#;

    let names = cfg_test_module_declarations(source);

    assert_eq!(names, vec!["tests", "ontology_actions_tests"]);
}

#[test]
fn cfg_test_module_declarations_ignores_inline_modules_and_production_modules() {
    let source = r#"
pub mod production;

#[cfg(test)]
mod inline {
    fn helper() {}
}

#[cfg(feature = "other")]
mod not_a_test_module;
"#;

    let names = cfg_test_module_declarations(source);

    assert!(names.is_empty(), "unexpected: {names:?}");
}

#[test]
fn derived_test_only_set_covers_every_cfg_test_route_file() {
    let routes_dir = crate_root().join("src").join("routes");
    let test_only_files = collect_cfg_test_module_files(&routes_dir);

    // 推导必须至少覆盖到曾经手工写死的那 4 个文件，外加后来新增、
    // 因命名不同而漏出白名单的两个 `*_tests.rs`。
    for expected in [
        "ai_execution_readiness/tests.rs",
        "ai_internal/tests.rs",
        "ai_internal/ontology_actions_tests.rs",
        "ai_internal/replan_snapshot_tests.rs",
        "ai_proposals/tests.rs",
        "nl_query/tests.rs",
    ] {
        let path = routes_dir.join(expected.replace('/', std::path::MAIN_SEPARATOR_STR));
        assert!(path.is_file(), "fixture moved: {expected}");
        assert!(
            test_only_files.contains(&path),
            "{expected} should be derived as test-only"
        );
    }
}

#[test]
fn routes_do_not_depend_on_infrastructure_repositories_or_raw_sql() {
    let routes_dir = crate_root().join("src").join("routes");
    let mut files = Vec::new();
    collect_rs_files(&routes_dir, &mut files);

    // 测试专属文件从 `#[cfg(test)] mod <name>;` 声明推导，不再手工维护文件名白名单。
    let test_only_files = collect_cfg_test_module_files(&routes_dir);

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
        if test_only_files.contains(&file) {
            continue;
        }
        let rel = file.strip_prefix(&routes_dir).expect("file under routes");

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
