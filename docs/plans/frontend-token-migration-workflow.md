# 前端 Token 迁移工作流

信号面 token（`--face-*` / `--ink-*` / `--act` 四声）落地到生产 CSS 的操作手册。理念与判定见 `docs/architecture/SIGNAL_SURFACE.md`；色值真值见 `frontend/signal-surface-preview.html`。

---

## 1. 背景与目标

前端曾经同时活着几套皮肤：运营台、管理页、工作台、AI 页各说各的蓝。`theme-tokens.css`、`apple-theme.css`、`workspace_unified_theme.css`、`variables.css` 是债务层，不是第二套语言。

目标：chrome（面、墨、控件、焦点）全部直用信号面标本名。域内事态色（航班状态、派工状态）可以留 `--status-*` / `--tbl-row-*`，不升进根。新代码禁止再发明第三套。

---

## 2. Token 映射速查表

### 面 / 墨 / 声

| 旧名 | 标本 | 备注 |
|---|---|---|
| `--ws-bg` / `--ws-bg-soft` / `--ws-surface-muted` | `--face-page` | 页底、弱面 |
| `--ws-surface` / `--ws-bg-card` / `--bg-card` | `--face-work` | 工作面；表头不要另换色 |
| `--ws-surface-strong` | `--face-raised` | 抬起面 |
| `--ws-border` / `--border-light` | `--line` | 分割用弱线 |
| `--ws-border-strong` | `--line-strong` | 控件轮廓 |
| `--ws-text` / `--ws-text-primary` / `--text-primary` | `--ink` | |
| `--ws-text-subtle` / `--text-secondary` | `--ink-subtle` | |
| `--ws-text-muted` / `--text-tertiary` | `--ink-muted` | |
| `--text-inverse` | `--ink-inverse` / `--act-on` | 实底上的字用 `-on` |
| `--ws-primary` / `--ws-accent` / `--system-blue` | `--act` | 唯一行动蓝 |
| `--ws-accent-soft` / `--system-blue-subtle` | `--act-soft` | |
| `--ws-success` / `--system-green` | `--ok` | |
| `--ws-warn` / `--system-orange` | `--warn` | |
| `--ws-danger` / `--system-red` | `--danger` | 没有第五声，禁止紫/青 |

没有 `--face-muted`。弱面用 `--face-page`，不要再开第四级面。

### 形 / 距 / 字 / 影

| 旧名 | 标本 |
|---|---|
| `--ws-radius-xs` | `--r-cell` (6px) |
| `--ws-radius-sm` / `--ws-radius-control` | `--r-control` (8px) |
| `--ws-radius-md` / `--ws-radius-lg` / `--ws-radius-xl` | `--r-panel` (10px) |
| `--ws-space-1` / `--spacing-xs` | `--s1` (4px) |
| `--ws-space-2` / `--spacing-sm` | `--s2` (6px) |
| `--ws-space-3` / `--spacing-md` | `--s3` (12px) |
| `--ws-space-4` / `--spacing-lg` | `--s4` (20px) |
| `--ws-space-5` / `--spacing-xl` | `--s5` (28px) |
| `--ws-control-h` | `--h-md` (36px) |
| `--font-size-base` / 13px 正文 | `--fs-body` |
| 12px 标 | `--fs-label` |
| `--ws-shadow-sm` | `--shadow-sm` |
| `--ws-shadow-md` | `--shadow-md` |

`--ws-space-2` 旧值 8px、`--s2` 是 6px：这是密度校正，不是疏漏。

### 兑色写法（替代固定 rgba）

```css
/* 悬停洗底 */
background: color-mix(in srgb, var(--ink) 8%, transparent);

/* 焦点环 */
box-shadow: 0 0 0 4px var(--act-soft);
outline: 2px solid var(--act);

/* 字影（深浅两面都跟页底走，不必 --shadow-text-highlight） */
text-shadow: 0 1px 1px color-mix(in srgb, var(--face-page) 55%, transparent);
```

黑兑透明在夜色面上等于没有。禁止 `rgba(0,0,0,x)` 做悬停/衬底。

---

## 3. 五步法详解

### 3.1 先清重复别名层

1. 列出卷内自定义属性定义（`--foo: …`）。
2. 全仓 `grep var(--foo)`。零消费 → 删定义。有消费 → 改消费为标本名，再删定义。
3. `theme-tokens.css` 的 `--ws-*` 改成 `var(--face-page)` 这类别名，给尚未迁完的消费方当 fallback。`--system-*` 保留，注释写 `deprecated, use signal tokens`。
4. 不要在页面 `:root` 再定义一份 hex。

### 3.2 分区梯度替换

按「最长名优先」批量替换，避免 `--ws-surface` 先下手把 `--ws-surface-muted` 切碎。

顺序建议：surface-muted / text-subtle / primary-strong / radius-* / space-* / 单音节（`--ws-bg`、`--ws-text`）。

本地覆盖块（如 data-hub 上重定义 `--ws-radius-lg: 18px`）必须删，不能改写成 `--r-panel: 18px`——那会在该子树里把标本梯子改歪，还可能写出重复属性。

### 3.3 小步提交

一卷或一页做完立刻：

```powershell
cd frontend\vue-app
npm run build
npm run test
```

不要攒十个文件再验。CSS 静默失败比 JS 更危险。

### 3.4 闸门守护

- [ ] `npm run build` 绿（chunk size 警告非阻断）
- [ ] `npm run test` 绿（当前基线 589/589，89 files）
- [ ] 目标文件 `grep --ws-` 计数为 0（注释里也不要写这个前缀，以免误计）
- [ ] 没有新的硬编码色进 chrome
- [ ] e2e 若断言类名，先保护再换件

### 3.5 采样浏览器核验

用 `.tmp/acc_shots.cjs` 或 Playwright 对同一页切 `data-theme`：

1. kpi_dashboard
2. user_manager
3. resource_manager
4. ontology_center
5. ai_config_center
6. command_center

每页 dark / light 各一帧。看：主按钮对比、表头是否还换了一层灰帽子、进度条是否在夜色面上消失、焦点环是否跟得上行动蓝。

---

## 4. 常见坑与解决方案

### PostCSS 注释语法错误（`#hex` vs `/* */`）

CSS 里用 `#edf2f8` 当行注释会把后面整行吃掉，或被当成颜色。注释只用 `/* … */`。不要写 `# hex is the old value`。

### Vue scoped 内层 svg 不继承

scoped 属性打在组件根上，内层 `<svg>` 没有 `data-v-xxx`。要改图标对齐，打 `.svg-icon` 包裹层，不要打 `svg`。库件已有 `vertical-align: -0.125em`，页内不要再写 `style="vertical-align:-2px"`。

### E2E spec 类名依赖保护

迁整卷前先 grep Playwright spec。被断言的类（`.ontology-pill.tone-ok`）保留形只换声；没被断言的私造按钮类才换 `UiButton`。`--ws-bg` 若被 e2e 读 computedStyle，theme-tokens 里留别名指向 `--face-page`，不要直接删定义。

### 批量替换把局部覆盖写坏

`--ws-radius-md: 14px; --ws-radius-lg: 18px` 经映射会变成 `--r-panel: 14px; --r-panel: 18px`。后一条生效，梯子被改成 18。替换后立刻搜 ` --r-panel:` / `--h-md:` 定义，不属于 `:root` 的删掉。

### 选择器列表孤儿括号

删掉列表末行（带 `{`）后，前一行尾逗号会变成孤立 `}`，浏览器丢弃整条规则。深度扫描负括号；删末行时把前一行逗号改成 `{`。

### 实底上的图标只洗一面

夜色面 `brightness(0) saturate(100%)` 把图标洗成 `--act-on`；白天面要补 `brightness(0) invert(1)`。只写一面，浅色实底上就是黑图标。

---

## 5. PR Checklist

- [ ] `npm run build` 绿
- [ ] 589 tests 绿（或新基线数字，写进 PR）
- [ ] no new hard-coded colors in chrome
- [ ] 目标卷 `grep --ws-` / `grep --spacing-md` 为 0
- [ ] dark / light 截图对比（至少一页运营台 + 一页管理）
- [ ] 若换了库件，同名单测的 DOM 断言已改 `data-tone` / `data-variant`

---

## 6. 本轮已完成的卷

| 卷 | 结果 |
|---|---|
| `workspace_unified_theme.css` | `--ws-*` 消费清零；共享 chrome hex 收进标本；`--nl-*` 留给 nl-query 壳 |
| `components.css` | `--ws-*`、`--spacing-*` 清零 |
| `tables.css` | 表面/悬停/线/字改标本；事态行色仍走 `--tbl-row-*` |
| `variables.css` | 删除 `--spacing-xs..xl`；`--system-*` 标 deprecated |
| `theme-tokens.css` | `--ws-*` 改为标本别名；`--system-*` 标 deprecated |
| `apple-theme.css` | 直用 `--act` / `--face-*` / `--scrim`，scrollbar 与 glow 去黑兑透明 |
| `dispatch-board.css` | hex/rgba 已为 0 |
| `flowable-modeler.css` | 已直用信号面 |
| kpi / command / resource / admin 页 | 已用 `UiPill` / `data-tone`；行内 style 只剩宽度百分比与实体色 |
