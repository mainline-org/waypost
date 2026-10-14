# Waypost 介绍网站

静态 HTML/CSS/JavaScript 网站，独立发布到 Vercel，不上传 Rust 服务、运行配置或私密资料。

- `/`：「你的助手信箱」介绍主页，以及结果投递、资料交接、Muse 汇总三个可切换示例。
- `/guide/`：用户使用指南。
- `/use-cases/`：五个配置示例，展示餐厅预约、机票查价、Muse 汇总、资料版本与恢复。
- `/privacy/`：数据与隐私保护范围。
- `/rfc/`：三个 RFC、性质计划和配套实现文档的完整网页，保留原文下载与章节目录。

本机预览：`python3 -m http.server 8794 --directory website`。

发布：`vercel --prod --yes --cwd website`。自定义域名是 `waypost.hika.fyi`，具体 DNS 值应以 Vercel 域名配置返回结果为准。

`.vercel/` 仅保存本机链接信息并被 Git 忽略。发布目录不包含凭据，页面不登录或调用 Waypost API。安全与用户隔离的文案明确标注新版和实例配置边界。

## 中英文（i18n）

单 URL、词典式前端国际化，无构建依赖、无第三方请求：

- 文案唯一来源是 `assets/locales/en.json`（英文）与 `assets/locales/zh.json`（中文），键必须一一对应。
- 页面 HTML 直接写英文（无 JS、未知语言、词典加载失败时都回退英文），中文由 `assets/i18n.js` 在运行时套用。
- 语言判定优先级：`?lang=zh|en` 分享参数 ＞ `localStorage`（`waypost-lang`，仅显式切换时写入）＞ 浏览器首选语言（`navigator.languages[0]`，`zh*` 进中文）＞ 默认英文。
- 页眉右上角有 `中文 / EN` 切换器（`data-lang` 按钮），切换即时生效并记忆；`assets/site.js` 的首页示例与复制按钮通过 `waypost:lang` 事件跟随重渲染（示例文案同时存于 `site.js` 的 `SCENARIOS`，与 `demo.*` 键保持同步）。
- 可翻译节点：`data-i18n`（纯文本）、`data-i18n-html`（受信任的本地富文本）、`data-i18n-aria`（`aria-label`）；`<html data-page="…">` 对应 `meta.<page>.title/desc`，切换时同步更新 `document.title`、`description` 与 `og:locale`。
- 使用场景详情页正文渲染自 `docs/use-cases/<slug>.md`（中文）与 `<slug>.en.md`（英文）；构建脚本把渲染结果回写为 `case.<slug>.bodyHtml`，改 Markdown 后必须重跑构建。
- RFC 文档正文暂仅中文（规范原文），外壳（导航、提示、索引卡片）已双语，正文上方有英文提示说明翻译进行中。

改文案后运行（需 `requirements-docs.txt` 依赖）：`python3 website/build_use_cases.py`（如改了用例 Markdown），`python3 website/build_docs.py`（如改了 RFC 源文档），最后必跑 `python3 website/check.py`。`check.py` 除原有引用与标题检查外，还校验：中英键 parity、`en.json` 无 CJK、所有 `data-i18n*` 引用存在、行内英文与 `en.json` 一致、用例正文与 Markdown 源一致、示例文案与 `site.js` 同步。

Dashboard（应用）是另一套独立的同构方案，见 `dashboard/i18n.js` 头注释，改文案后跑 `python3 dashboard/check.py`；新增静态资源需同步到 `src/service/dashboard.rs` 与 `src/http.rs` 的路由表（`tests/dashboard.rs` 有覆盖）。

## 本次发布

- Vercel 项目：`waypost`。
- 正式地址：<https://waypost.hika.fyi>；备用地址：<https://waypost-wheat.vercel.app>。
- 域名 `waypost.hika.fyi` 已配置 CNAME，Vercel 所有权验证通过，TLS 与正式 Host 请求返回 200。
- Vercel 推荐 CNAME：`waypost` → `140761196a2525c6.vercel-dns-017.com`，仅 DNS。
- TXT 验证名称为 `_vercel`；值应从 Vercel 当前项目域名验证结果获取，保留已有 TXT 后另行添加。

线上已检查三页、资源 MIME 类型和 CSP 等响应头，以及 `.env.local`、`.vercel/project.json` 和 `README.md` 不对外提供。当前 Browser 未连接，未进行浏览器视口与交互视觉检查。

## 更新 RFC 页面

安装 `requirements-docs.txt` 的构建依赖后，运行 `python3 website/build_docs.py`，再运行 `python3 website/check.py`。生成的 HTML 与公开 Markdown 原文一同保存在仓库中，因此 Vercel 发布仍不需要依赖安装或服务端执行。渲染只读取显式列出的公开文档，关闭 Markdown 原始 HTML，图示保留 Mermaid 源码。

更新使用场景：安装文档构建依赖后运行 `python3 website/build_use_cases.py`，再运行 `python3 website/check.py`。案例原文保存在 `docs/use-cases/`，仅展示配置方式，不宣称第三方专用集成或真实订单验证。
