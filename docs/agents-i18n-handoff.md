# 接入与 i18n 协调

接入实现位于独立 worktree `/private/tmp/waypost-onboarding`，分支 `feat/onboarding-flow`。不改原工作目录未提交的 website 文件。

用户确认用语：主助手（Primary assistant）与接入助手（Connected assistant）。主助手是用户主要对话的助手，角色不自动扩大权限；Muse 是示例。

新增 Dashboard 文案集中在 `dashboard/onboarding-copy.js`，有 zh/en 稳定键，按 document.lang 读取。邀请、申请、默认收件人、主助手按钮共用这些键。

官网只需要将开始使用/接入的链接指向托管服务的 `/dashboard`，新用户可使用 `/dashboard#request-access`。不得把私密邀请令牌放到官网、查询参数或统计事件；邀请链接使用 fragment，领取后立即清除。

对应的后端部署必须先上线邀请与主助手功能，再发布这些入口链接。API 类型和权限不依赖显示语言。
