# Waypost

**你的助手信箱。**

多个助手各自做事，做完把结果交给 Waypost。你选一位**主助手**，让它收取、整理，再向你汇报。只有一个助手时，它直接发给你。

Instinct 订好周五晚的两人位，Grok Bot 定期查机票，Today 整理行程笔记。三份结果经过 Waypost，你收到的是这样一封信：

```text
Instinct · Grok Bot · Today  →  Waypost  →  主助手  →  你
```

> Burnt Ends 11 月 6 日 19:00 两人位已确认，可改到周六。去程当天 09:40 起飞，S$540；回程三个候选，最便宜的 11 月 13 日 S$598，预算 S$600。请在周三前定回程。

Waypost 负责投递、留存和授权。订餐厅、查机票由助手完成；接入需要它的运行环境支持 HTTP MCP 或 HTTP API。以上是使用示例。

[网站](https://waypost.hika.fyi) · [使用场景](https://waypost.hika.fyi/use-cases/) · [使用指南](https://waypost.hika.fyi/guide/) · [数据与隐私](https://waypost.hika.fyi/privacy/)

## 收到第一封信

使用我们的 Cloudflare 托管服务，先[申请邀请](https://waypost-production.christopherwuy.workers.dev/dashboard#request-access)。收到私密链接后，打开它，保存自己的登录凭据。自己运行时，按文末步骤启动。

1. **接入助手。** 登录 Dashboard，点「接入助手」，给它取个名字。
2. **保存连接。** 把生成的 MCP 地址和连接凭据放进助手的私密设置。原始凭据只显示一次，当场存好。[Waypost Skill](skills/waypost/SKILL.md) 可以帮它理解用法。
3. **请它发信。** 先查身份和权限，再发一条通知：

> 通过 Waypost 给默认收件人发一条消息，主题是「我已接入」，正文是「以后可以把进展和结果发到这里」。

回到信箱，确认收到第一封信。你的登录凭据管理自己的空间；助手使用单独的连接凭据，首次接入只有发信能力。

## 让主助手汇总

主助手是你主要对话的助手，接入助手把各自的结果交给它。现在正在使用的助手也可以担任主助手，Muse 是一个例子。

1. **选定主助手。** 在「助手」页选择「选为主助手」，按页面提示签发收信、回执和发信权限。
2. **接入其他助手。** 默认收件人选择主助手，保存各自的连接信息。它们仍只有发信能力。
3. **请主助手汇报。** 让它收取结果、整理需要你决定的事，再给你发一封报告。

选为主助手不会自动扩大权限。什么时候执行、什么时候收信，由它的运行环境安排。

签发新连接不撤销旧连接。改完配置，记得更新助手并撤销旧的。完整步骤见[使用指南](https://waypost.hika.fyi/guide/#main-assistant)。

## 还能做什么

- **附上原始文件。** 随消息交付完整资料。
- **留下当时的版本。** 一组资料持续更新，随时回到历史版本核对。
- **断线后接着用。** 每个客户端独立确认收到或处理，从保存的进度继续。

附件与资料读写按需授权，连接可以设置有效期或随时撤销。见[完整示例](docs/demo.md)。

## 自己运行

需要 Rust 1.91 或更新版本。

```bash
git clone https://github.com/mainline-org/waypost.git
cd waypost
cargo run --release -- init --data ./var
cargo run --release -- serve --data ./var
```

打开 <http://127.0.0.1:8787/dashboard>，用 `var/admin-secret` 里的登录凭据进入，先点「新建空间」，再接入助手。

服务默认只接受本机连接。给别人使用时，为每人建一个空间和一份凭据。公网、Docker、Cloudflare 与备份见[部署说明](docs/deployment.md)。

## 谁能读取你的数据

每位用户有自己的空间，助手按授权访问。托管服务将消息正文、附件与资料加密保存，并按你配置的规则在发布前检查文字。

服务端持有解密密钥，能读取内容。用我们的 Cloudflare 托管服务时，密钥由我们维护；自己运行时，由你管理。

自己运行旧版本，需要升级并迁移已有内容。保护范围与密钥备份见[安全说明](docs/security.md)。

[开发者与 RFC](https://waypost.hika.fyi/rfc/) · [验证记录](docs/validation.md) · [Apache 2.0](LICENSE)
