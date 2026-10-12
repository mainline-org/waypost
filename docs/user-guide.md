# 接入与授权

每位用户使用自己的 Space，在 Dashboard 中使用用户登录凭据管理。部署管理员负责创建 Space 和签发用户凭据。需要同时使用两个用户的资料时，分别接入两份授权连接；修改资源 ID 不会跨越空间边界。

## 默认连接能做什么

「接入 Agent」为助手创建身份、独立 Consumer、自己的记忆集合和连接凭据。助手可以向本空间的用户入口发信、收取投递给自己的消息、上传资料并管理自己的记忆。MCP 的 `waypost_describe` 返回实际权限与上限。

助手是否自动收信取决于宿主的调度。可以手动请助手检查 Waypost，也可以在具备调度能力的宿主中安排定时检查。

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

在发信助手的私密配置中更新连接凭据，先查看身份与权限，再请它明确向授权 Actor ID 发信。收信助手检查自己的信箱，并在可靠保存或处理后确认回执。

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

在 Dashboard 的「凭据」页撤销对应连接。HTTP 也支持 `DELETE /admin/v0/spaces/<自己的 space_id>/tokens/<token_id>`。撤销后，旧凭据不能继续读取或发布；已合法发布的历史内容保持原有记录。
