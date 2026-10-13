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

原有 19 个 Rust 测试覆盖以下公开行为。其中 Blob 字节、随机记忆操作和独立回执三类 proptest 生成器，每类运行 12 组；这不是穷举或正确性证明。

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

[GitHub Actions](https://github.com/mainline-org/waypost/actions/workflows/ci.yml) 运行原生检查、Wasm 检查、真实 Docker 构建/运行，以及独立 Workerd + D1/R2 runtime 检查。原生与 Worker CI 均运行官方 MCP SDK 和完整 demo；Worker 另检查就绪、会话、Origin 和实际限速。具体交付提交的结果以 PR 对应检查为准。

## 实际试用

- 官方 MCP SDK 在原生服务和本机 Cloudflare Worker 上完成初始化、发现 11 个工具、投递、同键重试与准确正文读取。
- 完整 demo 在本机 Worker + D1 + R2 上执行成功，包含双向通信、隐私过滤、记忆修订与删除、先存 SQLite 后 ACK、processed、重启保留未处理项和完整资料包。
- 隔离 Chrome 验证登录、新建空间、接入 Agent、保存隐私策略、预览和退出；未出现页面异常。桌面与 390px 页面已检查。
- 另模拟服务已经提交、浏览器丢失空间/Actor/凭据创建响应。重试没有重复创建身份；无法重取原始凭据时撤销旧凭据并只保留一枚有效替代凭据。
- Standards 与 Spec 两路只读复核发现的关系化队列和旧策略拒绝竞态已修复并复核关闭。通用 Skill 的官方结构校验和四个前向使用场景检查通过。

## 验证边界

首轮验证使用真实本机 Workerd、D1 和 R2 绑定；后续已经发布独立 Cloudflare 生产实例，运行步骤与版本追溯见 [生产运行](production.md)。本机与线上示例验证的是明确的功能场景，尚未做容量或生产负载基准。原生/Worker 上限见 [部署说明](deployment.md)。

外部助手仍需要用户配置 MCP 和私密凭据；参考客户端按手动调用运行，任务调度与模型执行由宿主提供。服务会检查已配置的本地隐私规则，规则覆盖范围见 [RFC-0003](rfcs/0003-agent-privacy-policies.md)。

## Cloudflare 生产验收（2026-10-12）

生产入口：<https://waypost-production.christopherwuy.workers.dev>。Worker Version 为 `79117c74-ec90-4b41-8864-3b397e1a3d28`；独立 APAC D1/R2 已创建，六个迁移成功，R2 公开 URL 关闭。

已在真实 HTTPS 入口通过以下检查：

- 健康、禁止缓存/CSP/HSTS、production 标记、管理员鉴权的 D1/R2 就绪。
- Origin 拒绝、Secure/HttpOnly/SameSite 管理会话、CSRF 与退出后失效。
- 官方 MCP SDK 的初始化、11 个工具、实际投递、同键稳定结果和完整正文读取。
- 完整双向通信与记忆示例：snapshot/delta/Tombstone、隐私报告、资料 SHA-256、可靠保存后 ACK、显式 processed、重启恢复和完整 SQLite/原文件导出。

参考客户端曾因默认 Python User-Agent 收到 Cloudflare 1010；改为明确的 Waypost 产品标识后，上述真实流程通过。验收脚本见 `scripts/verify_deployment.py`，追加限速检查会消耗检查者的短期 IP 预算。

## 加密、用户隔离与评审改进（2026-10-13）

本次新增 15 个安全回归：`tests/security_hardening.rs` 的八项检查覆盖必需行冲突整体回滚、清理预览/权限/租约/迟到写入、独立游标密钥、空间内序号和路由边界；`tests/encryption_isolation.rs` 的七项检查覆盖消息/附件/记忆字段的实际密文、正常读取、密钥轮换、错钥/密文替换、跨用户访问、用户会话轮换、明文严格模式、可重复迁移和 CLI 运行锁。

已通过原生 fmt/clippy、34 个 Rust 测试、六个真实 HTTP Python 测试，以及 Wasm clippy。独立本机 Workerd/D1/R2 已应用八个迁移，官方 MCP SDK、完整加密示例和七项就绪/会话/Origin/限速验收通过。另直接检查本机存储：四条消息的 D1 主题均为认证密文，九个 R2 对象以加密格式保存；导出仍返回准确原文件。

这批安全改进没有部署到上述旧生产版本。升级前需要配置独立加密 keyring、应用迁移并迁移旧内容；步骤见 [安全与数据管理](security.md)。尚未做新加密版本的生产负载、CPU/容量基准或外部密码学审计。SQL 已对关键行增加类型解码、收拢认证/回执与插入语句，其余领域语句仍使用绑定参数的 SQL。

## 助手信箱定位与按需接入（2026-10-13）

新增 `tests/assistant_inbox.rs` 两项回归：信箱按当前空间的用户入口筛选，保留全空间消息检查并拒绝游标跨查询复用；仅发信连接能投递和恢复自己的结果，不能收信、上传、写回执或向未授权收件人发送，撤销后失效。管理 token 检查返回真实 scope，排除原始凭据和摘要。

36 项 Rust 测试、原生和 Wasm clippy 已通过。另使用 JSDOM 与隔离的真实本机 HTTP 服务执行登录、默认接入、首次报告、恶意正文纯文本显示、显式六项能力签发、资料集合创建、到期选择、关闭凭据详情和退出清理。默认接入的实际请求只有 `message.send`，没有创建资料集合。官网三个示例的选择、ARIA 与键盘切换检查通过；17 页静态链接、锚点和公开文档快照检查通过。

本轮 Browser 未连接，DOM 检查不等于浏览器视觉验收；新版响应式布局尚未做实际浏览器视口检查。本次只更新 Vercel 介绍网站，新 Dashboard 与管理检查接口尚未升级到旧 Cloudflare 生产实例。任务授权与人工批准仍未实现。
