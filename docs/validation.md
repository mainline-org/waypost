# v0.1 试用验证记录

验证对象是本次 Rust 实现与 [PR #3](https://github.com/mainline-org/waypost/pull/3)。它提供可以运行的中心层、参考客户端和 demo；验证没有将参考助手身份当作外部产品已安装的连接。

## 自动化检查

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --locked
python3 -m unittest discover -s tests -p 'test_*.py' -v
cargo clippy --no-default-features --features cloudflare --target wasm32-unknown-unknown --locked -- -D warnings
```

19 个 Rust 测试覆盖以下公开行为。其中 Blob 字节、随机记忆操作和独立回执三类 proptest 生成器，每类运行 12 组；这不是穷举或正确性证明。

| 范围 | 验证内容 | 测试 |
|---|---|---|
| 消息 | 同键稳定结果、重复内容冲突、线程授权 | `tests/mailbox.rs` |
| Consumer | 独立位置、ACK 幂等与单调推进 | `tests/receipts.rs`、`tests/properties.rs` |
| Blob | 封存、实际长度/摘要、不可变字节 | `tests/blobs.rs` |
| 记忆 | snapshot/delta、完整历史与 Tombstone、CAS、无变化、不混合分页版本 | `tests/memory.rs`、`tests/memory_feed.rs`、`tests/properties.rs` |
| MCP | 11 个工具、初始化协商、通知、公开调用与服务规则一致 | `tests/mcp.rs` |
| Dashboard | 管理会话、退出、CSRF/CSP、检查不推进 ACK | `tests/dashboard.rs` |
| 授权 | 观察权限不读取私有记忆/未分享 Blob，委托写不能读取完整私有 manifest，凭据轮换恢复原回复 | `tests/observer_privacy.rs`、`tests/delegated_memory.rs`、`tests/token_rotation.rs` |
| 隐私 | 选定发信/收信 Agent、正文与资料脱敏、二进制阻止、输出预算、稳定拒绝、策略变化后的成功/拒绝竞态 | `tests/privacy.rs`、`tests/privacy_race.rs`、`tests/submission_failures.rs` |

6 个 Python 测试启动实际原生 HTTP 服务，覆盖：已提交 ACK 的响应丢失与重启；Actor 出站队列在权限丢失和 Consumer 轮换后恢复；固定版本原文件导出；关系参数重放及请求冲突；记忆、ACK 和回执关系查询；旧试用版 JSON 队列的事务升级。

[GitHub Actions](https://github.com/mainline-org/waypost/actions/workflows/ci.yml) 运行原生检查、Wasm 检查，以及真实 Docker 构建、启动、健康读取和非 root UID 检查。原生 CI 同时运行官方 MCP SDK 和完整 demo；具体交付提交的结果以 PR 对应检查为准。

## 实际试用

- 官方 MCP SDK 在原生服务和本机 Cloudflare Worker 上完成初始化、发现 11 个工具、投递、同键重试与准确正文读取。
- 完整 demo 在本机 Worker + D1 + R2 上执行成功，包含双向通信、隐私过滤、记忆修订与删除、先存 SQLite 后 ACK、processed、重启保留未处理项和完整资料包。
- 隔离 Chrome 验证登录、新建空间、接入 Agent、保存隐私策略、预览和退出；未出现页面异常。桌面与 390px 页面已检查。
- 另模拟服务已经提交、浏览器丢失空间/Actor/凭据创建响应。重试没有重复创建身份；无法重取原始凭据时撤销旧凭据并只保留一枚有效替代凭据。
- Standards 与 Spec 两路只读复核发现的关系化队列和旧策略拒绝竞态已修复并复核关闭。通用 Skill 的官方结构校验和四个前向使用场景检查通过。

## 验证边界

Cloudflare 的验证使用真实本机 Workerd、D1 和 R2 绑定；当前账户登录已过期，未部署公开在线 demo，也未测量远程免费计划的生产 CPU。部署步骤与原生/Worker 各自上限见 [部署说明](deployment.md)。

外部助手仍需要用户配置 MCP 和私密凭据；参考客户端按手动调用运行，任务调度与模型执行由宿主提供。服务会检查已配置的本地隐私规则，规则覆盖范围见 [RFC-0003](rfcs/0003-agent-privacy-policies.md)。
