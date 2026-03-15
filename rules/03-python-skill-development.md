# Python Skill Development

## 1. 什么时候用

Python Skill 适合：

- 需要外部 SDK
- 需要复杂 HTTP/API 协议处理
- 需要 HTML 解析、数据清洗、网页抓取
- 已有成熟 Python 代码可以直接复用

不适合：

- 只是简单工具编排
- 只是想把提示词写成脚本

## 2. 目录结构

```text
skills/<skill_name>/
├── meta.yaml
├── SKILL.md
├── manual/
│   ├── planning.md
│   └── summary.md
├── SKILL.py
└── tests/
```

`tests/` 对新 Python skill 视为标配。

## 3. `meta.yaml` 写法

Python Skill 的 `meta.yaml` 只放最小元数据。

推荐：

```yaml
name: xiaohongshu
description: 搜索和整理小红书结果
triggers:
  - 小红书
  - 红薯搜索
requires:
  bins:
    - python3
fallback:
  strategy: degrade
  message: 当前无法完成这次搜索，请稍后重试。
```

参数和动作规则统一写进 `SKILL.md`。

## 4. `SKILL.md` 写法

Python Skill 的根 `SKILL.md` 必须写清：

1. 用户意图如何映射成脚本动作。
2. 参数如何构造，默认值是什么。
3. 结果整理规则。
4. 哪些内部字段不能暴露给用户。

推荐重点：

- 把参数构造规则写具体。
- 把不可暴露字段写明确。
- 给 2 到 5 个真实示例。

推荐结构：

```md
# <skill name>

## Shared {#shared}
- skill 范围
- 输入约束

## Planning {#planning}
- [动作映射](manual/planning.md#actions)
- [参数构造规则](manual/planning.md#build-argv)

## Summary {#summary}
- [结果整理规则](manual/summary.md#final-answer)
- [敏感字段过滤](manual/summary.md#redaction)
```

## 5. `SKILL.py` 脚本职责

Python 脚本应该负责：

- 外部 API/SDK 调用
- 数据清洗
- 错误归一化
- 组织可总结的原始结果

## 6. 输入输出规范

### 6.1 输入

Python Skill 以显式参数为主。

这意味着：

- 脚本入口要按明确参数设计。
- 需要的默认值和构造规则写在 `SKILL.md`。
- 规划阶段只根据 `planning_bundle` 生成脚本参数。

### 6.2 输出

stdout 只输出最终结果。

推荐输出 JSON：

```json
{
  "display_text": "可直接给用户的简短结果",
  "data": {},
  "refs": {}
}
```

规则：

- `display_text`：可选，适合直接展示。
- `data`：用户可理解的数据。
- `refs`：内部结构化标识。
- stderr：日志、调试、诊断。

## 7. 运行时行为

Python Skill 的运行时 bundle 为：

- 参数构造：`shared + planning`
- 结果整理：`shared + summary`

Python Skill 的统一流程是：

1. 命中 skill。
2. runtime 读取并拼接 `planning_bundle`，构造这次脚本参数。
3. 执行 `SKILL.py`。
4. 把脚本原始结果写入主历史。
5. 用 `summary_bundle` 整理最终回复。

因此：

- 后续多轮理解质量取决于原始结果里是否保留了足够的结构化标识。
- 这些标识可以保留在 `refs` 或等价字段里，但不要直接展示给用户。

## 8. 测试要求

Python Skill 至少要有：

1. 一个成功用例。
2. 一个失败用例。
3. 一个 stdout 无污染用例。
5. 根 `SKILL.md` 中的文档链接和 section 都能解析成功。

推荐再加：

- 外部 API mock
- 参数边界测试
- 非法输入测试

## 9. 常见错误

- 根 `SKILL.md` 没有 `#planning` / `#summary` 章节。
- 子文档 section 没有稳定 id。
- stdout 里同时打印日志和 JSON。
- 原始结果里完全没有结构化标识。
- `SKILL.md` 只写结果格式，不写参数构造规则。
- 本来只需要 Rhai，却直接上 Python。
