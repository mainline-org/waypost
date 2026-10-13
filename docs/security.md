# 安全与数据管理

## 用户、助手和部署管理员

一个 Space 是一个用户的数据边界。部署管理员创建 Space，使用 `POST /admin/v0/spaces/<space_id>/owner-credential` 签发用户管理凭据，也可以在 Dashboard 的「用户登录凭据」按钮中操作。用户通过同一个 `/dashboard` 登录，只能查看自己的空间。

用户管理凭据和 Agent token 分开使用。用户可在本空间内接入助手、配置策略、查看内容和撤销本空间的 Agent token；不能创建别人的空间、签发其他用户的登录凭据、列出其他空间或执行全局维护。Agent 继续使用按 Space/Actor/Consumer/操作/资源授权的 token。

重新签发用户凭据需要 `expected_version`，响应只展示一次原始凭据。`GET .../owner-credential` 只返回版本；`DELETE .../owner-credential` 撤销当前凭据。轮换和撤销后，旧 bearer 与旧用户会话失效。部署管理员仍拥有全局控制和服务端解密权限；本方案不承诺防御被攻陷的服务运行时。

## 保存哪些密文

服务端使用 [HPKE（RFC 9180）](https://www.rfc-editor.org/rfc/rfc9180.html)：DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + ChaCha20-Poly1305，由 [Rust hpke 库](https://docs.rs/hpke/latest/hpke/)实现。每次加密使用新的随机封装；Space、对象或字段身份、格式版本和密钥 ID 进入认证上下文。

根密钥按 Space 派生独立 X25519 密钥对。数据密文无法移到另一个 Space、对象或字段下解密。新对象的存储名称含空间命名域。加密、管理登录、游标签名使用独立配置；根密钥只存在私密运行配置中，数据库和 R2 不保存根密钥。

| 数据 | 保存方式 |
|---|---|
| 消息正文、附件、隐私过滤副本、记忆 manifest | 对象存储中保存认证密文 |
| 消息主题、资料文件名、记忆路径及 metadata | SQLite/D1 字段中保存认证密文 |
| Space/Actor/Consumer 身份、显示名称、集合名称与 scope | 数据库中的运行信息 |
| 授权关系、资源 ID、时间、大小、内容类型、SHA-256、提交指纹 | 数据库中的运行信息 |
| Agent/用户管理凭据 | 数据库只存摘要，原始值签发时展示一次 |

服务端收到明文，完成授权和隐私过滤，再加密保存；授权读取时解密并检查内容完整性。客户端仍使用 HTTPS；其本地资料、出站队列和导出包需另行保护。加密保护存储副本，无法隐藏流量、大小或关系，也不会替代权限检查。

## 密钥配置与备份

原生 CLI 首次 `init` 会生成 `var/encryption-keys`，Unix 文件权限为 `0600`。备份 `metadata.sqlite` 和整个 `objects/` 目录时，将密钥另存于受控位置。默认文件位于数据目录；更严格的部署应通过 secret manager 注入 `WAYPOST_ENCRYPTION_KEYS`，使密钥与数据备份分离。

Worker 使用 Wrangler secret `WAYPOST_ENCRYPTION_KEYS`。生产环境缺少密钥或密钥格式错误时拒绝处理请求。格式如下，示例占位值不能用于实际部署：

```json
{"active":"v1","keys":{"v1":"<64 个十六进制字符，来自 32 字节安全随机数>"}}
```

`active` 选择新写入使用的根密钥 ID；最多保留四个 ID，旧密钥用于读取历史。轮换时先加入新密钥并切换 `active`，迁移旧密文后再移除旧密钥。不要复用管理凭据作为加密根密钥，不要丢弃仍被历史数据引用的旧密钥。

可单独配置至少 32 字节的 `WAYPOST_CURSOR_SECRET`，使管理凭据轮换不影响 Agent 游标。未配置时从管理凭据按用途派生，管理凭据轮换仍会使游标失效。此次升级改用空间内 feed 位置和新游标签名，旧游标需从起点重新读取；沿用已有 item ID 去重和原提交 ID，不会重复发布内容。

## 升级与旧数据迁移

先备份元数据、对象和现有密钥，再停止旧服务。应用八个迁移。旧实例没有加密密钥时，需要先生成并保管新的随机 keyring，再通过 `WAYPOST_ENCRYPTION_KEYS` 注入；程序不会为已有数据库静默补发一份可能错误的密钥。

原生服务停止后运行：

```bash
# 用 secret manager 或私密环境配置提供 WAYPOST_ENCRYPTION_KEYS
cargo run --release -- encrypt --data ./var
```

命令逐页迁移对象及敏感字段。新密文先写入独立对象，元数据批次再切换引用；中断后可以重试。旧对象保留到清理操作执行，避免迁移中途破坏原内容。原生 `serve` 与 `encrypt` 通过数据目录运行锁互斥。

Worker 迁移使用部署管理员 bearer 调用 `POST /admin/v0/maintenance/encrypt`。依次处理 `objects`、`messages`、`uploads`、`blobs`、`memory_items`，每次最多四条记录，根据 `has_more` 和 `next_cursor` 翻页：

```json
{"kind":"objects","dry_run":true,"limit":4,"cursor":null}
```

先用 `dry_run:true` 预览，再用 `false` 执行。迁移对象、敏感字段及旧根密钥密文使用同一接口。准备中的提交和未封存上传不在对象迁移范围内：完成它们或等待过期后清理。

默认严格拒绝未迁移的历史明文。过渡期间确有读取需要，可以暂时显式设置原生 `WAYPOST_ALLOW_LEGACY_PLAINTEXT=1` 或 Worker `ALLOW_LEGACY_PLAINTEXT="1"`；新写入仍加密。迁移完成后关闭此选项，清理旧对象，并按备份保留策略处理旧明文备份。此选项不会自动加密旧数据。

## 预览和清理暂存对象

部署管理员可调用 `POST /admin/v0/maintenance/cleanup`。用户管理凭据和 Agent token 不允许调用。浏览器管理会话还必须提供 CSRF token。

```json
{"dry_run":true,"limit":8,"cursor":null}
```

响应返回候选数量与分页游标，不返回对象内容。确认范围后，将 `dry_run` 设为 `false` 执行。按 `has_more` 与 `next_cursor` 翻页，或下次从空游标重新扫描。

清理包括到期的一小时暂存上传、失败提交的未引用对象、已提交但未引用的旧准备副本，以及迁移完成后的旧对象。有未发布对象的提交经过 24 小时重试窗口后会记录为 `SUBMISSION_EXPIRED`；不会删除已发布消息、已封存资料或历史记忆 manifest。

对象写入前先在元数据留下记录。清理取得五分钟租约后才调用对象删除；删除失败可在租约过期后重试。Cleaning tombstone 会持续保留，至少隔一小时重新扫描，以处理删除后才完成的迟到 I/O。对象清理无定时任务，需要管理员安排调用。

升级迁移只回填旧数据库已记录的上传和正文对象。旧版本在写入失败前没有登记的孤儿对象、原生崩溃留下的临时文件，不能仅凭现有元数据可靠识别；此接口不会盲目删除它们。检查此类历史对象时应先备份并人工核对引用。

## 运行边界

认证一次查询读取身份、授权和过滤快照；Agent 发布仍在提交批次内检查 token/Actor/Consumer 的有效版本。必须新增的行使用严格 INSERT，幂等占位只忽略指定唯一键，约束错误使整个 SQLite/D1 批次回滚。解码、配置、完整性与语句错误返回不可重试的 `INTERNAL_ERROR`；暂时的存储故障返回 `STORAGE_UNAVAILABLE`。

原生存储以单线程单连接 SQLite 运行，同步 fsync 会阻塞该 executor。原生对象路径按上限缓冲。未启用加密时 Worker 专化使用 R2 流式 I/O；启用 HPKE 时使用经过大小限制的缓冲加解密，Worker 单对象上限 256 KiB。ObjectStore trait 本身是缓冲字节契约，不承诺通用流式实现。
