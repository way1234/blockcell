# Rhai Skill Development

## 1. 什么时候用

Rhai Skill 适合：

- 需要确定性工具编排
- 需要分支、重试、降级
- 主要仍然依赖 blockcell 内置工具

不适合：

- 强依赖 Python 第三方库
- 复杂网页抓取、SDK 接入、重数据清洗

## 2. 目录结构

```text
skills/<skill_name>/
├── meta.yaml
├── SKILL.md
├── manual/
│   ├── planning.md
│   └── summary.md
└── SKILL.rhai
```

可选：

```text
skills/<skill_name>/tests/
```

## 3. `meta.yaml` 写法

Rhai Skill 使用最小元数据：

推荐：

```yaml
name: ai_news
description: 聚合新闻并做确定性整理
triggers:
  - AI 新闻
  - 科技快讯
tools:
  - web_fetch
  - read_file
fallback:
  strategy: degrade
  message: 当前无法完成新闻整理，请稍后重试。
```

## 4. `SKILL.md` 写法

Rhai Skill 的根 `SKILL.md` 重点是两件事：

1. 如何从用户请求构造这次调用的动作和参数。
2. 脚本执行完成后，如何整理结果。

必须写清：

- 默认值
- 缺参时是否允许追问
- 哪些结果字段可见
- 哪些内部字段不可见

推荐结构：

```md
# <skill name>

## Shared {#shared}
- skill 目标
- 输入边界

## Planning {#planning}
- [参数构造规则](manual/planning.md#build-argv)
- [默认值规则](manual/planning.md#defaults)

## Summary {#summary}
- [结果整理规则](manual/summary.md#final-answer)
- [字段过滤规则](manual/summary.md#redaction)
```

运行时会按顺序拼接根章节正文和被链接的 section。

## 5. `SKILL.rhai` 脚本职责

脚本应该负责：

- 确定性工具编排
- 分支和降级
- 错误处理
- 把原始结果组织成稳定结构

## 6. 输出约定

Rhai Skill 可以返回文本，也可以返回 JSON。

推荐返回 JSON，当需要 follow-up 时尤其如此。

推荐结构：

```json
{
  "display_text": "可直接给用户的简短结果",
  "data": {},
  "refs": {}
}
```

规则：

- `display_text`：可选，适合直接展示。
- `data`：用户可理解的结构化信息。
- `refs`：内部结构化标识。
- `refs` 中的标识字段由 `SKILL.md` 的 `Summary` 规则控制是否展示。

## 7. 运行时行为

Rhai Skill 的运行时 bundle 为：

- 参数构造：`shared + planning`
- 结果整理：`shared + summary`

Rhai Skill 的统一流程是：

1. 命中 skill。
2. runtime 读取并拼接 `planning_bundle`，规划本次脚本调用。
3. 执行 `SKILL.rhai`。
4. 把脚本原始结果写入主历史。
5. 用 `summary_bundle` 做结果整理。

这意味着：

- follow-up 靠主历史，不靠私有元数据。
- 脚本原始结果里的结构化标识直接影响后续多轮理解质量。

## 8. 测试要求

至少验证：

1. 主成功路径。
2. 至少一个失败分支。
3. 至少一个降级分支。
5. WebUI 测试和 cron 触发都能走通。
6. 根 `SKILL.md` 中的文档链接和 section 都能解析成功。

## 9. 常见错误

- 根 `SKILL.md` 没有 `#planning` / `#summary` 章节。
- 子文档 section 没有稳定 id。
- 把 Rhai skill 写成“提示词 skill + 一点脚本”。
- 原始结果里完全没有结构化标识。
- 返回太多内部字段，却没在 `SKILL.md` 里写清展示规则。
