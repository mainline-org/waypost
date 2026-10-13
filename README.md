# Waypost

**你的助手信箱。**

多个助手各自做事，结果交到同一个信箱。让主助手接着处理，再把整理好的进展和结果发给你。只有一个助手时，也可以直接向你汇报。

[看看网站](https://waypost.hika.fyi) · [使用场景](https://waypost.hika.fyi/use-cases/) · [使用指南](https://waypost.hika.fyi/guide/) · [数据与隐私](https://waypost.hika.fyi/privacy/)

比如，Instinct 订好周五晚的米其林一星餐厅，Grok Bot 定期查机票，Today 整理行程笔记。结果经过 Waypost，交给 **主助手 Muse** 汇总：餐厅已确认，机票有哪些候选，接下来需要你决定什么。

```text
Today / Instinct / Grok Bot → Waypost 信箱 → Muse → 你
```

这是使用示例。实际接入需要助手的运行环境支持 HTTP MCP，或通过 HTTP API 对接。

## 收到第一封信

使用我们提供的 Cloudflare 托管服务，获取你的 Dashboard 地址和登录凭据。自己运行时，按下方命令启动，打开本机页面。

1. **接入助手。** 登录 Dashboard，点击「接入助手」，为它取个名字。
2. **保存连接。** 将生成的 MCP 地址和连接凭据放进助手的私密设置中。可加载 [Waypost Skill](skills/waypost/SKILL.md)，帮助它理解用法。
3. **请它发信。** 先查看身份和权限，再发送一条通知：

> 通过 Waypost 给默认收件人发一条消息，主题是「我已接入」，正文是「以后可以把进展和结果发到这里」。

回到信箱，确认消息已经收到。新版默认连接只允许助手向你发信，更多能力之后按需开放。登录凭据用来管理信箱，助手使用单独生成的连接凭据。

## 交给 Muse 汇总

1. **Muse 负责收信。** 在「助手 → 连接与权限」中允许它收信、确认收取和处理，并保留发信能力。
2. **其他助手负责交付。** 给它们签发允许向 Muse 发信的连接，并在发信时明确指定 Muse 为收件人。
3. **Muse 接着处理。** 收取、汇总，再把报告发给你。执行与定时收信由 Muse 的运行环境安排。

签发新连接后，请更新助手的配置。增加收件人授权不会自动改投给 Muse；默认收件人仍是你。详细步骤见[使用指南](https://waypost.hika.fyi/guide/#main-assistant)。

## 还可以做什么

- **附上原始文件：** 随消息交付完整资料。
- **保留上下文：** 把资料归成一组，持续更新，随时回看历史版本。
- **断线后继续：** 恢复提交和收取进度，每个客户端独立确认收到或处理。

附件与资料读写按需授权，连接可以设置有效期或随时撤销。双向通信、资料导出和恢复流程见[完整示例](docs/demo.md)。

## 自己运行

需要 Rust 1.91 或更新版本。

```bash
git clone https://github.com/mainline-org/waypost.git
cd waypost
cargo run --release -- init --data ./var
cargo run --release -- serve --data ./var
```

打开 <http://127.0.0.1:8787/dashboard>，用 `var/admin-secret` 文件中的登录凭据进入，先点击「新建空间」，再接入助手。

服务默认只接受本机连接。让其他人使用时，为每人创建独立空间和登录凭据；公网、Docker、Cloudflare 和备份配置见[部署说明](docs/deployment.md)。

## 谁能访问你的数据

每位用户有自己的空间，助手按授权访问。新版支持内容加密保存，以及发布前的文字隐私检查。

服务端持有解密密钥，能够读取内容。使用我们的 Cloudflare 托管服务时，由我们维护服务和密钥；自己运行时，由你管理。

旧服务需要升级和迁移，才能启用新版保护与接入流程；已有连接保留原权限。保护范围与密钥备份见[安全说明](docs/security.md)。

[开发者与 RFC](https://waypost.hika.fyi/rfc/) · [验证记录](docs/validation.md) · [Apache 2.0](LICENSE)
