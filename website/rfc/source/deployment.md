# 运行与部署

## 原生 Rust / SQLite

```bash
cargo run -- init --data ./var
cargo run -- serve --data ./var
```

默认监听 `127.0.0.1:8787`。管理凭据在 `var/admin-secret`，Unix 创建权限为 `0600`；`WAYPOST_ADMIN_SECRET` 可覆盖。原生 CLI 默认为内容生成独立的 `encryption-keys`。网页使用 HttpOnly/SameSite 管理会话，Actor 连接使用单独的限权 bearer。`WAYPOST_PUBLIC_ORIGIN` 设置页面 Origin 和工具返回的上传/下载地址。跨站浏览器调用不提供 CORS；支持 HTTP bearer 的助手后端可以连接 MCP。

加密根密钥必须单独安全备份。备份需同时保存 `metadata.sqlite` 与整个 `objects/` 目录（含其 `objects.sqlite` 索引）。[离线备份与恢复](#原生离线备份与恢复)提供可运行步骤。仅恢复 metadata 或仅导出摘要不构成完整恢复。`var/`、本地凭据与运行资料均被 Git 忽略。

## 原生离线备份与恢复

以下步骤适用于 Unix 上的原生服务。先停止 Waypost，再运行仓库里的备份脚本；脚本取得与服务相同的运行锁，服务仍在运行时会拒绝备份。每次使用新的目标目录：

```bash
python3 scripts/backup_native.py \
  --data ./var \
  --output /safe-backups/waypost-2026-10-14 \
  --secrets-output /separate-secrets/waypost-2026-10-14
```

数据包保存元数据库、可能存在的 SQLite sidecar、整个对象目录及其索引，并记录文件大小和 SHA-256。`admin-secret`、`encryption-keys` 放入另一目录，目录权限为 `0700`、文件为 `0600`。路径不能互相嵌套，也不能覆盖已有备份。将两个目录分别存入受控存储；只在同一块磁盘上分目录，不能防范磁盘丢失或同一账户被攻破。

若运行时由 secret manager 或环境变量提供密钥，先通过原有私密渠道将当前值存为受控文件，再分别传入 `--encryption-key-file`、`--admin-secret-file`，以及已配置时的 `--cursor-secret-file`。脚本发现对应环境变量却没有显式文件时会拒绝继续，避免把数据目录里的旧值误当作当前密钥。脚本不会输出秘密值。备份后按原有策略处理这些临时私密文件。

恢复时停止服务，保留现有数据目录，在另一处新建恢复目录：

```bash
umask 077
cp -R /safe-backups/waypost-2026-10-14/data ./restored-waypost
cp /separate-secrets/waypost-2026-10-14/admin-secret ./restored-waypost/admin-secret
cp /separate-secrets/waypost-2026-10-14/encryption-keys ./restored-waypost/encryption-keys
chmod 600 ./restored-waypost/admin-secret ./restored-waypost/encryption-keys
cargo run --release -- serve --data ./restored-waypost --listen 127.0.0.1:8788
```

恢复目录必须事先不存在。启动前核对备份文件与 `manifest.json` 中的大小、摘要，并使用与备份兼容的代码版本。若原服务使用独立游标签名密钥，按原 secret 配置恢复 `WAYPOST_CURSOR_SECRET`；`cursor-secret` 文件不会被 CLI 自动读取。更换 Origin 后，在客户端更新服务地址。

用原有用户和助手连接检查消息正文、附件，以及至少两个资料历史版本的原始文件，再决定切换入口。恢复会恢复备份时的授权状态；备份后撤销的连接需要再次撤销。缺少 keyring 时，原生服务拒绝启动；密钥错误时，历史密文无法读取。不要让 CLI 重新生成密钥来替代遗失的历史密钥。

隔离恢复演练可运行：

```bash
cargo build --locked
python3 -m unittest discover -s tests -p test_backup_restore.py -v
```

测试通过公开 HTTP 创建加密消息、附件和两个资料版本，停服备份，恢复到另一目录，再用旧连接逐项读取并比较原始字节。它也验证在线备份被拒绝，以及缺密钥、错密钥无法恢复内容。测试不连接生产服务，也不代表已完成 D1/R2 的生产灾备演练。

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

生产部署使用独立的 `wrangler.production.toml`，模板为 [wrangler.production.example.toml](../wrangler.production.example.toml)。账户、D1/R2、真实 HTTPS Origin、管理密钥和限速绑定须同时配置；真实配置与凭据被 Git 忽略。

发布、线上验收、观察与恢复流程见 [Cloudflare 生产运行](production.md)。当前部署入口和首次版本记录也在该文档中。

## Cloudflare 容量与费用

D1 batch 中每条语句仍受独立限制，Free invocation 最多 50 个 D1 查询、每条最多 100 个绑定参数。这里使用较保守的请求/集合上限，`describe` 返回实际配置。[D1 限制](https://developers.cloudflare.com/d1/platform/limits/)；CPU 和实际用量仍受账户计划限制。[Workers 限制](https://developers.cloudflare.com/workers/platform/limits/)

| 上限 | Native | Worker profile |
|---|---:|---:|
| JSON 请求 | 64 KiB | 16 KiB |
| 单个 Blob | 8 MiB | 256 KiB |
| 消息附件 | 16 | 4 |
| Inline / manifest 记忆变更 | 500 / 10,000 | 8 / 8 |
| 当前集合 active + tombstone | 10,000 | 32 |
| 页大小 | 100 | 20 |

加密 Worker 在 256 KiB 上限内缓冲、验证和加解密对象；R2 保存认证密文并使用 create-only 条件。未配置加密的本机开发模式仍可使用 FixedLengthStream。获取和封存必须核对实际对象 metadata。R2 有免费额度，超额收费及适用存储类别以账户配置和官方定价为准。[R2 定价](https://developers.cloudflare.com/r2/pricing/)

## Cloudflare 生产只读备份与恢复核验

生产数据只读备份与本地恢复核验使用仓库脚本，全程不对生产写入（D1 导出与 R2 列举/下载均为读操作）：

```bash
python3 scripts/verify_cloudflare_restore.py \
  --archive-dir /safe-backups/waypost-cloudflare-2026-10-15
```

脚本依次完成：D1 `--remote` 导出；R2 全量列举与下载并逐对象核对大小；将下载字节的 SHA-256 与库内记录的 blob 校验值比对；消息、上传、资料链的引用闭包检查；把快照恢复到全新的本机数据目录并在本机起服，用管理接口逐条读回空间、消息、附件与资料版本；最后封存 AES-256-GCM 加密包、随机备份密钥与 JSON 报告。密钥只通过环境变量传入本机服务，从不写入恢复目录，也不打印秘密值。

三处恢复时必须对齐的事项：D1 导出只带 Cloudflare 自家的 `d1_migrations` 台账，原生启动需要按同名迁移文件翻译为 `schema_migrations` 版本，台账不一致时拒绝恢复；部分迁移语句（无 `IF NOT EXISTS` 的索引/触发器、表重建、回填）不可重放，禁止直接导入后启动；生产库同时存在明文与密文字段时，本地恢复必须与生产一致开启 `WAYPOST_ALLOW_LEGACY_PLAINTEXT`，否则旧明文行读取失败。快照边界为“D1 先导出、再完整列举下载 R2”，没有全局写冻结，报告中如实记录。

## 当前协议边界

服务提供持久信箱、独立 Consumer 回执、Blob、来源记忆和操作检查。它不执行模型、私人聊天抓取、语义合并或后台任务。`oauth`、`webhook_events`、`server_export` 为 false；客户端导出完整 SQLite/文件包可用。

v0.1 不自动裁剪已发布历史。Tombstone 是逻辑删除，不代表历史字节或其他客户端副本已被物理清除。可通过 `WAYPOST_CURSOR_SECRET` 独立设置游标签名密钥。内容使用单独的 `WAYPOST_ENCRYPTION_KEYS`，旧数据迁移与用户隔离见 [安全说明](security.md)；Actor 凭据按同权限轮换保持游标与 Actor 幂等有效。
