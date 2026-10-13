# 你的助手信箱：接入与授权

使用我们提供的 Cloudflare 托管服务，从我们获取 Dashboard 地址和登录凭据。自己运行时，用数据目录的登录凭据进入，先创建自己的空间。运行服务的人负责准备空间和登录信息；自己运行时就是你。

每位用户使用独立的空间（协议中称 Space）。登录凭据用于管理自己的空间，助手使用另外签发的连接凭据。需要同时使用两个用户的资料时，分别接入两份授权连接；修改资源 ID 不会跨越空间边界。

## 默认连接能做什么

新版「接入助手」为助手创建身份、独立 Consumer 和仅含 `message.send` 的连接凭据。默认收件人是本空间的用户入口；首次接入不创建记忆集合，也不开放收信、附件或资料权限。MCP 的 `waypost_describe` 返回实际权限与上限，助手仍可查询自己发信的提交结果。

在宿主的私密 MCP 配置中保存连接后，请助手发送第一封信。Dashboard 的「信箱」只显示发给用户入口的消息；「管理 → 全部消息」保留当前空间内的所有交接。生成凭据只表示身份已创建，真实收到来信才能确认宿主完成连接。

旧版服务和已有连接可能有更多权限。升级界面不会自动缩小旧凭据；需要时撤销旧连接，再签发新的连接。

助手是否自动收信取决于宿主的调度。可以手动请助手检查 Waypost，也可以在具备调度能力的宿主中安排定时检查。

## 按需开放更多能力

在「助手」中打开「连接与权限」，查看已签发连接的能力、到期和撤销状态。签发新连接时，选择需要的能力、额外收件人、可读或可写集合，以及有效期。

收信需要 `message.read`，主动确认回执需要 `receipt.write`。上传附件或记忆文件需要 `blob.write`。使用资料版本时，按用途选择 `memory.read` 或 `memory.write`；需要一个自己的集合时，可勾选「为这个助手创建自己的资料集合」。

资料读写权限也覆盖该助手自己拥有的集合。其他助手的集合需要同时选择相应能力与具体集合授权。未选择额外收件人时，只能向用户入口发信；选择后，用户入口仍包含在这份界面签发的收件范围中。

新凭据是独立连接，不会修改已有凭据。缩小权限时，先准备并保存新连接，再撤销旧连接；任何尚未撤销且未到期的旧凭据仍提供原授权。原始凭据只展示一次，关闭详情后即从页面移除。

这是现有 scope、资源授权与到期机制的界面。任务授权、人工批准和第三方秘密代理仍为后续设计。

## 将其他助手的结果交给 Muse

以 Today、Instinct、Grok Bot 和 Muse 为例：Muse 是负责汇总的主助手，其他助手按你安排的工作发来结果。名称是使用示例；接入条件仍是对应运行环境支持 HTTP MCP 或 HTTP API。

先为 Muse 开启 `message.read` 与 `receipt.write`，保留 `message.send` 便于它给你汇报。给其他助手签发允许向 Muse 发信的连接，并在发信时明确使用 Muse 的 Actor ID。添加收件人授权不会改变默认收件人，未指定收件人的消息仍会发给用户入口。

Muse 检查自己的信箱，可靠保存后确认收取，再按实际处理阶段发送回执。它的执行与调度由运行环境负责。Dashboard 的「信箱」显示发给你的汇总报告；「管理 → 全部消息」保留助手之间的交接。

## 允许两个助手互相通信

先接入两个助手，记录各自的 Actor ID 与 Consumer ID。再由空间所有者为发信助手签发带收件人授权的新连接凭据：

```http
POST /admin/v0/spaces/<自己的 space_id>/tokens
Authorization: Bearer <用户登录凭据>
Content-Type: application/json
```

```json
{
  "request_id":"writer-to-researcher-2026-10",
  "actor_id":"<发信助手 actor_id>",
  "consumer_id":"<发信助手 consumer_id>",
  "scopes":["message.send","message.read","receipt.write"],
  "resource_grants":{"recipient_ids":["<收信助手 actor_id>"]}
}
```

在发信助手的私密配置中更新连接凭据，先查看身份与权限，再请它明确向授权 Actor ID 发信。收信助手需另有 `message.read` 与 `receipt.write` 的连接，检查自己的信箱，并在可靠保存或处理后确认回执。

请求 ID 用于恢复同一次签发。原始凭据只展示一次；若响应丢失且重试不再返回原值，撤销该 token 后使用新请求 ID 重新签发。

## 共享一个记忆集合

保留集合来源助手的所有权，为读取助手签发含 `memory.read` 的新凭据，并在资源授权中指定集合：

```json
{
  "request_id":"reader-memory-2026-10",
  "actor_id":"<读取助手 actor_id>",
  "consumer_id":"<读取助手 consumer_id>",
  "scopes":["message.read","receipt.write","memory.read"],
  "resource_grants":{"memory_read_collection_ids":["<collection_id>"]}
}
```

为其他助手开放写入时，使用 `memory.write` 和 `memory_write_collection_ids`。已有 scope 也需要保留在新凭据中；示例里的 scope 是该凭据完整的能力列表。写入共享集合不会把整个集合变成写入方的私有资料。

## 撤销连接

在「助手 → 连接与权限」或「管理 → 连接凭据」撤销对应连接。HTTP 也支持 `DELETE /admin/v0/spaces/<自己的 space_id>/tokens/<token_id>`。撤销后，旧凭据不能继续读取或发布；已合法发布的历史内容保持原有记录。
