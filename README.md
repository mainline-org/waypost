# Waypost

**你的助手信箱。**

助手的进展和结果，都送到你这里。无论它在哪里运行，你都能在同一个信箱收到报告；需要继续工作时，再共享资料或交给另一位助手。

[网站](https://waypost.hika.fyi) · [使用指南](https://waypost.hika.fyi/guide/) · [数据与隐私](https://waypost.hika.fyi/privacy/) · [开发者与 RFC](https://waypost.hika.fyi/rfc/)

## 收到第一封信

取得服务地址和用户登录凭据，打开 Dashboard，点击「接入助手」。为它取个名字，把生成的 MCP 连接信息保存到宿主的私密配置，并加载 [Waypost Skill](skills/waypost/SKILL.md)。

新版默认连接只允许向你发信。请助手查看身份和权限，再发送一条通知：

> 通过 Waypost 给默认收件人发一条消息，主题是「我已接入」，正文是「以后可以把进展和结果发到这里」。

回到信箱，确认第一封信已经收到。助手继续在原来的环境中运行，执行任务和定时收信由宿主安排。

## 继续工作

消息、附件、助手之间的通信、记忆集合与历史版本、独立回执、断线恢复和资料导出都继续提供。需要收信、上传或读写资料时，在助手的「连接与权限」中签发相应连接；共享给其他助手时，明确授予收件人和集合权限。步骤见[使用指南](docs/user-guide.md)。

## 自己运行

需要 Rust 1.91 或更新版本。

```bash
git clone https://github.com/mainline-org/waypost.git
cd waypost
cargo run --release -- init --data ./var
cargo run --release -- serve --data ./var
```

打开 <http://127.0.0.1:8787/dashboard>，用 `var/admin-secret` 登录。服务默认只监听本机；Docker、Cloudflare、HTTPS 和备份见[部署说明](docs/deployment.md)。

## 数据由你管理

用户使用独立空间，助手按资源授权访问。新版支持服务端内容加密、文字隐私过滤和连接撤销；部署方仍能解密，请选择可信的服务。

旧实例需要升级与迁移才能启用新版保护和接入流程，已有连接保留原权限。隐私规则不覆盖所有内容，逻辑删除不会抹除历史文件或客户端副本。详见[安全说明](docs/security.md)。

[完整示例](docs/demo.md) · [RFC](docs/rfcs/README.md) · [验证记录](docs/validation.md) · [Apache 2.0](LICENSE)
