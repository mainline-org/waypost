# Waypost 介绍网站

静态 HTML/CSS/JavaScript 网站，独立发布到 Vercel，不上传 Rust 服务、运行配置或私密资料。

- `/`：「你的助手信箱」介绍主页，以及报告、资料、接续三个可切换示例。
- `/guide/`：用户使用指南。
- `/privacy/`：数据与隐私保护范围。
- `/rfc/`：三个 RFC、性质计划和配套实现文档的完整网页，保留原文下载与章节目录。

本机预览：`python3 -m http.server 8794 --directory website`。

发布：`vercel --prod --yes --cwd website`。自定义域名是 `waypost.hika.fyi`，具体 DNS 值应以 Vercel 域名配置返回结果为准。

`.vercel/` 仅保存本机链接信息并被 Git 忽略。发布目录不包含凭据，页面不登录或调用 Waypost API。安全与用户隔离的文案明确标注新版和实例配置边界。

## 本次发布

- Vercel 项目：`waypost`。
- 正式地址：<https://waypost.hika.fyi>；备用地址：<https://waypost-wheat.vercel.app>。
- 域名 `waypost.hika.fyi` 已配置 CNAME，Vercel 所有权验证通过，TLS 与正式 Host 请求返回 200。
- Vercel 推荐 CNAME：`waypost` → `140761196a2525c6.vercel-dns-017.com`，仅 DNS。
- TXT 验证名称为 `_vercel`；值应从 Vercel 当前项目域名验证结果获取，保留已有 TXT 后另行添加。

线上已检查三页、资源 MIME 类型和 CSP 等响应头，以及 `.env.local`、`.vercel/project.json` 和 `README.md` 不对外提供。当前 Browser 未连接，未进行浏览器视口与交互视觉检查。

## 更新 RFC 页面

安装 `requirements-docs.txt` 的构建依赖后，运行 `python3 website/build_docs.py`，再运行 `python3 website/check.py`。生成的 HTML 与公开 Markdown 原文一同保存在仓库中，因此 Vercel 发布仍不需要依赖安装或服务端执行。渲染只读取显式列出的公开文档，关闭 Markdown 原始 HTML，图示保留 Mermaid 源码。
