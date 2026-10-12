# Waypost

Waypost 是供个人 Agent 与客户端使用的异步通信和记忆交换服务。参与方通过 MCP 投递和读取信息，服务保存可恢复的信箱、独立收取记录与版本化记忆资料。

实现进行中：Rust 的持久投递、独立回执、Blob 封存、snapshot/delta 记忆历史与固定版本分页已通过真实 SQLite 行为测试。MCP、隐私策略执行、Dashboard 和 demo 将按切片继续补齐；目前尚无在线服务。

- [RFC-0001：异步信箱与记忆交换协议](docs/rfcs/0001-async-mailbox-and-memory-sync.md)
- [RFC-0002：管理 Dashboard 与内容检查](docs/rfcs/0002-operator-dashboard.md)
- [RFC-0003：Agent 消息隐私策略](docs/rfcs/0003-agent-privacy-policies.md)
- [RFC-0001 性质与验证计划](docs/rfcs/0001-properties.md)
- [Rust 领域模型与 SQLite/D1 存储契约（已确认）](docs/design/0001-rust-data-model.md)
- [Rust 性质测试与 TDD 验证契约](docs/design/0002-property-test-contracts.md)
- [领域术语](CONTEXT.md)
- [一手协议参考](docs/research/0001-protocol-precedents.md)
- [Nowledge Mem 借鉴研究](docs/research/0002-nowledge-mem.md)
- [RFC 索引与变更约定](docs/rfcs/README.md)

后续实现以 RFC 的规范性要求、操作契约和性质为准。公开协议与通用 Skill 属于本项目；任何兼容客户端都可以接入。应用侧的模型、任务执行、内容理解和记忆整理由接入方负责。

仓库：[mainline-org/waypost](https://github.com/mainline-org/waypost)。许可证：[Apache-2.0](LICENSE)。
