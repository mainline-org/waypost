'use strict';
window.WaypostOnboardingCopy = {
  zh: {
    primary: '主助手', connected: '接入助手', selectSpace: '空间尚未加载，请稍后再接入助手。', choosePrimary: '选为主助手', primaryIntro: '主助手是你主要对话的助手，收取其他助手的结果并向你汇报。选择角色不自动扩大权限。',
    recipientLabel: '默认把结果送给', ownerRecipient: '你', configurePrimary: '配置收信连接', inviteTitle: '邀请使用', requestTitle: '申请使用', requestHelp: '留下邮箱，我们审核后通过私密渠道提供邀请。', requestAccepted: '申请已收到。请等待邀请，不要重复提交。',
    inviteName: '个人空间名称', inviteCreate: '创建空间并生成邀请', invitePrivate: '邀请链接有效期为 24 小时，只能领取一次。通过私密渠道交给接收者。', inviteCredential: '个人登录凭据 · 请当场保存', inviteJoined: '邀请已领取。下次使用这份个人登录凭据进入你的空间。',
    accessTitle: '邀请请求', approve: '生成邀请', issued: '已准备邀请', email: '邮箱', grantInfo: '选择主助手后，再明确签发收信和回执能力。', confirmPrimary: '确认选择', requestButton: '申请邀请', revokeInvite: '撤销这份邀请', inviteRevoked: '邀请已撤销，链接无法再领取。'
  },
  en: {
    primary: 'Primary assistant', connected: 'Connected assistant', selectSpace: 'Your space is still loading. Please wait before connecting an assistant.', choosePrimary: 'Choose as primary', primaryIntro: 'Your primary assistant receives other assistants’ results and reports to you. Choosing this role does not grant permissions.',
    recipientLabel: 'Default recipient', ownerRecipient: 'You', configurePrimary: 'Configure receiving access', inviteTitle: 'Invite a user', requestTitle: 'Request access', requestHelp: 'Leave your email. We review requests and share private invitations.', requestAccepted: 'Request received. Please wait for your invitation.',
    inviteName: 'Personal space name', inviteCreate: 'Create space and invitation', invitePrivate: 'This private invitation expires in 24 hours and can be claimed once.', inviteCredential: 'Personal login credential · save now', inviteJoined: 'Invitation claimed. Use this credential to enter your personal space next time.',
    accessTitle: 'Access requests', approve: 'Create invitation', issued: 'Invitation prepared', email: 'Email', grantInfo: 'Choose a primary assistant, then explicitly grant receiving and acknowledgement capabilities.', confirmPrimary: 'Confirm choice', requestButton: 'Request an invitation', revokeInvite: 'Revoke this invitation', inviteRevoked: 'Invitation revoked. This link can no longer be claimed.'
  }
};
window.waypostOnboardingText = key => {
  const language=document.documentElement.lang.startsWith('en')?'en':'zh';
  return window.WaypostOnboardingCopy[language][key] || key;
};
