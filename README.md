# Waypost

**你的助手信箱。**

多个助手各自做事，做完把结果投进同一个信箱。Muse 收取、整理，再把结论发给你。只有一个助手时，它直接发给你。

Instinct 订好周五晚的两人位，Grok Bot 定期查机票，Today 整理行程笔记。三份结果经过 Waypost，你收到的是这样一封信：

```text
Instinct · Grok Bot · Today  →  Waypost 信箱  →  Muse  →  你
```

> Burnt Ends 11 月 6 日 19:00 两人位已确认，可改到周六。去程当天 09:40 起飞，S$540；回程三个候选，最便宜的 11 月 13 日 S$598，预算 S$600。请在周三前定回程。

Waypost 负责收信、留存和授权。订餐厅、查机票这些工作，由助手的运行环境完成；接入需要环境支持 HTTP MCP 或 HTTP API。

[网站](https://waypost.hika.fyi) · [使用场景](https://waypost.hika.fyi/use-cases/) · [使用指南](https://waypost.hika.fyi/guide/) · [数据与隐私](https://waypost.hika.fyi/privacy/)

## 收到第一封信

托管服务给你 Dashboard 地址和登录凭据。自己运行时，按文末步骤启动，打开本机页面。

1. **接入助手。** 登录 Dashboard，点「接入助手」，给它取个名字。
2. **保存连接。** 把生成的 MCP 地址和连接凭据放进助手的私密设置。原始凭据只显示一次，当场存好。[Waypost Skill](skills/waypost/SKILL.md) 可以帮它理解用法。
3. **请它发信。** 先查身份和权限，再发一条通知：

> 通过 Waypost 给默认收件人发一条消息，主题是「我已接入」，正文是「以后可以把进展和结果发到这里」。

回到信箱，第一封信已经在那里。登录凭据管理信箱，助手用的是另外签发的连接凭据；新连接默认只允许它向你发信。

## 交给 Muse 汇总

Muse 做主助手，Today、Instinct、Grok Bot 把结果交给它。

1. **让 Muse 能收信。** 在「助手 → 连接与权限」中允许它收取、确认收取和处理，保留发信能力。
2. **让其他助手能交付。** 签发允许向 Muse 发信的连接，保留各自的发信能力。
3. **发信时指明收件人。** 用 Muse 的 Actor ID 指定收件人。默认收件人仍是你，不会因为新增授权而改变。

Muse 收取后汇总，再把报告发给你。什么时候执行、什么时候收信，由它的运行环境安排。

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

每位用户有自己的空间，助手按授权访问。新版支持内容加密保存和发布前的文字隐私检查。

服务端持有解密密钥，能读取内容。用我们的 Cloudflare 托管服务时，密钥由我们维护；自己运行时，由你管理。

旧服务需要升级和迁移，才能启用新版保护，已有连接保留原权限。范围与密钥备份见[安全说明](docs/security.md)。

[开发者与 RFC](https://waypost.hika.fyi/rfc/) · [验证记录](docs/validation.md) · [Apache 2.0](LICENSE)