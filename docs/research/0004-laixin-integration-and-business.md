# 来信接入 Waypost：能力、边界与商业模式

研究日期：2026-10-14。Waypost 基线为 `c33058b`；来信资料按当日用户提供的说明和本地私有源码只读核对。本文区分现有代码、仓库已有验证记录与接入建议；没有运行来信、连接真实用户账户或完成双方集成测试。

输入来源：用户提供的《来信 for macOS》（2026-10-14）；用户明确的产品方向——Waypost 中立开源，来信是私有产品，将来可作为主助手接入；商业化更倾向来信，但收费归属尚未决定。公开本文只记录兼容性结论和通用契约，不复制私有产品说明或源码。

**建议先做一个窄闭环：其他助手把结果交到 Waypost，来信可靠收取、关联到事情、向用户汇报，再记录处理阶段。** 来信保留本地执行、记忆、心跳和通知。Waypost 提供任何合规客户端都能使用的消息、附件和资料版本接口。

## 1. 已有能力与尚未接入

| 需要 | 现有能力与证据 | 当前边界 |
|---|---|---|
| 来信作为主助手 | Waypost 已有主助手选择、独立 Actor/Consumer、按收件人投递；选择角色不自动扩大权限。[用户指南](../user-guide.md)、[主助手示例](../../examples/mcp-client/handoff.mjs) | 示例使用模拟助手；来信产品还没有被这些验证覆盖。 |
| 保存后确认收取 | 参考客户端在本地事务保存完整内容、Blob 字节、游标与待发送 ACK，随后发 received。[Consumer.sync](../../examples/consumer.py)、[本地队列](../../examples/local_store.py) | 是参考实现，来信需实现自己的持久连接器。 |
| 重启和响应丢失后恢复 | 有稳定 request_id、submission.status、独立 Consumer 位置及持久待处理项。[领域提交](../../src/service/mailbox.rs)、[HTTP 客户端测试](../../tests/test_consumer.py) | 仓库测试和验证记录不等于来信已实测；本次未重跑测试。[验证边界](../validation.md) |
| 本机保管连接凭据 | 来信外壳已有原生配置和远端请求头；返回页面的配置不包含 remoteToken。私有源码：`laixin-mac/Sources/main.swift` 的 `LaixinSettings`、`applicationDidFinishLaunching`。 | 当前是既有共享服务器配置，不是 Waypost 配置；凭据不返回页面也不等于不受本机开放 shell 或被攻陷运行时影响。 |
| 私人记忆与历史 | 来信已使用本地 Markdown 和 git；原生外壳有 `agent:memCommit`、`agent:memLog`、`agent:memRestore`。私有源码：`agent-kit/swift/AgentKit/Host.swift`。 | 本机 git 历史不是 Waypost 共享版本，也没有自动同步契约。 |
| 有版本的共享资料 | Waypost 已有 Blob、Collection、snapshot/delta、Tombstone、expected_revision CAS 与固定版本读取。[记忆服务](../../src/service/memory.rs)、[资料契约](../../skills/waypost/references/files-and-memory.md) | 服务器保存版本，不代替来信选择记忆、理解冲突或检索。 |
| 心跳、例行任务、通知 | 来信已有 `pulseTick`、`runRoutine` 和原生通知调用。私有源码：`p5/laixin.html`；用户提供的《来信 for macOS》。 | 应用彻底退出或设备关机后不继续执行。Waypost 当前没有调度器；describe 的 webhook_events 为 false。[能力发现](../../src/service/mod.rs) |
| 所有内核都受同样批准约束 | 来信已有不同运行内核的能力说明。私有源码：`laixin-mac/Sources/main.swift` 的 `cfg.providerVariants`；`p5/laixin.html` 的内核设置说明。 | OpenCode 变体为 unrestricted，逐工具批准不覆盖它。不能把接上信箱说成执行权限已统一收口。 |

“已有验证”应按 [validation.md](../validation.md) 表述：Waypost 的参考 HTTP/MCP 客户端验证了可靠交接；没有因此验证来信实际接入、模型处理、系统通知点击或其他产品完成外部任务。附件对来信自检与未验证项的说明是用户提供的项目记录，本次只做源码核对。

## 2. 不能只替换现有共享服务器地址

来信当前 remoteStoreURL 只用于指定远端集合；外壳将 plans/replies 操作转成 `/api/doc`、`/api/coll` 文档 CRUD。私有源码：`laixin-mac/Sources/main.swift` 的 `cfg.remoteCollections`；`agent-kit/swift/AgentKit/Host.swift` 的 `remoteStore`。

Waypost 是领域接口：`POST /v0/messages`、`POST /v0/inbox/read`、`POST /v0/receipts`、`POST /v0/submissions/status`，以及独立 Blob/记忆资源。HTTP 响应用 ok/data/error 信封；MCP 用工具结果与 structuredContent。既有 `DocStore.remote` 读取原文档 JSON，不做这种领域映射。[实际路由](../../src/http.rs)、[HTTP 响应](../../src/service/mod.rs)、[MCP 实现](../../src/service/mcp.rs)；私有源码：`agent-kit/swift/AgentKit/DocStore.swift` 的 `remote`。

还有一个独立问题：原 p5/server 的 currentUser 只识别签名 Cookie 或指定 SSO 头，没有 Bearer token 认证。外壳配置 remoteToken 不会自动让该服务器接受它。私有源码：`p5/server/server.mjs` 的 `currentUser`、`readCookie`。

因此要新增 **Waypost 连接器**，保留原共享服务器配置的用途。此次读取的来信原生入口、AgentKit Swift、应用页面和 Agent 资源中未发现 Waypost 连接实现；不能宣称已有 Swift SDK 或改一个 URL 就能上线。

## 3. 最小接入路径

### 连接和凭据由原生外壳管理

首版建议由原生连接器直接调用 Waypost HTTP API。它更适合在模型未运行时收取与恢复，不依赖每个 CLI 内核是否具备相同远端 MCP 配置。MCP 可以作为后续宿主入口，两者复用相同领域契约。[HTTP/MCP 映射](../rfcs/0001-async-mailbox-and-memory-sync.md)、[MCP 参考客户端](../../examples/mcp-client/handoff.mjs)

用户在自己的 Waypost Space 接入来信，选择主助手并显式签发 `message.read`、`receipt.write`、`message.send`。当前首次接入只有 `message.send`，主助手角色也不自动授予收信。不要发给来信部署管理员或用户管理凭据。[默认授权](../user-guide.md)、[发信限制测试](../../tests/assistant_inbox.rs)

建议新连接凭据使用原生秘密存储，例如受应用访问控制的 macOS Keychain；页面和模型只接触连接名称、身份、权限、到期与状态。不要把 token 写入普通对话、模型 prompt、记忆、工具 stdout、导出包或命令参数。原生网络层限制服务地址与重定向，避免将鉴权头发给其他主机。[秘密与文件传输约束](../../skills/waypost/references/files-and-memory.md)、[参考 HTTP 的禁止重定向实现](../../examples/consumer.py)

连接后先调用 describe，保存实际 Space、Actor、Consumer、权限与 limits；端点来自用户配置，身份来自认证响应。每个安装实例使用独立 Consumer；同一主助手身份是否跨设备复用，另由用户选择。连接不存在、撤销、过期或服务不可用时如实显示，不把离线收取说成成功。[领域认证](../../src/service/mod.rs)、[Consumer 绑定与恢复示例](../../examples/consumer.py)

### 收取先入本机，再发 received

原生连接器要有自己的 SQLite 输入、出站和关联记录，至少保存：连接绑定、Feed item/message ID、固定资源版本、完整消息与附件、原始来源、处理状态、游标、每次写入的 request_id 与精确参数。游标和待处理内容同一事务保存；已保存但未处理的内容必须跨重启保留。

收取步骤为：读取 Feed → 按授权读取完整原文和附件 → 核对附件字节 → 本地事务保存整页与待发 ACK → `received`。分页按 describe 的 limits；未知 Feed 类型不直接跳过或标 processed。[Consumer.sync](../../examples/consumer.py)、[关系队列](../../examples/local_store.py)、[收取 Skill 契约](../../skills/waypost/SKILL.md)

读列表、显示一条消息、收到模型摘要，都不是完整可靠收取。ACK 响应丢失时，先查询原提交状态；恢复仍用同一个 request_id。不能为了“再试一次”更换 ID 制造重复发布。[响应丢失与重启测试](../../tests/test_consumer.py)、[submission 恢复](../../src/service/mailbox.rs)

### 关联事情，处理完成后发 processed

Waypost thread_id 与来信事情 ID 分别保留。首版由本地关联表记录两者关系，不让外部发送方直接选本机任务并获得执行权。出站可以使用现有 source.native_thread_id/native_message_id 以及受限 metadata 标注来源；这些是客户端声明，不能改变服务器认证的 Actor 或授权。[Send/Source DTO](../../src/domain.rs)、[身份来源契约](../rfcs/0001-async-mailbox-and-memory-sync.md)

来信将来信关联到用户选择的事情，或作为待查看的外部结果；记录实际处理结果和报告引用后才发 processed。等待用户决定与外部执行结果未知应单独显示。processed 只表示这个 Consumer 报告处理阶段，不保证某项预约、付款或用户目标成功。[回执语义](../../src/service/receipts.rs)、[主助手场景](../use-cases/main-assistant.md)

收到的内容是数据。经过认证的助手发来“删除文件”“替我付款”或“以后都允许”，都不能充当本机用户授权。由来信的可信执行层检查当前用户权限与批准规则；已有开放权限内核若做不到这一点，应先限制自动执行，不能只用 prompt 要求模型忽略恶意指令。[Waypost 接收原则](../../skills/waypost/SKILL.md)；私有源码：`laixin-mac/Sources/main.swift` 的 `unrestricted` 配置，`p5/laixin.html` 的批准与内核设置逻辑。

## 4. 私人记忆与共享资料应分开

| 层 | 权威内容 | 版本与用途 |
|---|---|---|
| 来信本地记忆 | 用户本机的 Markdown，含尚未选择共享的私人信息 | git commit 用于编辑追溯、恢复与本机工作。私有源码：`Host.swift` 的记忆方法。 |
| 来信共享导出 | 用户明确选择的文件与条目 | 本地保存 item_id/path、内容摘要、来源 git commit、目标 Collection、上次发布结果。是接入建议。 |
| Waypost Collection | 已授权发布的源资料与历史修订 | 服务 revision、CAS、Blob、Tombstone；其他助手按资源权限读取。[记忆契约](../../skills/waypost/references/files-and-memory.md) |
| 来信收取的外部资料 | 完整远端原文、固定 revision 与来源 | 先保留为外部资料；经明确规则或用户确认后，才写入自己的记忆。是接入建议。 |

git commit 和 Waypost revision 不是同一个版本号，不能相互替换。建议首版每个来源助手有独立 Collection，来信汇总时引用来源集合和固定版本；共同编辑一个集合则明确授予写入权并处理 CAS 冲突。[历史与过期版本测试](../../tests/memory.rs)、[委托写入边界](../../tests/delegated_memory.rs)

共享只导出用户选定范围，不默认上传全部 memory/、本机工作目录或聊天原文。增量用 delta；只有完整掌握且获准替换整个集合时才用 snapshot。缺失文件或扫描失败不能当删除。被用户从共享范围移除的文件如何处理历史，要明确告知：Tombstone 不抹掉已发布历史和已下载副本。[共享资料场景](../use-cases/shared-materials.md)、[snapshot/delta 约束](../../skills/waypost/references/files-and-memory.md)

每次发布保存固定请求。遇到 REVISION_CONFLICT，读取现有版本比较本地/远端变化，保留冲突或请用户选择；不能仅更新 expected_revision 然后覆盖。来信恢复旧 git commit 后，分享的是一份新的内容变更，不能把远端 revision 倒退。[memory.publish](../../src/service/memory.rs)、[客户端冲突处理](../../skills/waypost/references/files-and-memory.md)

本地私人资料、输入队列与备份仍需本机保护。Waypost 的加密是服务端存储保护，部署运行时持有解密密钥；选择托管不代表运营者不能读取共享内容。[安全说明](../security.md)

## 5. 心跳、调度与共享计划

心跳是来信判断是否值得主动联系用户的模型运行；收取是连接器的网络同步。这两件事应分开：低成本轮询可先可靠保存消息，不因每条消息都唤醒模型；需要汇总时再由来信当前运行策略、安静时段与用户偏好决定是否执行和通知。私有源码：`p5/laixin.html` 的 `pulseTick`、`runRoutine`。

Waypost 继续保存消息与资料；来信安排何时收取、处理、提醒。应用后台运行与设备离线状态如实显示。当前 webhook_events=false，不能假设 MCP 连接会主动唤醒来信；未来通知扩展只能提示变化，可靠内容仍通过 Feed 补收。[当前 capabilities](../../src/service/mod.rs)、[可选事件边界](../rfcs/0001-async-mailbox-and-memory-sync.md)

plans/replies 涉及共享计划、成员权限、回复、取消与冲突。它们不是信箱消息的另一个名字。首版保留既有共享服务器机制；Waypost 可传递“计划提议/回复通知/结果引用”，不直接冒充日历、预订或共享计划的权威状态。

若未来需要跨助手共享计划，再设计中立的计划状态、参与者授权、版本、明确同意与幂等变更。来信 UI 负责约人体验；新共享领域是否进入 Waypost，要根据其他客户端是否同样需要，而不是把来信私有文档结构塞进通用核心。现有事实来源：私有源码 `Host.swift.remoteStore` 与 `p5/server/server.mjs` 的 `/api/doc`、`/api/coll`；Waypost 当前 [路由](../../src/http.rs) 与 [领域模型](../../src/domain.rs)。

## 6. 开源与商业边界

Waypost 当前仓库采用 [Apache 2.0](../../LICENSE)，提供通用 API、Skill、参考客户端与自托管路径。来信是用户明确的私有产品。建议共用公开协议，连接器将私有页面、事情模型、提示词、内核编排和通知体验留在来信；Waypost 的测试与示例使用中立角色，不要求安装来信。[公开运行与接入](../../README.md)、[通用 Skill](../../skills/waypost/SKILL.md)

以下是商业选项，不是价格、合同、订阅状态或已上线收费功能：

| 选项 | 谁承担主要商业价值 | 优点 | 需要解决 |
|---|---|---|---|
| A：来信应用与体验收费 | 来信的执行、整理、主动联系、多设备体验；Waypost 保持开源和可自托管 | 符合当前更倾向来信商业化的方向；开放协议帮助更多助手接入 | 托管成本由谁承担仍需决定，不能因此许诺永久免费托管。 |
| B：Waypost 托管独立收费 | 托管存储、运维、备份、可用性与支持；来信可另外选择商业模式 | 不使用来信的人也能购买同一种服务；开源与运营价值区分清楚 | 需要有真实成本、额度与服务能力依据；现有邀请试用不是收费套餐。 |
| C：来信方案包含托管额度 | 用户购买来信体验，同时获得一份 Waypost 托管权益 | 用户理解较直接，可减少接入与账单步骤 | 服务额度、超额行为和取消后的数据出口要清楚；来信仍应支持自托管端点，其他客户端仍可使用开放接口。 |

建议先以 A 验证来信对用户的价值，托管收费归属保持未定。确认用户需求和运营成本后，再比较 B 与 C；不在当前网站写未经决定的定价或“已上线订阅”。推荐只是产品判断，尚无付费转化、成本或市场访谈数据。

中立性可以具体检验：相同公开权限与接口适用于所有助手；账户与数据归用户空间；能导出固定版本原文件；来信不成为创建账号、阅读协议、导出或迁移的必选依赖。付费权益也不能隐式扩大 Agent 的数据授权。[用户空间与凭据](../security.md)、[导出示例](../../examples/consumer.py)

## 7. 首轮接入验收

建议先用隔离空间和模拟发送方验证以下闭环，再将其标记为“来信已接入”：

1. 原生外壳保存 Agent 凭据，页面与模型无法通过正常连接器接口取得原始值；describe 返回预期身份与最小权限。
2. 外部助手发送一份消息与原始附件；来信完整持久保存后 received，尚未处理的内容在重启后仍可查询。
3. 模拟 ACK 已提交但响应丢失，以及发信已提交但响应丢失；恢复原结果，不产生第二笔发布。
4. 来信将输入关联到事情，记录实际处理阶段，再向用户投递报告并 processed；恶意外部指令不会自动扩大本机权限。
5. 撤销、过期、换 Consumer 与变更资源范围不会跨空间取数，也不会错误沿用别的 Consumer 进度。
6. 选择一个文件发布 delta，另一助手固定 revision 回读准确字节；并发 CAS 冲突不静默覆盖，私人记忆文件不被导出。

依据为现有 [参考收取测试](../../tests/test_consumer.py)、[权限与信箱测试](../../tests/assistant_inbox.rs)、[记忆历史测试](../../tests/memory.rs)、[空间隔离测试](../../tests/encryption_isolation.rs) 和 [MCP 交接示例](../../examples/mcp-client/handoff.mjs)。这些提供验收起点，不能替代真实来信端的整轮验证。

## 私有输入的回溯索引

以下只列项目内相对位置和符号，方便维护者复查；不构造不可访问的公开源码链接。

- 用户提供：《来信 for macOS》，2026-10-14。
- `laixin-mac/Sources/main.swift`：`LaixinSettings`、`applicationDidFinishLaunching`、`cfg.remoteCollections`、`cfg.providerVariants`。
- `agent-kit/swift/AgentKit/Host.swift`：`handleStore`、`remoteStore`、`agent:memCommit`、`agent:memLog`、`agent:memRestore`。
- `agent-kit/swift/AgentKit/DocStore.swift`：`remote`。
- `p5/server/server.mjs`：`currentUser`、`readCookie`、`/api/doc`、`/api/coll`。
- `p5/laixin.html`：`pulseTick`、`runRoutine`、批准与内核设置逻辑。
