# Prompt Skill Development

## 1. 什么时候用

Prompt Skill 适合：

- 主要依赖 `SKILL.md` 约束模型行为
- 主要通过 blockcell 工具完成任务
- 不需要脚本级确定性编排

不适合：

- 多分支、重试、强流程控制
- 需要复杂外部协议处理
- 需要依赖第三方 SDK

## 2. 目录结构

```text
skills/<skill_name>/
├── meta.yaml
├── SKILL.md
└── manual/
    └── prompt.md
```

`manual/` 是可选目录。简单 skill 可以只保留一个根 `SKILL.md`。

## 3. `meta.yaml` 写法

推荐：

```yaml
name: travel_plan
description: 规划出行方案并整理建议
triggers:
  - 旅游攻略
  - 行程规划
tools:
  - web_search
  - read_file
fallback:
  strategy: degrade
  message: 当前无法完成这次出行规划，请稍后重试。
```

规则：

- `tools` 只写真正要用的工具。
- `triggers` 覆盖真实用户说法。
- `meta.yaml` 只承载最小元数据。

## 4. `SKILL.md` 应该怎么写

Prompt Skill 的根 `SKILL.md` 重点写 4 类规则：

1. 什么时候先澄清，什么时候直接执行。
2. 工具使用顺序或决策原则。
3. 最终输出格式。

推荐结构：

```markdown
# <skill name>

## Shared {#shared}
- 解决什么问题

## Prompt {#prompt}
- 缺什么先问
- 什么情况直接做
- 优先用什么工具
- 不要调用什么工具
- 最终答案的结构
- 不要暴露什么
```

复杂 skill 推荐把长规则拆到子文档，再通过标准 markdown 链接接入：

```md
## Prompt {#prompt}
- [澄清规则](manual/prompt.md#clarify)
- [工具规则](manual/prompt.md#tools)
- [输出规则](manual/prompt.md#output)
```

## 5. 运行时行为

Prompt Skill 的运行时文档 bundle 为：

- `shared + prompt`

Prompt Skill 的实际流程是：

1. 命中 skill。
2. runtime 读取并拼接 `prompt_bundle`。
3. runtime 只开放 `meta.yaml.tools` 里的工具。
4. 模型在受控工具范围内完成提问、调用工具、整理答案。
5. 完整 tool 链路写入主历史。

这意味着：

- 根 `SKILL.md` 是入口文档。
- 子文档只是根 `SKILL.md` 的可拆分扩展。
- 写得越空泛，运行效果越差。
- `tools` 越宽，模型越容易漂移。

## 6. 测试要求

至少验证：

1. 常见用户说法能命中 skill。
2. 参数不足时会先澄清。
3. 只会调用白名单工具。
4. 输出格式稳定。
6. 根 `SKILL.md` 中的文档链接都能解析到有效 section。

## 7. 常见错误

- 根 `SKILL.md` 没有显式 `#prompt` 章节。
- 子文档 section 没有稳定 id。
- `tools` 声明过多。
- 输出规则只有目标，没有格式。
- 需要脚本编排的 skill 仍然做成 Prompt Skill。
