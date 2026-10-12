# ADR-0001：共享原子领域操作，封闭内部 SQL 驱动

状态：Accepted during implementation。用户已授权完成实现，无需逐步等待审查。修订 `docs/design/0001-rust-data-model.md` §6 的存储端口草图；公开 RFC 操作与性质不变。

## 问题

SQLite 与 D1 的事务运行机制不同，但身份、幂等、CAS、权限代次、隐私修订和 Feed 发布条件相同。若各适配器分别实现整套领域 MetadataStore，会复制同一组关键规则。最初使用公开 `Database::query/batch(Statement)`，虽然领域规则仍统一，但把原始 SQL 和未类型化行暴露成了 Rust 库的公共存储端口，背离已确认的领域边界。

## 决定

消息、记忆、ACK、Blob 等具名操作由同一个核心处理；HTTP 与 MCP 调用这些操作。SQL 语句、参数、行解码和执行 trait 收入 crate-private 的 `storage::driver`，仅供核心及已提供的 SQLite/D1 适配器使用。它是内部执行机制，不是客户端可调用的 Repository 或可动态注册的领域接口。

原生 `SqliteDb` 和 Worker D1 session 只负责执行查询与原子 batch；权限、最终发布 guard、稳定结果和 CAS 仍在共享核心中。数据库约束承担空间与目标完整性。ObjectStore 保留独立字节/不可变性契约，支持真实存储测试与准备期间故障插入。

## 理由与验证

这保留了简单的共享 SQL 实现，避免包装一层转发方法或复制 SQLite/D1 的领域实现。公开集成面是 RFC 的 HTTP/MCP 操作；原始 SQL 不是对外 API。客户端不能通过协议或 Rust 公共模块提交任意 SQL。现有实际 SQLite 行为、随机状态序列、准备期间策略竞争、官方 MCP SDK 和本机 D1/R2 完整 demo 验证同一组可观察语义。

此决定不主张全部内部状态已经拥有独立 Rust 值类型。数据库行解码仍是持久化实现细节；未来新增存储后端应保持这些原子性质，不能绕过共享领域规则。
