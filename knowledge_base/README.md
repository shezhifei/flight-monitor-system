# 知识库目录说明

此目录用于存放 AI 处置建议工具（`AdvisorToolExecutor`）可检索的参考文件。

> 基线更新时间：2026-02-08

## 1. 当前实现行为

- 知识库读取组件：`src/infrastructure/ai/tools/advisor_tool_executor.py` 的 `SimpleKnowledgeBase`
- 基础路径：`knowledge_base`（在 `src/di/container.py` 中以 `SimpleKnowledgeBase(base_path="knowledge_base")` 注入）
- 扫描方式：递归扫描目录下所有支持后缀文件
- 检索方式：按文件名关键词匹配（非向量检索）

## 2. 目录建议

```text
knowledge_base/
├── sop/           # 标准操作规程（SOP）
├── cases/         # 历史案例
├── policies/      # 规范制度（可选）
└── README.md
```

> 当前代码会递归扫描，不强制子目录名称。

## 3. 支持文件格式

与代码保持一致的后缀集合：

- `.pdf`
- `.docx`
- `.xlsx`
- `.md`
- `.txt`

## 4. 使用方式

1. 将文件放入 `knowledge_base/` 或其子目录。
2. 触发 AI 建议工具时，系统按文件名关键词匹配参考资料。
3. 若命中，响应中会包含匹配到的文件名列表；若未命中，退回内置案例模板。

## 5. 注意事项

- 文件名建议语义化（如：`航班延误处置规程_v2.pdf`）。
- 上传前请做好脱敏处理，不要放置明文敏感信息。
- 当前不解析文档全文，仅按文件名检索；可通过后续升级引入全文索引/向量检索。
