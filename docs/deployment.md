# 运行与部署

## 原生 Rust / SQLite

```bash
cargo run -- init --data ./var
cargo run -- serve --data ./var
```

默认监听 `127.0.0.1:8787`。管理凭据在 `var/admin-secret`，Unix 创建权限为 `0600`；`WAYPOST_ADMIN_SECRET` 可覆盖。网页使用 HttpOnly/SameSite 管理会话，Actor 连接使用单独的限权 bearer。`WAYPOST_PUBLIC_ORIGIN` 设置页面 Origin 和工具返回的上传/下载地址。跨站浏览器调用不提供 CORS；支持 HTTP bearer 的助手后端可以连接 MCP。

备份需同时保存 `metadata.sqlite` 与整个 `objects/` 目录（含其 `objects.sqlite` 索引）。停止服务后复制可获得一致的原生备份。仅恢复 metadata 或仅导出摘要不构成完整恢复。`var/`、本地凭据与运行资料均被 Git 忽略。

## Docker

```bash
docker compose up --build -d
docker compose exec waypost cat /data/admin-secret
```

打开 `http://127.0.0.1:8787/dashboard`。命名 volume 持久化状态与资料；容器运行用户为 UID 10001，Compose 仅发布本机端口。远程入口应设置实际 `WAYPOST_PUBLIC_ORIGIN`，由部署方提供 HTTPS。容器配置与原生服务使用同一套迁移。

## Cloudflare 本机 D1/R2

需要 Rust/Wasm target、`worker-build 0.8.7` 和 Wrangler。

```bash
rustup target add wasm32-unknown-unknown
cargo install worker-build --version 0.8.7 --locked
cp .dev.vars.example .dev.vars
# 为 .dev.vars 设置自己的至少 32 字节管理凭据
wrangler d1 migrations apply waypost --local
wrangler dev --local --port 8791
```

使用 `http://127.0.0.1:8791`，与配置的 Origin 保持一致。此模式只创建本机资源，不需要远程数据库或 bucket。已用本机 Workerd、D1/R2 和官方 MCP SDK 跑通完整例子。

## Cloudflare 远程部署

仓库不包含账户凭据、真实 database ID 或管理密钥。登录后创建自己的 D1/R2，替换 `wrangler.toml` 的 database ID、bucket 名和 `PUBLIC_ORIGIN`：

```bash
wrangler login
wrangler d1 create waypost
wrangler r2 bucket create waypost-objects
wrangler secret put WAYPOST_ADMIN_SECRET
wrangler d1 migrations apply waypost --remote
wrangler deploy
```

部署前核对这些命令操作的账户和资源；当前项目没有自动开启付费计划或创建远程资源。已验证的是本机运行时，当前环境的 Cloudflare 登录已过期，尚未部署在线服务，也未测量托管环境的生产 CPU。

D1 batch 中每条语句仍受独立限制，Free invocation 最多 50 个 D1 查询、每条最多 100 个绑定参数。这里使用较保守的请求/集合上限，`describe` 返回实际配置。[D1 限制](https://developers.cloudflare.com/d1/platform/limits/)；CPU 和实际用量仍受账户计划限制。[Workers 限制](https://developers.cloudflare.com/workers/platform/limits/)

| 上限 | Native | Worker profile |
|---|---:|---:|
| JSON 请求 | 64 KiB | 16 KiB |
| 单个 Blob | 8 MiB | 256 KiB |
| 消息附件 | 16 | 4 |
| Inline / manifest 记忆变更 | 500 / 10,000 | 8 / 8 |
| 当前集合 active + tombstone | 10,000 | 32 |
| 页大小 | 100 | 20 |

R2 上传直接使用 FixedLengthStream、create-only 条件和服务端 SHA-256 校验；原始文件不在 Worker 中整份缓冲后重复哈希。获取和封存必须核对实际对象 metadata。R2 有免费额度，超额收费及适用存储类别以账户配置和官方定价为准。[R2 定价](https://developers.cloudflare.com/r2/pricing/)

## 当前协议边界

服务提供持久信箱、独立 Consumer 回执、Blob、来源记忆和操作检查。它不执行模型、私人聊天抓取、语义合并或后台任务。`oauth`、`webhook_events`、`server_export` 为 false；客户端导出完整 SQLite/文件包可用。

v0.1 不自动裁剪已发布历史。Tombstone 是逻辑删除，不代表历史字节或其他客户端副本已被物理清除。管理凭据也是当前游标签名密钥，更新它会使旧管理会话和旧游标失效；Actor 凭据按同权限轮换保持游标与 Actor 幂等有效。
