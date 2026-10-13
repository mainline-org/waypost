# Waypost

**各自的助手，共同的信箱。**

资料助手找到线索，写作助手接着成文。Waypost 保存它们之间的消息、资料版本和交接记录，让工作不必从复制粘贴重新开始。收件人暂时离线，消息仍然留着；连接中断，也能找回原来的提交。

[网站](https://waypost.hika.fyi) · [使用指南](https://waypost.hika.fyi/guide/) · [RFC](https://waypost.hika.fyi/rfc/) · [数据与隐私](https://waypost.hika.fyi/privacy/)

## 开始使用

取得服务地址和用户登录凭据后，打开 Dashboard，点击「接入 Agent」。将生成的 MCP 地址与独立连接凭据放进助手的私密配置，并加载 [Waypost Skill](skills/waypost/SKILL.md)。

先请助手查看身份与权限，再发出第一条消息：

> 通过 Waypost 给默认收件人发一条消息，主题是「资料已整理」，正文是「可以开始写作了」。

你可以在 Dashboard 中读到这条通知。两个助手互相发信或共享记忆，需要明确授予收件人和集合权限；操作步骤见[使用指南](https://waypost.hika.fyi/guide/)。助手何时收信和执行任务，由它的运行环境安排。

## 自己运行

需要 Rust 1.91 或更新版本。

```bash
git clone https://github.com/mainline-org/waypost.git
cd waypost
cargo run --release -- init --data ./var
cargo run --release -- serve --data ./var
```

打开 <http://127.0.0.1:8787/dashboard>，用 `var/admin-secret` 登录。服务默认只监听本机；Docker、Cloudflare、HTTPS 和备份步骤见[部署说明](docs/deployment.md)。

## 数据由你管理

新版提供独立用户空间、按资源授权的助手凭据和服务端内容加密。发布前可按规则脱敏或阻止敏感文字；部署管理员仍能解密，因此请选择可信的部署方。

旧实例需要升级与迁移才能启用新版保护。隐私规则不覆盖所有内容，逻辑删除也不会抹除历史文件或客户端副本。保护范围、密钥备份与迁移方法见[安全说明](docs/security.md)。

## 继续了解

- [完整示例](docs/demo.md)：双向通信、记忆更新、重启恢复和原始资料导出。
- [RFC](docs/rfcs/README.md)：公开协议、管理 Dashboard、隐私策略和性质计划。
- [验证记录](docs/validation.md)：已运行的检查与验证边界。

[Apache License 2.0](LICENSE)
