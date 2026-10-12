> 这份评审记录保留改进前的判断。2026-10-13 的修复与新增加密/用户隔离见 [安全说明](../security.md) 和 [验证记录](../validation.md)。其中需要校正一项事实：SQLite `OR IGNORE` 不会忽略外键错误；它会静默跳过 UNIQUE、NOT NULL 或 CHECK 冲突。[SQLite 官方说明](https://www.sqlite.org/lang_conflict.html)

# 架构评审：Waypost v0.1（2026-10-12）

| 项目 | 值 |
|---|---|
| 对象 | 分支 `feat/cloudflare-production`，提交 `177eac4` 及未提交工作区 |
| 方式 | 静态审查：源码、迁移、设计文档、测试、CI 配置；未运行 cargo test / clippy |
| 基线 | [RFC-0001](../rfcs/0001-async-mailbox-and-memory-sync.md)、[数据模型](../design/0001-rust-data-model.md)、[ADR-0001](../adr/0001-internal-sql-driver.md) |

## 1. 总体评价

架构主线清晰，执行扎实。一个 Rust crate，`App<D, O>` 领域核心对两个很薄的 trait 泛型：`Database`（只有 query/batch）和 `ObjectStore`。SQLite/文件与 D1/R2 两套适配器各约 130 行。HTTP 与 MCP 都落到同一组领域方法上，幂等、CAS、授权检查没有在传输层复制。文档体系（CONTEXT.md、RFC、性质规范、ADR、validation）与代码的一致程度高。

最核心的设计是"reserve → prepare → 带 guard 的 batch 提交"。每条写语句都带 `attempt_nonce + state='prepared' + authority_guard` 条件，最后一条 UPDATE 是唯一提交点，提交后重新读 submission 行而不是信任 batch 返回值。这是为 D1 没有交互式事务量身定做的方案，在 SQLite 上同样成立，是整个项目最值得保留的决定。

## 2. 做得好的地方

- **存储边界收口正确。** ADR-0001 把 SQL 收进 crate-private 的 `storage::driver`，避免 SQLite 和 D1 各自复制一套领域规则。
- **授权在两端检查。** 入口 `authenticate` 建立 Principal，提交语句里 `authority_guard` 再次绑定 token/actor/consumer 的版本号，撤销和轮换在提交瞬间生效（P14）。
- **游标与视图绑定。** HMAC 签名的游标包含 space、actor、consumer、view_revision、filter 指纹和 grant 指纹，换 Consumer 或改权限立即返回 409（P09）。
- **回执 feed 存快照。** `feed_receipt_updates` 保存推进时的阶段而不 join 当前回执，历史阶段不会被改写。
- **输入面严格。** JSON 解析拒绝重复键，DTO 用 `deny_unknown_fields`，幂等指纹用 JCS 规范化，所有 ID 字符集受限，对象 key 由服务生成。
- **验证分层诚实。** 进程内 proptest、真实 HTTP 的 Python 测试、CI 里跑真实 workerd + D1 + R2，validation.md 明确写了哪些没验证。

## 3. 主要风险与建议

按优先级排序。

### 3.1 领域核心是"字符串 SQL + 无类型 Row"，不是设计文档承诺的类型化聚合

设计文档 §4 定义了 `Submission`、`SubmissionState`、`FeedItem` 等类型；实现里全部是 `text(&row,"state")?` 加 `format!` 拼接的几百字符 SQL，参数靠位置对齐。典型例子是 `src/service/receipts.rs:115` 到 141 的五条语句。任何列名或参数顺序错误只能靠运行时测试发现，Rust 类型系统几乎没有参与。v0.1 为了交付可以接受，但这是最大的可维护性风险。

建议：至少把 submission、feed_item、upload 三类行解码成结构体；SQL 保留，但按操作命名集中到一个模块，参数用命名常量而不是裸位置。

### 3.2 `INSERT OR IGNORE` 让约束违反静默通过

所有 guard 插入都经过 `src/service/mod.rs:442` 的 `push_rows`，用的是 OR IGNORE。FK 或 UNIQUE 冲突会丢掉该行，但最后的 `state='committed'` 仍然成功，可能出现消息已提交而 `message_recipients` 缺行。SQLite 事务和 D1 batch 本来都会因错误整体回滚。

建议：对"必须是新行"的插入去掉 OR IGNORE。失败会变成可重试的 503，而不是半个消息。

### 3.3 D1 上每个请求的查询轮次偏多

`authenticate` 固定发 6 次查询；一次 `send` 算上 reserve、prepare、commit 和多次 `current()` 约 20 次往返。D1 每次往返是网络调用，Free 档每次调用上限 50 次查询。

建议：把 authenticate 合并成一次 `json_group_array` 查询；去掉已被 `authority_guard` 覆盖的重复 `current()`。

### 3.4 没有任何垃圾回收

设计文档 §4.5 有 `Cleaning` 状态和租约，但迁移 0003 里 `uploads.state` 只有 pending/sealed，`ObjectStore::remove` 在服务代码里没有任何调用方（已逐文件核对）。过期未封存的上传字节、prepared 但未 committed 的正文、隐私过滤生成的 `filtered_*` 副本、记忆 manifest 都永久留在对象存储里。

建议：至少提供一个管理员可触发的清理操作，按租约与已发布引用判断可删对象。

### 3.5 管理密钥身兼三职

它同时是管理员 bearer、游标 HMAC 密钥（`src/service/mod.rs:353`）和 CSRF 密钥。轮换管理密钥会让所有 Agent 的游标失效，deployment.md 已承认这一点。

建议：用 HKDF 从它派生带标签的 cursor key，或者单独配置一个，解耦控制面和数据面。

### 3.6 路由在三处手工解析

核心 `route`、Cloudflare `handle_fetch`（限速分类和流式上传下载路径）、原生 server（按路径选 body 上限）各自 split 路径。改一个路由要动三个文件。

建议：核心里提供 `Route::parse(method, path)` 枚举，适配器只 match 变体。

### 3.7 错误分类把 bug 伪装成故障

`Error::storage()` 覆盖了行解码失败、配置不合法（admin secret 太短也返回 503）、Worker 顶层所有 `Err(_)`。客户端看到的都是 retryable 的 `STORAGE_UNAVAILABLE`，schema 不匹配会被当成临时故障无限重试。

建议：区分不可重试的 `internal`（记录原因，不返回给客户端）和真正的 `storage`。

### 3.8 ObjectStore trait 不是真正的契约

`create_only` 只接受 `&[u8]`，原生路径整份缓冲 8 MiB。Cloudflare 为了流式上传绕过 trait 直接访问 `self.objects.bucket`，下载时的 sha/size 校验在 `blob_content` 和 `streaming_download` 各写一遍。

建议：把流式类型放进 trait，或在文档里明说 Worker 适配器是特化实现。

## 4. 次要问题

- 迁移清单在原生 `include_str!` 列表、D1 readiness 的硬编码文件名和 `schema_revision:6` 三处重复，加一个迁移要改三处。
- `Send::validate` 和 `Limits` 对附件数有两套上限。
- feed 序号跨空间共享，会泄露其他空间的活动量。设计文档已接受。
- 单线程单连接 SQLite 意味着一次 fsync 阻塞所有连接。设计文档已明确不引入线程池，但吞吐上限应写进部署文档。

## 5. 结论

这是一个边界划得清楚、关键不变量落实到了提交语句里的 v0.1，适合继续演进。最需要投入的是把 SQL 层从字符串拼接推进到有类型的行与语句，其余问题都是局部修改。
