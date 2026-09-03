//! application 层「越过 domain 数据端口直连数据库」的守门。
//!
//! 扫描必须先剥离注释、字符串字面量与 `#[cfg(test)]` 模块再匹配。早期版本对原始文本做
//! `source.contains(pattern)`，松散子串 `Postgres` 因此命中 8 个纯文档注释文件，使唯一
//! 严格的守门看起来「还差得很远」，只能长期 `#[ignore]`。见
//! docs/architecture/TECH_DEBT_INVENTORY_2026-09-02.md D-28。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEBT_PATTERNS: &[&str] = &[
    "sqlx::query",
    "sqlx::query_as",
    "sqlx::query_scalar",
    "fms_infrastructure::repositories",
    "PgPool",
    "Transaction<",
    "Postgres",
    // `Sqlx` 前缀曾是 10 个别名 trait（Sqlx*TransactionalRepository）的专属标记，那些
    // trait 把 `Transaction<'tx, Postgres>` 从签名里藏起来，绕过 `Postgres` / `Transaction<`
    // 的扫描。别名 trait 已随生产 sqlx 依赖一并删除；模式保留，防止同类变通回流。
    "Sqlx",
    // application 层同样不得直连缓存与逻辑复制客户端：这是 sqlx 之后的同类违规（D-05）。
    "redis::",
    "pgwire_replication::",
];

/// 生产代码中仍然合法的越界文件——**只能变短**，删除时同步改这里。
///
/// `services/domain_event_cdc_relay_service.rs` 把 pgwire-replication 的复制流客户端与
/// pgoutput 解码器直接握在 application 层。消除它需要把逻辑复制流下沉成 domain port、
/// 并将 TLS/LSN/解码移到 infrastructure；该改动无法在无复制槽的环境中验证，因此写成
/// 显式基线常驻守门，而不是把守门重新 ignore 掉。
const PRODUCTION_DEBT_BASELINE: &[&str] = &["services/domain_event_cdc_relay_service.rs"];

/// `src/services` 下仍越过 domain 数据端口的活清单（含仅在测试构建中编译的文件）。
/// 只能变短：P3 每下沉一个服务就删掉一行，新增一行即新增债务。
const SERVICE_DEBT_INVENTORY: &[&str] = &[
    "domain_action_executor/tests.rs",
    "domain_action_executor/tests_terminal_equipment.rs",
    "domain_event_cdc_relay_service.rs",
    "flight_runtime_service/tests.rs",
];

#[test]
fn application_services_boundary_debt_inventory_matches_baseline() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let services_dir = manifest_dir.join("src").join("services");

    let mut actual = Vec::new();
    collect_debt_files(&services_dir, &services_dir, &mut actual);
    actual.sort();

    let expected = SERVICE_DEBT_INVENTORY
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "application service boundary debt inventory changed");
}

#[test]
// 此前被 `#[ignore]`，理由写着「P3 未完成：application 层仍有 20 个文件持有 sqlx 类型」。
// 那个数量是扫描器把文档注释当代码算出来的虚数，真实的越界生产文件只有 1 个。忽略一条
// 会误报的守门是把问题冻结；修正扫描器、把剩余真实债务写成只能变短的基线，守门才能常驻。
fn production_application_source_does_not_bypass_domain_data_ports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let application_src = manifest_dir.join("src");
    let test_only = collect_cfg_test_module_files(&application_src);

    let mut violations = Vec::new();
    collect_production_debt_files(&application_src, &application_src, &test_only, &mut violations);
    violations.sort();
    let expected = PRODUCTION_DEBT_BASELINE
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();

    assert_eq!(
        violations, expected,
        "production application source must use fms_domain data ports"
    );
}

/// 仍常驻 application `[dependencies]` 的数据面客户端（只能变短）。
///
/// 与 `PRODUCTION_DEBT_BASELINE` 是同一笔债：`domain_event_cdc_relay_service.rs` 把
/// pgwire 逻辑复制流握在 application 层，下沉成 port 需要在有复制槽的环境里验证，
/// 因此先显式记账，而不是把守门 ignore 掉。
const ALLOWED_DATA_PLANE_DEPS: &[&str] = &["pgwire-replication"];

#[test]
fn application_cargo_dependencies_do_not_include_data_plane_clients() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("read application Cargo.toml");

    // 与 api crate 的同类断言对齐（crates/api/tests/layer_boundary_guard.rs）。
    // 源码扫描能被 `use` 重命名绕过，Cargo 依赖面不能。
    let keys = dependency_keys(&cargo_toml);
    let forbidden = ["fms-infrastructure", "sqlx", "redis", "pgwire-replication"];
    let violations = keys
        .iter()
        .filter(|key| forbidden.iter().any(|f| **key == *f))
        .cloned()
        .collect::<Vec<_>>();
    let expected = ALLOWED_DATA_PLANE_DEPS
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();

    assert_eq!(
        violations, expected,
        "application [dependencies] must not add data-plane clients beyond the baseline"
    );

    // 基线条目必须仍然真实存在；删掉依赖时要同步删掉这一行，不能留下失效豁免。
    for allowed in ALLOWED_DATA_PLANE_DEPS {
        assert!(
            keys.contains(allowed),
            "baseline entry `{allowed}` is no longer a dependency; remove it from ALLOWED_DATA_PLANE_DEPS"
        );
    }
}

/// 全部 6 个 crate 生产源码中 `Option<Arc<dyn …>>` 的存量基线——**只能调小**。
///
/// 结构债计划 P1 曾把 DispatchService 的 26 处清零（`74c0dd2`），但该模式随后在别处
/// 回潮且全仓无守门（TECH_DEBT_INVENTORY D-29）。依赖注入字段应是必选构造参数，
/// 而不是「可以不装配」的 `Option`；清理后同步调低这个数字，新增一处即失败。
const OPTION_ARC_DYN_BASELINE: usize = 113;

#[test]
fn workspace_production_source_option_arc_dyn_ratchet() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crates_dir = manifest_dir.parent().expect("application crate sits under crates/");

    let mut per_file = Vec::new();
    for entry in fs::read_dir(crates_dir).expect("read crates directory") {
        let crate_src = entry.expect("crate entry").path().join("src");
        if !crate_src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_rs_files(&crate_src, &mut files);
        for file in files {
            let source = fs::read_to_string(&file).expect("read source file");
            let count = count_token_sequence(
                &strip_comments_and_strings(&source),
                &["Option", "<", "Arc", "<", "dyn"],
            );
            if count > 0 {
                per_file.push((relative_path(crates_dir, &file), count));
            }
        }
    }
    per_file.sort();

    let total: usize = per_file.iter().map(|(_, count)| count).sum();
    let breakdown = per_file
        .iter()
        .map(|(path, count)| format!("  {path}: {count}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        total <= OPTION_ARC_DYN_BASELINE,
        "Option<Arc<dyn …>> occurrences grew: {total} > baseline {}.\n\
         Replace with required constructor parameters (see D-29); after cleanup, \
         lower OPTION_ARC_DYN_BASELINE.\n{breakdown}",
        OPTION_ARC_DYN_BASELINE
    );
}

fn collect_debt_files(root: &Path, current: &Path, actual: &mut Vec<String>) {
    scan_tree(root, current, &BTreeSet::new(), false, actual);
}

fn collect_production_debt_files(root: &Path, current: &Path, test_only: &BTreeSet<PathBuf>, actual: &mut Vec<String>) {
    scan_tree(root, current, test_only, true, actual);
}

fn scan_tree(
    root: &Path,
    current: &Path,
    test_only: &BTreeSet<PathBuf>,
    production_only: bool,
    actual: &mut Vec<String>,
) {
    let mut entries = fs::read_dir(current)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", current.display()))
        .map(|entry| entry.unwrap_or_else(|err| panic!("failed to read directory entry: {err}")))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            // tests/ 目录与 `#[cfg(test)] mod tests;` 声明的整棵子树都在 test_only 里。
            if production_only && path.file_name().and_then(|name| name.to_str()) == Some("tests") {
                continue;
            }
            scan_tree(root, &path, test_only, production_only, actual);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        if production_only
            && (test_only.contains(&path) || path.file_name().and_then(|name| name.to_str()) == Some("tests.rs"))
        {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let production = production_source(&source);
        if DEBT_PATTERNS.iter().any(|pattern| production.contains(pattern)) {
            actual.push(relative_path(root, &path));
        }
    }
}

/// 只留下会进生产构建的代码文本：先剥注释与字符串，再移除 `#[cfg(test)]` 模块体。
///
/// 顺序很重要——先剥注释，`strip_test_modules` 的花括号配对才不会被注释或字符串里的
/// `{`/`}` 干扰；反过来则可能把生产代码整段误删。
fn production_source(source: &str) -> String {
    strip_test_modules(&strip_comments_and_strings(source))
}

enum Scan {
    Code,
    LineComment,
    BlockComment,
    String,
}

/// 单遍扫描：移除 `//` 行注释、可嵌套的 `/* */` 块注释、以及字符串字面量内容。
///
/// 刻意**不**处理单引号：Rust 里 `'a` 绝大多数是生命周期而非字符字面量，按字符字面量
/// 解析会把 `Transaction<'tx, Postgres>` 吞掉，正好漏掉要抓的模式。
/// 原始字符串 `r#"…"#` 也不特殊处理：它只会导致少删（把内容当代码），偏保守不误放。
fn strip_comments_and_strings(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut state = Scan::Code;
    let mut block_depth = 0usize;
    let mut escaped = false;
    let bytes: Vec<char> = source.chars().collect();
    let mut index = 0;

    while index < bytes.len() {
        let ch = bytes[index];
        let next = bytes.get(index + 1).copied();

        match state {
            Scan::Code => {
                if ch == '/' && next == Some('/') {
                    state = Scan::LineComment;
                    index += 2;
                    continue;
                }
                if ch == '/' && next == Some('*') {
                    state = Scan::BlockComment;
                    block_depth = 1;
                    index += 2;
                    continue;
                }
                if ch == '"' {
                    state = Scan::String;
                    escaped = false;
                    index += 1;
                    continue;
                }
                output.push(ch);
                index += 1;
            }
            Scan::LineComment => {
                if ch == '\n' {
                    state = Scan::Code;
                    output.push('\n');
                }
                index += 1;
            }
            Scan::BlockComment => {
                if ch == '/' && next == Some('*') {
                    block_depth += 1;
                    index += 2;
                    continue;
                }
                if ch == '*' && next == Some('/') {
                    block_depth -= 1;
                    index += 2;
                    if block_depth == 0 {
                        state = Scan::Code;
                    }
                    continue;
                }
                if ch == '\n' {
                    output.push('\n');
                }
                index += 1;
            }
            Scan::String => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    state = Scan::Code;
                }
                index += 1;
            }
        }
    }

    output
}

fn strip_test_modules(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;

    while let Some(relative_attr_start) = source[cursor..].find("#[cfg(test)]") {
        let attr_start = cursor + relative_attr_start;
        let after_attr = attr_start + "#[cfg(test)]".len();
        let rest = &source[after_attr..];
        let module_start = after_attr + rest.len() - rest.trim_start().len();
        if !source[module_start..].starts_with("mod") {
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
            // 外部文件声明：该文件本身由 collect_cfg_test_module_files 归类为测试专属。
            output.push_str(&source[cursor..next_token + 1]);
            cursor = next_token + 1;
            continue;
        }
        if next_char != '{' {
            output.push_str(&source[cursor..after_attr]);
            cursor = after_attr;
            continue;
        }

        let block_start = next_token;
        output.push_str(&source[cursor..attr_start]);

        let mut depth = 0usize;
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

/// 把 `#[cfg(test)] mod <ident>;` 解析成磁盘上的测试专属文件集合。
///
/// 取代手工命名白名单：白名单会随新增测试文件静默失效（`ai_internal` 下新增的
/// `*_tests.rs` 就因为命名不等于 `tests.rs` 而漏进过扫描范围）。推导出的集合不会漂移。
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

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// 只取 `[dependencies]` 段的 key，`[dev-dependencies]` / `[build-dependencies]` 不算。
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

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or_else(|err| panic!("failed to strip root {} from {}: {err}", root.display(), path.display()))
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// 在去注释后的源码里做「空白容错」的 token 序列计数（如 `Option<Arc<dyn` 的
/// `Option < Arc < dyn` 写法也要命中）。命中后从匹配末尾继续，避免嵌套重复计数。
/// 按 ASCII 字节比较：源码含中文等多字节字符时不会踩到 char boundary panic。
fn count_token_sequence(source: &str, tokens: &[&str]) -> usize {
    let bytes = source.as_bytes();
    let mut count = 0;
    let mut i = 0;
    'outer: while i < bytes.len() {
        let mut pos = i;
        for (idx, token) in tokens.iter().enumerate() {
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if bytes[pos..].starts_with(token.as_bytes()) {
                pos += token.len();
                if idx == tokens.len() - 1 {
                    count += 1;
                    i = pos;
                    continue 'outer;
                }
            } else {
                i += 1;
                continue 'outer;
            }
        }
    }
    count
}

#[cfg(test)]
mod scanner_tests {
    use super::*;

    #[test]
    fn doc_comment_mentioning_a_pattern_is_not_a_violation() {
        let source = "//! Postgres implementation, so the pool is shared.\nfn ok() {}\n";
        let cleaned = production_source(source);
        assert!(!cleaned.contains("Postgres"), "stripped source was {cleaned:?}");
    }

    #[test]
    fn real_production_use_survives_stripping() {
        let source = "use sqlx::query;\nfn repo() -> PgPool { todo!() }\n";
        let cleaned = production_source(source);
        assert!(cleaned.contains("sqlx::query"));
        assert!(cleaned.contains("PgPool"));
    }

    #[test]
    fn string_literal_content_is_not_a_violation() {
        let source = "let url = \"postgres://user@host/db\";\n";
        let cleaned = production_source(source);
        assert!(!cleaned.contains("postgres"), "stripped source was {cleaned:?}");
    }

    #[test]
    fn lifetime_generic_arguments_are_not_swallowed_as_char_literals() {
        let source = "fn tx(&self) -> Transaction<'tx, Postgres> { todo!() }\n";
        let cleaned = production_source(source);
        assert!(cleaned.contains("Postgres"), "stripped source was {cleaned:?}");
    }

    #[test]
    fn inline_cfg_test_module_is_removed_but_production_kept() {
        let source = "fn prod() {}\n#[cfg(test)]\nmod tests {\n    use sqlx::PgPool;\n}\n";
        let cleaned = production_source(source);
        assert!(cleaned.contains("fn prod()"));
        assert!(!cleaned.contains("PgPool"), "stripped source was {cleaned:?}");
    }

    #[test]
    fn nested_block_comments_are_removed() {
        let source = "/* outer /* inner */ still comment */ fn prod() {}\n";
        let cleaned = production_source(source);
        assert!(!cleaned.contains("comment"), "stripped source was {cleaned:?}");
        assert!(cleaned.contains("fn prod()"));
    }

    #[test]
    fn line_comment_containing_block_comment_marker_does_not_swallow_code() {
        let source = "// see /* for details\nfn prod() -> PgPool { todo!() }\n";
        let cleaned = production_source(source);
        assert!(cleaned.contains("PgPool"), "stripped source was {cleaned:?}");
    }

    #[test]
    fn cfg_test_prefix_on_a_longer_identifier_is_not_treated_as_module() {
        // `#[cfg(test)]` 后跟 `mod tests;` 才是模块声明；`_cfg(test)` 之类噪声不应触发。
        let source = "let x = cfg_test_helper();\n#[cfg(test)]\nfn only_in_tests() {}\n";
        let cleaned = strip_test_modules(source);
        assert!(cleaned.contains("cfg_test_helper"), "stripped source was {cleaned:?}");
    }

    #[test]
    fn dev_dependencies_are_not_counted_as_production_dependencies() {
        let toml = "[dependencies]\ntokio = \"1\"\n\n[dev-dependencies]\nsqlx = \"1\"\n";
        let keys = dependency_keys(toml);
        assert!(keys.contains(&"tokio"));
        assert!(!keys.contains(&"sqlx"), "dev-dependency leaked into {keys:?}");
    }

    #[test]
    fn token_sequence_counts_regardless_of_inner_whitespace() {
        let source =
            "let a: Option<Arc<dyn T>> = x;\nlet b: Option < Arc < dyn T > > = y;\nlet c: Vec<Arc<dyn T>> = z;\n";
        assert_eq!(count_token_sequence(source, &["Option", "<", "Arc", "<", "dyn"]), 2);
        assert_eq!(count_token_sequence(source, &["Arc", "<", "dyn"]), 3);
    }

    #[test]
    fn token_sequence_is_case_sensitive_and_boundary_aware() {
        let source = "option < Arc < dyn\nOption<Arc<dyn>>\n";
        assert_eq!(count_token_sequence(source, &["Option", "<", "Arc", "<", "dyn"]), 1);
    }
}
