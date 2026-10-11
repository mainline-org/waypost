# 0003：Rust Workers 与最小 MCP 实现依据

- 检索日期：2026-10-12。
- 目标：纯 Rust 领域核心、原生单线程 HTTP + SQLite、Worker D1/R2 适配器共享业务契约。
- 范围：仅核对官方文档与源码，不编写服务代码，不部署，不声称远端存储或 Wasm 已通过验证。
- 版本：MCP **2025-11-25**；`worker 0.8.7`；官方 `rmcp/main` 当时为 **3.5.1**。最新 SDK 已包含更新的 MCP 生命周期，不能混用。
- workers-rs 精确观察：`b171cb4bdd6fa943bb81f429df0a6b0df37e14b3`，提交时间 2026-10-03T00:40:16Z；D1、R2 builder、Cargo 与 checksum 通过 GitHub API 核对固定源码。下列 API 片段是研究用法，尚未在本项目编译。

## 1. 运行环境边界

**事实。** 官方 Rust MCP SDK 使用 Tokio；当前 3.5.1 要求 Rust 1.88，默认依赖含 Tokio sync/macros/rt/time。README 同时说明旧 initialize 生命周期与 2026-07-28 discover 生命周期。[rmcp README](https://github.com/modelcontextprotocol/rust-sdk)、[workspace Cargo](https://github.com/modelcontextprotocol/rust-sdk/blob/main/Cargo.toml)、[rmcp Cargo features](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/Cargo.toml)

**事实。** workers-rs 的默认目标为 `wasm32-unknown-unknown`，常规 Tokio runtime 不是可直接搬入的宿主运行时；新实验路径为 `worker-build --emscripten` 与 `experimental_tokio`，不能与默认构建混为一谈。[workers-rs README：Emscripten 与 FAQ](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/README.md)、[worker 0.8.7 Cargo](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/Cargo.toml)

**实施建议。**

- 核心不依赖 Tokio、socket、文件系统、rusqlite 或 Cloudflare 类型，保持现有领域模型和操作。
- SQLite/文件存储在原生适配层；D1/R2/JsValue 在 Worker 适配层。协议封装共享数据和业务语义。
- 可用原生 Tokio current-thread，不代表 Worker 可运行同一 server。也不能因为 rmcp feature 名含 `transport-worker` 就认定它是 Cloudflare 适配器。
- 若需要一个最小 stateless MCP envelope 层，可按固定版本规范实现并用真实 SDK 客户端验证；不要未经编译和互通测试宣称 rmcp 可直接运行 Worker。
- ports 不强迫所有 future 都为 Send：Worker ByteStream 明确 !Send/!Sync；需要的原生 Send 约束放在相应适配边界。

## 2. D1 prepare/bind/batch 精确形状

固定源码中的签名如下；`Result` 为 `worker::Result`，`JsValue` 来自 wasm-bindgen。[D1 Rust 源码](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/d1/mod.rs)

```rust
D1Database::prepare<T: Into<String>>(&self, query: T)
    -> D1PreparedStatement
D1Database::batch(&self, statements: Vec<D1PreparedStatement>)
    -> impl Future<Output = Result<Vec<D1Result>>>

D1PreparedStatement::bind(self, values: &[JsValue]) -> Result<Self>
D1PreparedStatement::run(&self)
    -> impl Future<Output = Result<D1Result>>
D1PreparedStatement::all(&self)
    -> impl Future<Output = Result<D1Result>>
D1PreparedStatement::first<T>(&self, col_name: Option<&str>)
    -> impl Future<Output = Result<Option<T>>>
// T: for<'a> Deserialize<'a>

D1Result::success(&self) -> bool
D1Result::error(&self) -> Option<String>
D1Result::results<T>(&self) -> Result<Vec<T>>
D1Result::meta(&self) -> Result<Option<D1ResultMeta>>

D1ResultMeta { changes: Option<usize>, last_row_id: Option<i64>, ... }
```

上述为整理后的签名示意，省略 impl、泛型完整约束与方法体，并非可直接粘贴的 Rust 文件。

**事实。** batch 按输入顺序返回，每个 statement 的结果保持对应；执行错误会回滚整批。SQL success 与 changes 分开：合法条件 UPDATE 命中 0 行并不构成 SQL 错误。[D1 Database batch](https://developers.cloudflare.com/d1/worker-api/d1-database/#batch)、[D1 return object](https://developers.cloudflare.com/d1/worker-api/return-object/)

**实施陷阱。**

1. `prepare` 后不能加 `?`；`bind` 消费旧 statement，返回新对象。重复绑定可 clone 或用 bind_refs。
2. 需要查询行用 all/first；当前 IntoFuture 走 run，仅取执行 metadata，不能用直接 await 代替查询行。
3. `result.meta()?.and_then(|m| m.changes)` 缺失时不能当 CAS 成功。必须有明确命中证据。
4. 最终发布 batch 中的 Revision、head、Feed、稳定提交结果，必须由同一成功 guard 控制。批后才检查 changes 无法撤销已无条件写入的副作用。
5. 不把原生 `BEGIN → await → COMMIT` 交互模型直接搬成独立 D1 调用；用平台支持的原子 batch，并在实际 D1 验证冲突和回滚。
6. R2 与 D1 不存在共同事务。对象先就绪，元数据最终发布，随后才能返回 committed。

**事实。** D1 返回大整数经过 JavaScript 数字边界；Rust D1Type::Integer 是 i32，不是通用 i64。[Rust D1Type](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/d1/mod.rs)、[大整数说明](https://developers.cloudflare.com/d1/worker-api/return-object/#d1execresult)

**建议。** revision/sequence 不能 `as i32` 截断，也不能任意 i64 转 f64。实现时明确合法数值边界、检查精度；需要 SQL CAST 为文本的适配方式时保持公开类型契约。此笔记不新增协议字段。

### 平台预算

**事实。** D1 Free 每 Worker 调用最多 50 查询，Paid 为 1000；每条最多 100 绑定参数、100KB SQL、2MB 行/string/BLOB，批内每条仍受这些约束。[D1 limits](https://developers.cloudflare.com/d1/platform/limits/)

**建议。** 500/10000 条记忆逐条 INSERT 的直接实现可能超预算。必须计算真实语句数、绑定数、解析开销和数据容量。较大 prepared 资料的 staging 与 final publication 可以分开，但不能让不完整版本成为可见 head；需额外故障与恢复验证。免费额度不是 RFC 承诺的无条件部署保证。

## 3. R2 create-only 与真实 SHA-256

**事实。** Bucket::put 接受 Into<Data>；builder 有 only_if、sha256，execute 返回 Result<Option<Object>>。条件失败为 None，不是成功空对象。[R2 builder](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/r2/builder.rs)、[Bucket 0.8.7](https://docs.rs/worker/0.8.7/worker/struct.Bucket.html)

**研究用法：**

```rust
let stored = bucket
    .put(object_key, body_bytes)
    .only_if(worker::Conditional {
        etag_does_not_match: Some("*".to_owned()),
        ..Default::default()
    })
    .sha256(expected_digest_bytes) // 原始 32 字节，不是 64 字节 hex 文本
    .execute()
    .await?;
```

“*”是创建前不存在条件的通配语义，需在实际 R2 契约测试核对映射与并发行为。[R2 conditional operations](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/#conditional-operations)、[RFC 9110 If-None-Match](https://www.rfc-editor.org/rfc/rfc9110.html#name-if-none-match)

**事实。** PUT 可提供一种校验算法；SHA-256 输入用真实内容检查，写入时提供的 checksum 可从对象 checksums 读取。Rust `Object::checksum()` 返回 R2Checksums，其 `sha256` 是 `pub Option<Vec<u8>>`。[R2 checksum 语义](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/#checksums)、[Object::checksum](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/r2/mod.rs)、[R2Checksums 字段](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker-sys/src/types/r2/checksums.rs)

```rust
let actual_digest: Option<Vec<u8>> = object.checksum().sha256;
```

**实施建议。**

- 先限制并计算实际 size/digest，R2 PUT 再携带 digest 验证；封存根据真实对象 checksum/size，不能采信 customMetadata 中的 sha256。
- create-only 返回 None 时读已有对象，核对真实内容/摘要。只有相同内容且 Upload 仍可写，才按既有契约返回 already_uploaded；封存后始终 UPLOAD_SEALED。
- 对象缺失 SHA-256 时不能用 ETag 代替。使用有界 GET 重算或拒绝封存并报告不可验证。
- 迟到 PUT、重试和清理不得覆盖/删除已封存或已发布引用。
- checksum 不证明授权、业务提交或数据库版本成功。

## 4. ByteStream、ReadableStream 与上传边界

**事实。** Request::stream(&mut self) 返回 Result<ByteStream>，只能消费一次；ByteStream 实现 Stream<Item=Result<Vec<u8>>> 且 !Send/!Sync。当前实现将字符串 `Error: aborted` 映射为流结束。[Request 源码](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/request.rs)、[ByteStream 0.8.7](https://docs.rs/worker/0.8.7/worker/struct.ByteStream.html)、[Stream 源码](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/streams.rs)

**事实。** R2 Data 可由 Vec<u8>、web_sys::ReadableStream 或 FixedLengthStream 构造；ByteStream 本身没有直接 Into<Data> 实现。FixedLengthStream::wrap(stream,length) 校验最终长度。[R2 Data](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/r2/mod.rs)、[FixedLengthStream](https://github.com/cloudflare/workers-rs/blob/b171cb4bdd6fa943bb81f429df0a6b0df37e14b3/worker/src/streams.rs)

```rust
let input = req.stream()?;
let fixed = worker::FixedLengthStream::wrap(input, expected_size);
let stored = bucket.put(object_key, fixed)
    .sha256(expected_digest_bytes)
    .only_if(create_only_condition)
    .execute().await?;
```

**建议。** 首版大小上限有限，可以有界逐块读取、超限即拒绝、增量计算 size/digest，再以 Vec 写入，优先直观实现。streaming 路径需真实验证取消、短读、长度与摘要失败，不因 EOF 就认定上传完整。Content-Length 是额外检查，不能取代实际字节上限。

零字节且 Request 无 body 时 stream() 可能报“no body”，必须按合法空资料路径处理并校验空摘要。不能先 bytes()/json() 再读取 stream。直传 req.inner().body() 可得到 ReadableStream，但平台 checksum 以外的 size/权限检查仍不能省略。8MiB 上传的拷贝与摘要 CPU 开销要测量，不能保证一定在免费 CPU 预算内。

## 5. MCP 2025-11-25 的最小无会话工具面

**事实。** initialize 必须先协商版本与 capabilities；成功后客户端发送 notifications/initialized。无 SSE 可以返回普通 JSON，GET /mcp 返回 405；不分配 MCP-Session-Id 可以无服务会话。[Lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)、[Transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)

首版可采用如下 **实施轮廓**：

| 输入 | 输出 |
|---|---|
| initialize JSON-RPC request | 匹配 id，版本 2025-11-25，serverInfo 与 tools capability |
| notifications/initialized | 已接受时 HTTP 202，空 body |
| ping request | 相同 id，result={} |
| tools/list | 固定有权限的工具目录与有效 inputSchema |
| tools/call | 相同 id，真实领域结果；正文 content 与 structuredContent |
| GET /mcp | 405，不宣称 SSE |
| DELETE /mcp | 405，不宣称会话销毁 |

**传输要求。** 每个 POST 是单个 UTF-8 JSON-RPC 消息，数组 batch 不属于此传输。客户端 Accept 同时含 application/json 与 text/event-stream；请求响应可选 application/json。接受的 notification/response 必须 202 空 body。后续 MCP-Protocol-Version 验证；缺失且无法识别时规范 SHOULD 回退 2025-03-26，不能静默认最新。无效/不支持版本返回400。[Transports：发送与版本头](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)

**Tools 语义。** tools capability 必须声明，未提供主动变更通知则不宣称 listChanged。2025-11-25 structuredContent 是对象；附相同 JSON 的 TextContent 便于旧客户端。业务冲突/参数值错误等返回 isError:true，未知工具/格式不合法用 JSON-RPC error；不能把拒绝内容藏在成功结果里。[Tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)

**建议。** 生命周期响应正确，但不把初始化或 session 当授权。无会话端每请求重新校验凭据与版本，持久 Consumer/幂等/游标完全独立于连接。不支持 sampling、prompts、resources、tasks、subscriptions 时不声明能力。取消通知也不能代替回滚已 committed 的业务。

当前 rmcp 服务可配置 legacy_session_mode(false) 与 json_response(true)；该配置名称与新版 discover 行为以锁定版本为准。原生可用其客户端验证上述工具面，Worker 不必携带完整 Tokio server。[rmcp stateless 文档](https://github.com/modelcontextprotocol/rust-sdk#stateless-streamable-http)

## 6. Origin 与授权

**事实。** Streamable HTTP 要校验 Origin；存在且非法时必须403。本地建议只绑定127.0.0.1，连接建议有认证。MCP OAuth能力可选，HTTP实现支持该授权能力时应遵循 OAuth/Protected Resource Metadata 契约；无效/过期 token 为401，权限不足为403。[Transport security](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports#security-warning)、[Authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)

**实施建议。**

- 对 /mcp 所有方法校验 Origin，包括 notifications、GET/DELETE；无 Origin 的非浏览器客户端可正常认证，不能用缺失 Origin 放弃鉴权。
- 使用精确允许的 scheme+host+port，拒绝任意“localhost”子串、null 与未经配置的站点。若使用浏览器跨源，CORS 与 Origin 允许表一致。
- 实例受限 Bearer 符合当前 Waypost 私有部署范围，但不称为完整 MCP OAuth；未实现元数据/授权服务器时不虚构 OAuth端点。
- Bearer 不能来自 query 参数、JSON producer 字段或 MCP session ID；每次检查到期、撤销、Actor/Consumer/grants。
- Cookie 管理会话依然属于独立控制面。普通 Agent 凭据不能进入 Dashboard；Dashboard 查看不 ACK。
- 外部试用 HTTPS 入口保持认证，不能把本地明文开发配置直接公开。

## 7. 实现验收清单

| 边界 | 关键反例 |
|---|---|
| D1原子性 | CAS0行、约束失败整批回滚、成功丢响应恢复、无变化与并发head前进 |
| R2不可变 | 新建成功、同key条件失败、同/异内容重试、摘要不符、封存后迟到PUT |
| 上传 | 无body空对象、短读、中止、超限、读两次、真实checksum缺失 |
| MCP | initialize→initialized→list→call；202空body；GET405；未知/缺失version策略 |
| 安全 | 无效Origin、缺凭据、撤销后重试、普通token调用管理面、跨Space资料 |
| 跨入口 | HTTP提交后MCP同键恢复，MCP提交后HTTP精确ID回读，同一真实结果 |
| 构建 | 原生tests与Wasm target检查分开，真实Worker存储契约不可由SQLite替代 |

源码读取和签名核对不证明实际运行兼容。正式宣称 Cloudflare 可用之前必须完成 Wasm build、实际绑定调用、远端 D1/R2 故障集、MCP客户端握手和部署资源预算；未执行项应明确记录。
