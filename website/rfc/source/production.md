# Cloudflare 生产运行

Waypost 的 Cloudflare 部署使用独立 Worker、D1 和私有 R2。运行参数与本机配置分开；账户配置和管理凭据不进入公开仓库。

## 当前入口

- Dashboard：<https://waypost-production.christopherwuy.workers.dev/dashboard>
- MCP：`https://waypost-production.christopherwuy.workers.dev/mcp`
- 存活探测：`GET /health`
- 存储就绪：`GET /ready`，需要管理员 bearer

新用户通过 [申请邀请](https://waypost-production.christopherwuy.workers.dev/dashboard#request-access) 留下联系方式。运行者审核后私密送达一次性邀请，领取后获得独立空间和个人登录凭据。助手使用自己的限权连接，不能使用管理凭据作为普通连接。

## 配置与发布

先准备 Rust/Wasm、`worker-build 0.8.7` 和 Wrangler 4.111.0 或更新版本。登录后创建自己的独立资源：

```bash
wrangler login
wrangler d1 create waypost-production --location apac
wrangler r2 bucket create waypost-production-objects --location apac
cp wrangler.production.example.toml wrangler.production.toml
chmod 600 wrangler.production.toml
```

填写 `account_id`、`database_id`、bucket 和实际 `PUBLIC_ORIGIN`。在 Cloudflare Workers & Pages 查看账户的 `workers.dev` 子域名；入口形如 `https://<worker>.<subdomain>.workers.dev`。生产 Origin 必须是完整 HTTPS origin，不带尾随路径或占位地址。两个 rate-limit namespace 必须在账户内为此应用独立分配，避免与其他服务共用计数。

配置 `DEPLOYMENT_ENV=production`。生产运行时在限速绑定缺失或 Origin 无效时返回错误，不会静默降级。R2 不启用 `r2.dev` 或公共资料域名。

通过私密 `.env.production` 设置 `WAYPOST_ADMIN_SECRET`，使用至少 32 字节随机凭据并将文件权限设为 `0600`。当前实例的凭据保存在部署工作区 `var/production-admin-secret`；该文件、`.env.production` 和真实配置均被 Git 忽略。

```bash
wrangler d1 migrations apply waypost-production --remote --config wrangler.production.toml
wrangler deploy --config wrangler.production.toml --dry-run
wrangler deploy --config wrangler.production.toml --secrets-file .env.production
```

重新部署会保留服务数据。上线前核对目标账户、资源和旧 Worker Version ID。迁移使用具名文件；不要把开发数据库绑定到生产配置。

## 运行边界

所有响应禁止缓存并设置 `nosniff` 与 Referrer Policy；HTTPS 响应设置 HSTS。Dashboard 另有 CSP，管理会话使用 Secure、HttpOnly、SameSite=Strict，并对浏览器写入验证 CSRF 与 Origin。

生产配置默认：

| 路径 | 限速 |
|---|---:|
| 登录、申请与领取：POST `/admin/session`、`/admin/access-request`、`/admin/invitation` | 每 IP 每 60 秒 10 次 |
| 其他动态路径，包含会话检查和 MCP / HTTP API | 每 IP 每 60 秒 300 次 |
| 公开静态 GET 与 GET `/health` | 不使用应用限速绑定 |

超限返回 `429 RATE_LIMITED` 和 `Retry-After`。计数使用 IP 摘要，服务不在计数键中保存原 IP。Cloudflare 的计数按位置执行且为近似限制，不能视为全局严格配额或账单上限。[Rate Limiting API](https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/)

`/health` 表示请求处理可用；`/ready` 同时验证当前九个迁移和 R2 读取连接，且返回明确的 environment。实际上传、封存及下载还应通过完整示例检查。

请求大小、Blob、附件、集合和分页上限仍采用 [Worker profile](deployment.md#cloudflare-容量与费用)。服务通过 `waypost_describe` 返回实际值。应用限速不替代账户日用量、CPU、数据库及存储计划限制；部署不会自动升级账户计划。

## 邀请试用与支持

当前托管服务按邀请开放试用，尚未开放付费订阅，收费与正式服务方案待定。试用使用 Worker profile：单个文件最多 256 KiB、一封消息最多 4 个附件、当前集合的有效条目与删除标记合计最多 32 条；其他上限以连接后的 `waypost_describe` 为准。请求超过限制会被拒绝，不会自动扩容。账户级用量仍可能先于这些单次请求上限耗尽。

使用者应保留重要资料的本地副本，并定期导出。当前不承诺 SLA、响应时限或固定数据保留年限；完整生产 D1/R2 恢复演练尚未完成，原生离线恢复测试不能替代它。试用用户看到的公开说明见 [使用指南](https://waypost.hika.fyi/guide/#trial-support)。

非敏感故障与使用建议通过 [GitHub Issues](https://github.com/mainline-org/waypost/issues/new) 反馈，包含操作步骤、浏览器/客户端版本、发生时间和错误代码即可。不要贴登录凭据、助手连接配置、完整邀请链接、私人消息、资料或未脱敏日志。凭据遗失、邀请重发和涉及私人内容的问题，沿最初收到邀请的私密渠道联系运行者；公开 Issue 不能用于重置身份。处理前先撤销可能泄露的凭据。

## 上线验收

使用私密凭据文件运行：

```bash
python3 scripts/verify_deployment.py \
  --url https://your-service.example \
  --admin-secret-file var/production-admin-secret
```

验收检查健康、HTTPS 安全头、production 标记、D1/R2、Origin 拒绝、管理会话、CSRF 和退出失效。脚本不输出凭据或私有响应正文。

随后在独立试用空间运行 [官方 SDK](../examples/mcp-client) 与 [完整通信/记忆示例](demo.md)。最后可追加 `--check-rate-limit --check-api-rate-limit` 检查限速；这会暂时消耗验证者 IP 的登录和 API 预算，因此应放在其他检查之后。

参考客户端发送明确的 `Waypost/0.1.0` User-Agent。若客户端收到 Cloudflare `1010` 而非 Waypost JSON，应检查边缘的客户端签名策略；本次部署已实际复现默认 Python 标识被拒绝，并验证产品标识可用。[Cloudflare 1010](https://developers.cloudflare.com/support/troubleshooting/http-status-codes/cloudflare-1xxx-errors/error-1010/)

## 观察与恢复

生产模板开启 10% invocation 日志采样。Cloudflare 控制台可查看请求、状态和异常；应用不记录消息正文、管理密钥或原始隐私匹配值。后台检查成功率、503/429 与账户用量；日志配置见 [Workers Logs](https://developers.cloudflare.com/workers/observability/logs/workers-logs/)。

```bash
wrangler deployments list --config wrangler.production.toml
wrangler versions list --config wrangler.production.toml
wrangler d1 time-travel info waypost-production --config wrangler.production.toml
```

D1 Time Travel 当前保留 Free 7 天、Paid 30 天的恢复窗口。需要更长保留时，另行导出数据库并保存不可变资料的完整副本。[D1 Time Travel](https://developers.cloudflare.com/d1/reference/time-travel/)

Worker 回滚使用此前确认的版本：

```bash
wrangler rollback <previous-version-id> --config wrangler.production.toml
```

回滚代码不会回滚数据库或 R2。数据库恢复前记录当前 bookmark、停止写入，并确认目标 Worker 与目标 schema 兼容；恢复后再次运行完整验收。R2 中的历史对象与关系状态必须共同保留，仅导出记忆摘要不能构成服务备份。

对外扩大托管规模前，还应在独立 D1/R2 资源完成恢复演练：从同一停写边界保存数据库、全部对象和独立密钥，恢复到新资源，使用兼容 Worker 验证历史消息、附件与资料版本，并记录恢复时长。演练应使用合成数据或专门获准的数据，不在生产空间创建测试内容。恢复点后的授权撤销也需要重新应用。

2026-10-15 已用生产只读快照完成一次本地恢复核验（`scripts/verify_cloudflare_restore.py`）：14 个对象共 5905 字节全部下载并与库内校验值一致，引用闭包成立；快照恢复到全新本机目录并起服，5 个空间、9 条消息、4 个资料版本项逐项读回，附件字节一致；生产零写入。完整结果见[验证记录](validation.md)。

## 首次发布记录

2026-10-12 首次发布，Worker Version：`79117c74-ec90-4b41-8864-3b397e1a3d28`，运行时代码提交：`177eac4`。独立 APAC D1 的六个迁移已应用，R2 Standard bucket 的公开 URL 已确认关闭。发布过程没有启用新付费计划。

生产验收结果以 [验证记录](validation.md) 为准。后续发布会产生新版本，以上 ID 是首次交付的追溯记录。

## 加密与用户隔离版本升级

服务端 HPKE 内容加密、用户空间管理凭据和迁移 `0007` / `0008` 已在生产启用。本次接入工作开始时，线上就绪检查已确认八迁移版本；上文六迁移版本保留为首次交付记录。

从旧版本升级前备份 D1/R2 与密钥，配置私密 `WAYPOST_ENCRYPTION_KEYS`（推荐同时配置独立 `WAYPOST_CURSOR_SECRET`），应用全部迁移，再按 [安全说明](security.md) 迁移旧内容。生产 Worker 缺少加密 keyring 会拒绝请求。若过渡期间临时允许旧明文读取，完成迁移后关闭 `ALLOW_LEGACY_PLAINTEXT` 并清理旧对象。

密文与字段迁移后，旧明文版本不能直接读取新数据。回滚应匹配代码、数据库、对象和密钥的备份版本；不要直接把旧 Worker 覆盖到已经迁移的存储上。此次更新改变游标格式，客户端需重新读取并按 item ID 去重。

## 邀请与主助手上线（2026-10-13）

代码已通过 PR [#6](https://github.com/mainline-org/waypost/pull/6) 合并到 main，提交 `9d989ff`。Worker Version：`35877fca-1f73-4eb8-8cfa-7085c70728af`，就绪检查返回 `schema_revision: 9`。

上线前保存了完整 D1 导出、恢复点、旧 Worker 版本与私密配置。迁移 `0009` 添加主助手、连接默认收件人、邀请和申请字段，不改写 R2；发布保留现有加密 keyring。D1 与配置备份不替代完整 R2 备份。

真实 HTTPS 入口通过健康、安全头、D1/R2、Origin、CSRF 与退出失效检查。标准 MCP 客户端完成接入助手投递、同请求恢复、主助手可靠保存后确认、向用户汇报与 processed；只有发信能力的连接被拒绝读取信箱。真实 HTTP + DOM 完成申请、审批、一次性领取、个人空间、主助手连接预设与默认投递验证。

验收使用独立示例空间模拟助手角色，没有调用真实 Muse 产品或执行餐厅订单。当前邀请通过私密渠道手动送达，不自动发送邮件。
