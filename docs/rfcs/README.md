# Waypost RFC

RFC 定义可以据以实现和验证的公开行为：目的、范围、契约、状态转换、失败语义、性质、替代方案和演进方式。

| RFC | 状态 | 配套资料 |
|---|---|---|
| [0001：异步信箱与记忆交换](0001-async-mailbox-and-memory-sync.md) | Draft / implementation baseline | [性质与验证计划](0001-properties.md) |
| [0002：管理 Dashboard 与内容检查](0002-operator-dashboard.md) | Proposed / Step 2 | [Rust 数据模型](../design/0001-rust-data-model.md) |

## 变更约定

- `Draft` 表示已经写出实现基线，尚未通过实际互操作验证；不是已运行产品。
- 核心操作和错误行为不得靠实现者猜测；遇到规范冲突先修正文档并保留变更理由。
- 首次正式发布前可调整草案，但规范、Skill、schema、示例与测试必须一起更新。
- 发布后改变可观察行为或权限含义的变更另写 RFC，明确版本和迁移方式。
- 引用稳定版本的一手规范；外部事实、借鉴项和本项目自己的选择分别表达。
- 完成实现后，以性质和验收结果判断可用性；不能仅凭格式检查把 RFC 标为已实现。
