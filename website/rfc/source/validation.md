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

本段记录最初的本机验证。接入工作开始时，生产已经运行八迁移加密版本；当前九迁移发布与真实验收见本文最后一节和 [生产运行](production.md)。升级旧实例仍需配置加密 keyring、应用迁移并迁移旧内容。尚未做生产负载、CPU/容量基准或外部密码学审计。SQL 已对关键行增加类型解码、收拢认证/回执与插入语句，其余领域语句仍使用绑定参数的 SQL。

## 助手信箱定位与按需接入（2026-10-13）

新增 `tests/assistant_inbox.rs` 两项回归：信箱按当前空间的用户入口筛选，保留全空间消息检查并拒绝游标跨查询复用；仅发信连接能投递和恢复自己的结果，不能收信、上传、写回执或向未授权收件人发送，撤销后失效。管理 token 检查返回真实 scope，排除原始凭据和摘要。

36 项 Rust 测试、原生和 Wasm clippy 已通过。另使用 JSDOM 与隔离的真实本机 HTTP 服务执行登录、默认接入、首次报告、恶意正文纯文本显示、显式六项能力签发、资料集合创建、到期选择、关闭凭据详情和退出清理。默认接入的实际请求只有 `message.send`，没有创建资料集合。官网三个示例的选择、ARIA 与键盘切换检查通过；17 页静态链接、锚点和公开文档快照检查通过。

本轮 Browser 未连接，DOM 检查不等于浏览器视觉验收；新版响应式布局尚未做实际浏览器视口检查。此段是当时网站更新的记录，当前 Dashboard 与管理检查接口已随后续生产发布升级。任务授权与人工批准仍未实现。

## 邀请与主助手接入验证

在独立 worktree 实现，避免覆盖并行 i18n。新增五项 HTTP 回归覆盖默认投递目标与发信范围、主助手选择的版本检查与空间限制、一次性领取与个人会话、邀请撤销/替换/到期/所有者变化、使用申请的隐私与幂等。

44 项 Rust 测试与原生、Wasm clippy 通过。Dashboard DOM 回归在每页 20 条的公开 HTTP 契约下复现并修复了权限面板故障，也验证了后续资料页被保留。真实本机原生服务与 Workerd/D1/R2 的官方 MCP 客户端完成接入助手投递、同键恢复、主助手可靠保存后 received、汇报用户与 processed，发信连接无法读信箱。

两个运行时都通过真实 HTTP + DOM 的申请、审批、邀请领取、清除 URL 片段、个人空间登录、主助手连接预设和接入助手默认投递测试。直接读取隔离 D1 的匿名统计确认申请联系方式为密文。验收模拟协作角色，没有执行实际餐厅预订，也不宣称已验证 Muse 等第三方产品。

双轴评审基线为 8840a0a。评审后的失败测试覆盖两空间同时审批同一申请、同权限换连接后默认收件人重试、联系方式轮换后移除旧密钥、领取后空间加载失败，以及退出与邀请响应在同一回合完成。上述问题均先复现，再修复；前三项增加 Rust 回归，后两项增加 DOM 回归。私密邀请与连接凭据只在原会话展示，领取后的个人凭据先显示，再加载空间。

脚本为 `examples/mcp-client/handoff.mjs`、`examples/mcp-client/onboarding-ui.mjs`，已加入原生与 Worker CI；Worker 启动等待涵盖首次 Wasm 构建，超时明确报告失败。实际 CI 发现领取后空间尚未加载时能够点击接入，已补齐入口等待与空间检查；修复后的 native、Wasm、container 和真实 Worker 四项 CI 全部通过，并在 PR #6 合并 main。

## 九迁移生产验收（2026-10-13）

生产 Worker Version：`35877fca-1f73-4eb8-8cfa-7085c70728af`，运行时代码 `9d989ff`。`verify_deployment.py` 验证真实 HTTPS 安全头、九迁移 D1/R2、Origin、CSRF 与退出失效。

官方 MCP 客户端在生产隔离空间验证同请求稳定结果、接入助手只发信、主助手先保存后 received、向用户汇报与 processed。真实 HTTP + DOM 验证从匿名申请、运行者审批到个人空间领取，以及主助手与接入助手的权限配置。生产网络下的模拟页面关闭等待尚未完成的请求，完整验收进程正常退出。

本轮没有执行真实 Muse 产品集成或实际餐厅订单，没有进行浏览器视觉与负载测试。并行 i18n 尚未提交，本次没有覆盖或发布其未提交文件。

## Dashboard 与主页交互验收（2026-10-14）

PR #8 已合并主线，原生、Wasm、container 和真实 Worker 四项 CI 全部通过。45 项 Rust 测试、原生与 Wasm clippy 通过；本轮另通过五项 Dashboard DOM 回归，覆盖登录重置的明确确认、主助手停用提示和敏感详情清理。

隔离 Chrome 验证了真实 CSS 布局：1280×640 和 1024×480 的侧栏与视口等高，导航独立滚动，退出按钮始终可见；390px 和 320px 页面按正常页面滚动，没有横向溢出。管理入口仅服务运行者可见，邀请说明对方获得独立信箱，登录管理先说明目标空间与重置后果。

主页在 1280px、390px、320px 下验证了信箱、助手、资料三个入口，助手角色选择、附件跳转、资料历史版本和键盘切换。示例只在浏览器内运行，不连接真实账户。布局验收使用隔离浏览器和模拟 API 响应；另用真实本机原生 HTTP 服务完成申请、审批、邀请领取、个人登录、主助手配置，以及仅发信助手投递、主助手保存后 received、向用户汇报与 processed。

Standards 复核补齐安全文档的登录状态字段说明；Spec 复核修正停用主助手后仍要求等待的提示。说明明确保留已有连接的收件人，选定主助手不自动授予权限。23 页静态链接与 13 份公开原文快照检查通过。未执行实际餐厅订单、第三方助手安装或负载测试；并行 i18n 文件保持原样。
