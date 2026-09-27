// 版本更新检测：请求 GitHub Releases API 获取最新版本与更新说明。
// 若网络失败 / 仓库暂无 Release → 返回 null（静默处理，不打扰用户）。
export type UpdateInfo = {
  hasUpdate: boolean;
  currentVersion: string;
  latestVersion: string;
  changelog: string;
  releaseUrl: string;
};

const REPO = 'lsx666xsl/Tauri-Rust-SerialTool';
const RELEASE_API = `https://api.github.com/repos/${REPO}/releases/latest`;

// 语义化版本比较：返回 >0 表示 a 比 b 新（忽略 v 前缀，逐段数字比较）
export const compareVersion = (a: string, b: string): number => {
  const seg = (v: string) => v.replace(/^v/i, '').split('.').map((n) => parseInt(n, 10) || 0);
  const pa = seg(a);
  const pb = seg(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
};

// 获取当前应用版本（浏览器调试环境无 Tauri API 时回退 0.0.0，让更新按钮可被看到）
const getCurrentVersion = async (): Promise<string> => {
  try {
    const { getVersion } = await import('@tauri-apps/api/app');
    return await getVersion();
  } catch {
    return '0.0.0';
  }
};

export const checkUpdate = async (): Promise<UpdateInfo | null> => {
  try {
    const res = await fetch(RELEASE_API, { headers: { Accept: 'application/vnd.github+json' } });
    if (!res.ok) return null;
    const data: { tag_name?: string; body?: string; html_url?: string } = await res.json();
    const latest = String(data.tag_name ?? '');
    if (!latest) return null;

    const current = await getCurrentVersion();
    return {
      hasUpdate: compareVersion(latest, current) > 0,
      currentVersion: current,
      latestVersion: latest.replace(/^v/i, ''),
      changelog: String(data.body ?? '').trim() || '（本次发布未填写更新说明）',
      releaseUrl: String(data.html_url ?? `https://github.com/${REPO}/releases`),
    };
  } catch {
    return null;
  }
};

// 打开更新下载页（优先 Tauri opener 插件，浏览器调试环境回退 window.open）
export const openReleasePage = async (url: string): Promise<void> => {
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch {
    window.open(url, '_blank');
  }
};
