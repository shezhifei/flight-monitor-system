# 文档维护流程

文档基线：**2026-08-11**。代码变了就按这页同步文档。事实来源表见 `docs/SOURCE_OF_TRUTH.md`。

## 1. 什么时候要改文档

- API 路由、参数、权限
- 启动参数、端口、环境变量、Vault、部署方式
- 表结构、迁移策略
- AI / SSE / Flowable / MQ / 权限等核心能力
- 前端页面路径或静态资源策略
- 文档体系本身（增删主文档、调整基线）

## 2. 流程

1. 查 `docs/SOURCE_OF_TRUTH.md`，定位源码/脚本。
2. 按下面映射表列出要改的文档。
3. **同一次变更**里改代码和文档。
4. 过期页：删除、并入主文档，或标成历史资料。
5. 自检：命令可跑、路径存在、端点真实、迁移号正确。
6. PR/提交说明写清改了哪些文档。

## 3. 变更 → 文档映射

| 变更类型 | 必改 | 常改 |
|---|---|---|
| 启动 / compose / host 脚本 | `README.md`, `QUICK_START.md`, `docs/DEPLOYMENT.md`, `docs/SOURCE_OF_TRUTH.md` | `docs/SYSTEM_MANUAL.md` |
| API 路由 / 权限 | `docs/API_ROUTE_SNAPSHOT.md` | `README.md`, `docs/SYSTEM_MANUAL.md` |
| Vault / 密钥 | `docs/DEPLOYMENT.md`, `docs/SOURCE_OF_TRUTH.md` | `QUICK_START.md` |
| 迁移 / schema | `README.md`, `QUICK_START.md`, `docs/DEPLOYMENT.md` | `docs/SYSTEM_MANUAL.md` |
| 分层 / 领域边界 | `docs/SYSTEM_MANUAL.md`, `docs/SOURCE_OF_TRUTH.md` | `docs/architecture/*` |
| AI 侧车 / NL Query / Eval | `docs/API_ROUTE_SNAPSHOT.md`, `docs/SYSTEM_MANUAL.md` | `docs/SOURCE_OF_TRUTH.md` |
| 前端入口 | `README.md`, `QUICK_START.md`, `docs/SOURCE_OF_TRUTH.md` | `docs/operations/frontend-parity-audit.md`（本地） |
| 文档基线调整 | `README.md`, 本文件, `docs/SOURCE_OF_TRUTH.md` | `CLAUDE.md` |

## 4. 入库范围（基线）

**应跟踪的产品文档：**

- 根目录：`README.md`、`QUICK_START.md`、`CLAUDE.md`
- `docs/`：`SYSTEM_MANUAL.md`、`DEPLOYMENT.md`、`API_ROUTE_SNAPSHOT.md`、`SOURCE_OF_TRUTH.md`、`DOCUMENTATION_WORKFLOW.md`、`GLOSSARY.md`
- `docs/architecture/`、`docs/observability/`
- 少量长期 ops 笔记与技术债主计划（见 `.gitignore` 白名单）
- 代理交接：`docs/operations/agent-handoff.md`

**默认不入库：**

- 一次性计划、设计草稿、token/codemod 报告（`docs/plans/*` 多数）
- 审计提示词、阶段报告、superpowers 草稿
- Agent 本地目录（`.agents/`、`.claude/`、`.codex/`、`.opencode/`、`.shared/` 等）
- 运行时数据、证书、构建产物

本地可以继续写计划稿；需要共享时再决定是否升格进基线。

## 5. 写法

- 术语跟 `docs/GLOSSARY.md`。
- 同一细节只维护一处，别处链接。
- 默认路径写 Rust API + Vault + Vue 正式页；兼容入口标明 legacy。
- Python 命令写 `.\.venv\Scripts\python.exe`。
- 少用口号式句子；写清路径、命令、约束即可。
- 不要把 Wave/任务编号、agent 会话记录写进产品文档正文。

## 6. PR 检查清单

```text
### Documentation Checklist
- [ ] 已按 docs/SOURCE_OF_TRUTH.md 核对事实
- [ ] 已更新受影响基线文档
- [ ] 过期内容已删/并/标注
- [ ] 命令、路径、端点已核对
- [ ] 迁移最新编号已更新（若相关）
- [ ] 路由变更已同步 docs/API_ROUTE_SNAPSHOT.md
- [ ] 术语符合 docs/GLOSSARY.md
```

## 7. 发布前

- 扫一遍旧端口、旧路径、旧迁移号、已删文档名（`tests/tools/test_docs_no_stale_references.py` 等）
- 补不齐的文档债写明影响与后续位置
