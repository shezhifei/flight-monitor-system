/**
 * 标签管理已并入派工规则页（左右分栏 · section=labels）。
 * 保留此入口做兼容跳转，避免旧书签/外链 404。
 */
import { pageUrl } from '@/shared/page-routes';
import { bootstrapProtectedPage, markWorkspaceEmbed } from '@/shared/bootstrapProtectedPage';

markWorkspaceEmbed();

await bootstrapProtectedPage(() => {
  const target = `${pageUrl('dispatch_rule_center')}?section=labels`;
  const params = new URLSearchParams(window.location.search);
  // 工作区 iframe 嵌入时带上 embed=1
  if (params.get('embed') === '1' || params.get('shell') === '1') {
    const url = new URL(target, window.location.origin);
    url.searchParams.set('embed', '1');
    window.location.replace(url.pathname + url.search);
    return;
  }
  window.location.replace(target);
});
