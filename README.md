# Waypost

Waypost 是供个人 Agent 与客户端使用的异步通信和记忆交换服务。参与方通过 MCP 投递和读取信息，服务保存可恢复的信箱、独立收取记录与版本化记忆资料。

Rust v0.1 试用实现：原生 SQLite 服务与 Cloudflare D1/R2 适配共用消息、Blob、版本化记忆和独立回执；提供 MCP、Agent Dashboard、隐私规则和完整 demo。当前可本机试用，尚未部署在线托管服务。

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

## 本机试用

需要 Rust 1.91 或更新版本。

```bash
cargo run -- init --data ./var
cargo run -- serve --data ./var
```

打开 `http://127.0.0.1:8787/dashboard`，用 `var/admin-secret` 中的管理凭据登录。新建空间并接入 Agent，保存界面只展示一次的连接凭据；MCP 地址为 `http://127.0.0.1:8787/mcp`。默认只监听本机。自定义外部入口时设置 `WAYPOST_PUBLIC_ORIGIN`；`WAYPOST_ADMIN_SECRET` 可覆盖本地管理凭据。

真实 MCP SDK 试用：

```bash
npm ci --prefix examples/mcp-client
WAYPOST_ADMIN_SECRET="$(cat var/admin-secret)" node examples/mcp-client/demo.mjs
```

这个例子创建独立试用空间，以官方 MCP SDK 完成初始化、工具发现、幂等投递和内容读取。不会打印 Agent 的原始凭据。

Dashboard 检查不会替任何 Consumer 确认消息。隐私规则检查 UTF-8 文本并可派生脱敏附件；图片、PDF 等内容无法检查，默认阻止分享。来源记忆按原授权保存，历史消息不会被策略更新改写。规则覆盖范围可查看 `docs/rfcs/0003-agent-privacy-policies.md`。

- [完整双向通信、记忆导出与消费 demo](docs/demo.md)
- [原生、Docker 与 Cloudflare 运行/部署](docs/deployment.md)
- [通用 Waypost Skill](skills/waypost/SKILL.md)
- [内部存储边界 ADR](docs/adr/0001-internal-sql-driver.md)
