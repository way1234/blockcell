# Skills Development Rules

> Last updated: 2026-03-15
> Scope: all new skills in blockcell

## 1. 总体规范

一个 skill 由 3 层信息组成：

1. `meta.yaml`
2. 根 `SKILL.md`
3. 可选脚本文件：`SKILL.rhai` 或 `SKILL.py`

复杂 skill 可以把说明拆到多个子 `.md` 文件，但运行时入口永远只有根 `SKILL.md`。

## 2. skill 类型选择

### Prompt Skill

适合：

- 主要靠 `SKILL.md` 和受控工具完成任务
- 不需要脚本级确定性编排

规范见：

- [01-prompt-only-skill-development.md](01-prompt-only-skill-development.md)

### Rhai Skill

适合：

- 需要确定性工具编排
- 主要依赖 blockcell 工具能力
- 不想引入 Python 运行时和第三方依赖

规范见：

- [02-rhai-skill-development.md](02-rhai-skill-development.md)

### Python Skill

适合：

- 需要外部 SDK、HTTP 客户端、复杂解析
- 需要把协议和数据清洗封装进脚本

规范见：

- [03-python-skill-development.md](03-python-skill-development.md)

## 3. `meta.yaml` 规范

`meta.yaml` 只承载最小元数据：

```yaml
name: weather
description: 查询天气并整理结果
triggers:
  - 天气
tools:
  - web_fetch
permissions: []
requires:
  bins: []
  env: []
fallback:
  strategy: degrade
  message: 当前无法完成天气查询，请稍后重试。
```

字段职责：

- `name`：skill 名称
- `description`：一句话描述
- `triggers`：路由触发词
- `tools`：Prompt/Rhai 的工具白名单
- `permissions`：权限声明
- `requires`：运行时依赖
- `fallback`：失败时的默认用户提示
- `always` / `output_format`：仅在确实需要时使用

## 4. `SKILL.md` 规范

根 `SKILL.md` 是技能调用说明书，也是结果整理说明书。

它必须覆盖：

1. 什么时候应该使用这个 skill
2. 哪些输入直接执行，哪些输入先澄清
3. 参数和默认值如何构造
4. 结果如何整理

## 5. 多 `.md` 文档设计

复杂 skill 可以拆成：

```text
skills/<skill_name>/
├── meta.yaml
├── SKILL.md
├── manual/
│   ├── planning.md
│   └── summary.md
└── SKILL.rhai | SKILL.py
```

根 `SKILL.md` 使用标准 markdown 链接引用子文档：

```md
## Planning {#planning}
- [参数构造规则](manual/planning.md#build-argv)
- [默认值规则](manual/planning.md#defaults)

## Summary {#summary}
- [结果整理规则](manual/summary.md#final-answer)
```

子文档用显式 section id：

```md
## 参数构造规则 {#build-argv}
...

## 默认值规则 {#defaults}
...
```

## 6. 链接解析规则

运行时只解析根 `SKILL.md` 中的本地 markdown 链接。

解析规则：

- 只解析根 `SKILL.md`
- 子 `.md` 里的链接不递归解析
- 只允许相对路径 `.md` 链接
- 路径 canonicalize 后必须仍在当前 skill 目录内
- `a.md#summary` 表示提取 `summary` 对应 section
- `a.md` 表示提取整份子文档
- section 范围为：目标标题开始，到下一个同级或更高层级标题之前
- section 优先匹配显式 id，如 `{#summary}`
- 找不到路径或 section 时，视为 skill 文档错误

## 7. 根 `SKILL.md` 的保留章节

根 `SKILL.md` 使用以下保留章节作为运行时文档入口：

- `## Shared {#shared}`
- `## Prompt {#prompt}`
- `## Planning {#planning}`
- `## Summary {#summary}`

规则：

- `Shared`：所有阶段共享规则
- `Prompt`：Prompt Skill 注入内容
- `Planning`：脚本 skill 的动作和参数构造规则
- `Summary`：脚本结果整理规则

每个保留章节中可以同时包含：

- 章节正文
- 指向子文档的 markdown 链接

运行时会按出现顺序拼接正文和被引用 section。

## 8. 缓存设计

### 8.1 `meta.yaml` 缓存

`meta.yaml` 在 skill 扫描或 reload 时读取、解析并缓存到内存中的 skill 对象。

缓存内容：

- 解析后的 `SkillMeta`
- 可用性检查结果
- 当前 skill 路径和版本信息

### 8.2 文档缓存

每个 skill 的文档缓存按 skill 目录整体维护，缓存内容应包括：

- 根 `SKILL.md` 原文
- 被引用子 `.md` 原文
- 子文档 section 索引
- 拼接后的阶段 bundle

推荐缓存 bundle：

- `prompt_bundle`
- `planning_bundle`
- `summary_bundle`

### 8.3 失效策略

任意 skill 目录文件改动后，整 skill 文档缓存整体失效并重建。

重建范围：

- `meta.yaml`
- 根 `SKILL.md`
- 根 `SKILL.md` 引用到的子 `.md`
- 阶段 bundle

不做子文档递归增量失效。

## 9. 运行时如何使用这些 bundle

- Prompt Skill：使用 `shared + prompt`
- Rhai Skill 参数构造：使用 `shared + planning`
- Rhai Skill 结果整理：使用 `shared + summary`
- Python Skill 参数构造：使用 `shared + planning`
- Python Skill 结果整理：使用 `shared + summary`

## 10. 历史、测试、cron

统一规则：

- 主历史持久化完整 tool 链路
- follow-up 只依赖主历史
- WebUI 技能测试走同一套 skill kernel
- cron 技能任务走同一套 skill kernel

skill 设计时要默认：

- 脚本结果里保留足够的结构化标识
- `SKILL.md` 的 `Summary` 章节明确哪些字段可展示，哪些只用于内部整理

## 11. 作者检查清单

- `meta.yaml` 只包含最小元数据
- 根 `SKILL.md` 是唯一入口
- 子文档只通过根 `SKILL.md` 链接暴露给运行时
- 链接全部是 skill 目录内相对 `.md` 路径
- section 都有稳定显式 id
- 规划和总结规则已拆清
- WebUI 测试和 cron 都不需要额外协议
