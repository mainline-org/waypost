# 0001：Waypost 0.1 的协议先例与实现边界

- 检索日期：2026-10-11。
- 范围：第一方协议、规范仓库与平台文档；不实施服务、不部署、不执行 Git 操作。
- 标记：**事实**是来源直接规定的行为；**Waypost 建议**是本项目的产品或架构推论，不代表原规范已有该设计。
- 版本边界：Waypost 应用 API 版本 `0.1`、CloudEvents 格式版本、MCP 协商版本分别管理。

## 1. Matrix MSC：RFC 是可以据此实施的契约

**事实。** MSC 要同时解释问题与动机，并无歧义地描述技术变化及边界情况，供评审和实施使用；模板不是强制格式。Checklist 要求写明各端点的认证、限流、错误码/HTTP 状态、分页、兼容性和安全考量。

README 当前真正链接的模板路径是 `proposals/0000-proposal-template.md`；正文仍提到旧路径 `docs/0000-proposal.md`。本次成功读取 README 和 Checklist，模板正文的直接抓取未成功，不能声称已逐项核对完整模板。Checklist 直接列明 Introduction、Proposal text、Potential issues、Alternatives、Unstable prefix、Dependencies 等可选标题。

来源：[MSC README](https://github.com/matrix-org/matrix-spec-proposals/blob/main/README.md)、[MSC Checklist](https://github.com/matrix-org/matrix-spec-proposals/blob/main/MSC_CHECKLIST.md)、[README 链接的模板](https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/0000-proposal-template.md)。检索：2026-10-11。

**Waypost 建议。** 借鉴“动机 + 精确契约 + 故障语义 + 替代方案 + 兼容/安全”的写法；首版 RFC 至少覆盖身份、信封、收发、同步、来源记忆、存储提交与可选适配。把成功和失败时的外部可见结果写成可以验证的约束。

**不照搬。** 不采用 Matrix 的联盟、房间状态、Room Version、社区投票或 MSC 编号制度。项目本身定义 RFC 状态；研究出处不是本项目的规范承诺。

## 2. Matrix Client-Server：同步位置与请求幂等分开

**事实。** `/sync` 首次不带 `since`，后续用响应的 `next_batch` 继续；支持有界等待。受限 timeline 可以产生缺口，需另行补历史，客户端按 event ID 去重。`txnId` 用于区分新提交与请求重传。

v1.19 的通用 Transaction identifiers 章节把 txn 范围定义为单个 device 与 HTTP endpoint；同一版本 send 端点参数表仍有“同 access token 唯一”的措辞。这里记录来源内部的差异，不把 token 生命周期引入 Waypost。

来源：[Matrix v1.19 同步](https://spec.matrix.org/v1.19/client-server-api/#syncing)、[Transaction identifiers](https://spec.matrix.org/v1.19/client-server-api/#transaction-identifiers)、[Send event](https://spec.matrix.org/v1.19/client-server-api/#put_matrixclientv3roomsroomidsendeventtypetxnid)。检索：2026-10-11。

**Waypost 建议。** 为每个消费者保留独立同步位置；身份、view、规范化 filter 和权限版本绑定游标。同一 view/filter 的分页不得静默跳过尚未返回的可见项。客户端先持久保存本批再提交接收游标；消息业务处理回执独立于读取位置。

幂等提交范围建议明确为 `space + authenticated producer + operation + idempotency key`；密钥轮换不改变 producer 身份。同 key 同内容返回同一结果，同 key 不同内容拒绝，不能悄悄把后一次写当新信封。

**不照搬。** Matrix 的 room timeline 受限截断、状态 delta 与 send-to-device 删除机制不直接用于普通持久 feed；消费者已同步不能意味着所有消费者处理完成，更不能触发全局删除。

## 3. CloudEvents 1.0.2：稳定身份与格式，不提供授权或业务一致性

**事实。** 必填上下文有 `id`、`source`、`specversion`、`type`。不同事件的 `source + id` 必须唯一；重复发送可保留同一 ID，消费者可据此识别重复。`source` 是非空 URI-reference，推荐绝对 URI，不必是可访问的 HTTPS 地址。1.0.2 文本的 `specversion` 值仍是 `1.0`。扩展属性名只能使用小写 ASCII 字母与数字，建议不超过 20 字符；不兼容 data schema 变更应改变 `dataschema` URI。

来源：[CloudEvents v1.0.2](https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md)。检索：2026-10-11。

**Waypost 建议。** 定义固定的 Waypost CloudEvents profile，而非只称“兼容”。来源可采用由服务绑定的 `urn:waypost:source:...`；`source + id` 与存储幂等范围一并说明。收件人、主题、关联、正文引用和 API 版本等应用字段放 `data`，或为确需顶层的字段定义合法扩展名。`reply_to`/`api_version` 不能直接冒充合法 CloudEvents 顶层扩展。

**不照搬。** CloudEvents `source` 不是已认证身份，`time` 不是 feed 排序依据，ID 不等于服务器提交序号。CloudEvents 不规定消费者游标、业务回执、访问控制、存储不可变性和共享记忆合并；这些必须由 Waypost 自己规定。事件发生时间、服务发布时间与 webhook 签名时间分开保存。

## 4. Standard Webhooks：逐次投递签名与稳定事件去重

**事实。** 使用 `webhook-id`、`webhook-timestamp`、`webhook-signature`；签名覆盖 `id.timestamp.payload` 的实际发送内容，不能解析后重新序列化再验签。对称签名为 HMAC-SHA256，密钥为 24–64 随机字节，展示格式为 `whsec_` 加 base64；也支持 Ed25519。重试保留 webhook ID，每次重试更新时间戳。验证要检查时间容忍度、使用恒时比较并去重；多签名支持轮换。建议指数退避加抖动，`2xx` 为收到，`410` 表示停止，其他失败/`Retry-After` 需按交付策略处理。

来源：[Standard Webhooks specification](https://github.com/standard-webhooks/standard-webhooks/blob/main/spec/standard-webhooks.md)。检索：2026-10-11。

**Waypost 建议。** 将 webhook ID、Waypost event ID 与一次投递 attempt ID 的映射写清；同一事件对不同订阅有独立投递进度。订阅持久化，发出通知后消费者仍按持久 feed 拉取缺失记录。签名成功证明来源与内容，HTTP 成功证明对方接收，两者均不能证明 Agent 的业务工作完成。

**不照搬。** 规范建议的重试日程和短期去重示例不是 Waypost 的固定 SLA。不要把一个五分钟去重缓存当成永久信封幂等库；不要把“至少一次通知”表述成“业务副作用恰好一次”。

## 5. Cloudflare：存储边界与当前关键约束

**事实：平台额度。** 截至检索日，Workers Free 为每天 100,000 请求、每次 10ms CPU，isolate 内存 128MB；网络/数据库等待不计 CPU。HTTP 请求结束或断开后工作可能取消，`waitUntil` 仅额外延续最多 30 秒。付费最低 $5/账号/月。

来源：[Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/)、[Workers limits](https://developers.cloudflare.com/workers/platform/limits/)。检索：2026-10-11。

**事实：D1。** 免费每天 500 万读行、10 万写行，账号总存储 5GB；免费单库上限 500MB，单行/string/BLOB 上限 2MB，每次 Worker 调用最多 50 查询。读行按实际扫描而非结果条数计算。batch 是 D1 SQL 事务，错误回滚整批；返回对象把 SQL success 与 `meta.changes` 分开。读复制通过 Sessions API 和 bookmark 提供顺序一致性。

来源：[D1 pricing](https://developers.cloudflare.com/d1/platform/pricing/)、[D1 limits](https://developers.cloudflare.com/d1/platform/limits/)、[D1 Database API](https://developers.cloudflare.com/d1/worker-api/d1-database/)、[D1 Return objects](https://developers.cloudflare.com/d1/worker-api/return-object/)。检索：2026-10-11。

**事实：R2。** Standard 免费每月 10GB-month、100 万 A 操作、1,000 万 B 操作；直接出网免费。对象读取/写入/删除/列举为强一致，同 key 并发写最后完成者生效，缓存域名可能仍返回旧副本。对象 metadata 最多 8KiB，同 key 写入上限约每秒一次；R2 最大对象或上传额度不能替代 Worker 入站及内存限制。

来源：[R2 pricing](https://developers.cloudflare.com/r2/pricing/)、[R2 consistency](https://developers.cloudflare.com/r2/reference/consistency/)、[R2 limits](https://developers.cloudflare.com/r2/platform/limits/)、[R2 Workers API](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/)。检索：2026-10-11。

**Waypost 建议：D1 是发布可见性的提交点。** 先使不可变 R2 blob 可读，再在同一 D1 发布事务内完成唯一幂等记录、信封索引、来源记忆 revision 变更与 feed 项，并分配 `visible_seq`。只有这次发布提交成功才对读者可见和返回已接受的稳定结果。不得先分配可被游标越过的序号，再等上传完成后补发布；否则较晚插入的低序号记录可能永久漏读。

此处是本项目推论：R2 的强一致与 D1 batch 的原子性各自成立，不形成一个跨 R2/D1 的事务。上传成功后 SQL 发布失败可留下不可见孤立 blob；发布成功后响应丢失应以同幂等 key 返回原结果。清理策略须避免删除仍在提交或已被索引引用的 blob。

CAS 实施边界也要写清：SQL success 不等于条件更新命中，`changes = 0` 不能被当成 CAS 成功；如果 CAS 没命中，不能在同一批中无条件追加 feed 或提高 revision。RFC 应要求事务内所有发布副作用受同一个成功条件约束，但不在研究笔记里指定未经验证的 SQL 写法。

**不照搬。** 不把全量正文/附件装入 D1 单行；不把全量信箱放一个可覆盖 R2 JSON；不把 Worker 内存或 `waitUntil` 当持久队列。R2 免费档只针对 Standard，平台免费额度不能作为协议消息大小或长期可用性承诺。

## 6. 来源记忆：借鉴快照/增量思想，但定义自己的状态契约

**事实。** Matrix 区分初始状态快照和后续状态变化；前面的 CloudEvents、Standard Webhooks 和 Cloudflare 资料并未规定 Agent 来源记忆集的写入语义。

来源：[Matrix v1.19 Sync](https://spec.matrix.org/v1.19/client-server-api/#get_matrixclientv3sync)。检索：2026-10-11。

**Waypost 建议。** 每个来源记忆集由 `space + authenticated source + collection` 定位；服务提供单调 revision，写请求携带 `expected_revision` 做 CAS。`snapshot` 替换明确声明范围内的全部当前内容；局部变化用 `delta`，避免“局部快照默认不删除”的含糊行为。完整原文、旧版本和输入信封不可变；当前 materialized view 可以变化，更新由新记录表达。

**不照搬。** 不把另一个来源的记忆写成此来源快照，不以供应商时间戳作为服务 revision，不把 blob 已落盘当“记忆已生效”。外部来源可能只导出可见文本或部分内容；`snapshot` 不应承诺抓到平台不可导出的隐藏记忆。

## 7. MCP 通用核心与 OpenAI Events 适配

**事实：MCP 基础。** 当前 2026-07-28 Base Protocol 采用每请求完整上下文与显式跨请求 handle；连接/进程不是 conversation。核心有 subscribe-and-notify 消息模式与 `subscriptions/listen` 流，这与任意平台的后台 webhook 唤醒不是同一个承诺。客户端自报的 clientInfo 不能用于安全决定。

来源：[MCP 2026-07-28 Base Protocol](https://modelcontextprotocol.io/specification/2026-07-28/basic)。检索：2026-10-11。

**事实：Cloudflare 路径。** 新服务器推荐 stateless `createMcpHandler`；应用数据可持久保存到 D1/R2 等，旧 `McpAgent` 已弃用。普通旧版 tool/resource 请求可兼容，但不能据此假设旧式持续流、重放和主动请求均可用。该 handler 的通知路由是 isolate-local。

来源：[Cloudflare handler APIs](https://developers.cloudflare.com/agents/model-context-protocol/apis/handler-api/)、[Remote MCP guide](https://developers.cloudflare.com/agents/model-context-protocol/guides/remote-mcp-server/)。检索：2026-10-11。

**事实：OpenAI 适配。** Official OpenAI documentation 明确其 ChatGPT MCP Events 接入取自 draft，支持 webhook 与 callback verification，不支持 draft 的 polling、streaming、gap/terminated 控制通知。该接入有 `events/list/subscribe/unsubscribe`、客户端提供 callback/secret、verification challenge、订阅刷新、Standard Webhooks 签名以及特定 eventId/name/data 格式；一次事件请求上限 256KiB，`2xx` 仅承认接收，后续处理异步进行。

来源：[Official OpenAI documentation: MCP Events](https://developers.openai.com/plugins/build/mcp-events)。检索：2026-10-11。

**Waypost 建议。** 通用业务信封/feed/回执/来源记忆 API 保持传输独立；通用 Skill 说明何时、如何收发，MCP 工具映射这些应用操作。OpenAI Events 可以是可选 adapter/profile，负责重新包装和唤醒。API `0.1` 不因 CloudEvents `specversion: 1.0` 或 MCP 协商版本变化而自动变化；各层单独协商/发现能力。

**不照搬。** `events/*` 方法、OpenAI callback 验证、`X-MCP-Subscription-Id`、256KiB、Work/dot 的后台行为及调度不能成为所有 MCP 客户端必需的 Waypost 核心。不得把 provider callback 的订阅 cursor 当某个消费者的已处理游标；一般 `subscriptions/listen` 也不是业务历史的唯一来源。

## 8. RFC 必须明确的歧义

以下为本次研究得到的规范问题，不是向用户追加审批。

1. **身份及 source。** 稳定 principal、producer、Agent、consumer、凭据分别是什么？单用户空间内能否为不同 Agent 限制来源、投递和记忆访问？服务如何绑定 CloudEvents source，防止请求自行冒名？
2. **唯一与冲突。** event ID、提交幂等 key、blob hash、visible_seq、memory revision 各自范围是什么？同 key 不同内容返回哪个稳定错误？撤销授权后重试是否仍必须拒绝？
3. **发布线性化。** R2 上传与 D1 提交之间/之后崩溃的结果是什么？事务什么时候分配 visible_seq？幂等重试会不会产生第二条 feed？CAS 未命中时是否保证无可见副作用？
4. **同步位置。** 游标绑定 consumer/view/filter/权限代次到什么程度？筛选结果为空时如何前进？跨页 snapshot high-watermark 如何保持一致？权限/筛选改变及历史保留到期如何显式重置，而不遗漏或越权回放？
5. **独立消费者。** 同一身份的多个安装是否是独立消费者？消费者从历史起点还是从当前创建？同步回执、Agent 处理回执与用户阅读状态是否分开？注销/停用消费者是否影响保留期？
6. **记忆集。** snapshot 的明确范围是什么？空 snapshot 是否清空该范围？delta 的新增/覆盖/删除、重复写、乱序及 expected_revision 冲突如何表达？源自第三方的 revision 只作来源 metadata 还是影响 CAS？
7. **不可变与删除。** 不可变信封能否被用户删除/撤回？blob 保留、tombstone、隐私删除与消费者历史如何协调？如果允许删除，应明确不可变的是已提交记录身份/内容而非永远保留所有字节。
8. **可选事件。** 通知代表变化还是可独立消费的全量记录？丢失通知后由哪个 API 恢复？单次大小、重试期限、订阅过期/撤销、callback URL 策略和 key 轮换属于核心还是 profile？
9. **版本与互操作。** Waypost API/schema、CloudEvents profile、MCP capability 与 provider profile 的支持矩阵如何公布？未知类型/字段如何处理？Skill 不能替代服务器授权或可靠后台机制。

## 9. 研究限制

本笔记核对一手文字契约，不证明某个第三方客户端、收费账号、运行版本或 UI 已通过端到端互操作。Matrix 模板实际正文未成功抓取，模板结构依据已读取的官方 Checklist；Matrix txn 章节与端点参数说明的措辞差异已单独记录。平台资料为检索日快照，实施前仍需按目标账号与最终 SDK 版本核对，并验证事务、并发和故障行为。
