// 版本更新检测：请求 GitHub Releases API 获取最新版本与更新说明。
// 若网络失败 / 仓库暂无 Release → 返回 null（静默处理，不打扰用户）。
export type UpdateInfo = {
  hasUpdate: boolean;
  currentVersion: string;
  latestVersion: string;
  changelog: string;
  releaseUrl: string;
};

const REPO = 'lsx666xsl/NNSerialTool';
const RELEASES_API = `https://api.github.com/repos/${REPO}/releases`;
const RELEASE_API = `${RELEASES_API}/latest`;

// 分版本更新历史：一个 Release 对应一条（tag 去掉 v 前缀作为版本号）
export type VersionHistory = {
  version: string;
  date: string; // YYYY-MM-DD
  notes: string;
};

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

// ---------- 分版本更新历史 ----------
// 拉取 Releases 列表，筛出比当前版本新的所有版本（升序：从当前版本的下一个一路读到最新）。
// 每个版本的更新说明取 Release body；网络失败返回空数组，由调用方回退到最新版说明。
export const fetchVersionHistory = async (currentVersion: string): Promise<VersionHistory[]> => {
  try {
    const res = await fetch(`${RELEASES_API}?per_page=30`, { headers: { Accept: 'application/vnd.github+json' } });
    if (!res.ok) return [];
    const data: Array<{ tag_name?: string; body?: string; published_at?: string }> = await res.json();
    if (!Array.isArray(data)) return [];
    return data
      .map((r) => ({
        version: String(r.tag_name ?? '').replace(/^v/i, ''),
        date: String(r.published_at ?? '').slice(0, 10),
        notes: String(r.body ?? '').trim(),
      }))
      .filter((r) => r.version && compareVersion(r.version, currentVersion) > 0)
      .sort((a, b) => compareVersion(a.version, b.version));
  } catch {
    return [];
  }
};

// ---------- 更新说明归类（新功能 / 优化 / 修复 / 文档） ----------
export type ReleaseGroupKind = 'feature' | 'improve' | 'fix' | 'docs' | 'other';
export type ReleaseGroup = { kind: ReleaseGroupKind; label: string; items: string[] };

const GROUP_DEFS: Array<{ kind: ReleaseGroupKind; label: string; keys: string[] }> = [
  { kind: 'feature', label: '新功能', keys: ['新功能', '新增', '添加', '增加', 'feat', 'feature'] },
  { kind: 'improve', label: '优化', keys: ['优化', '改进', '改善', '提升', '调整', '重构', 'improve', 'optimize', 'perf', 'refactor'] },
  { kind: 'fix', label: '修复', keys: ['修复', '修正', 'fix', 'bug'] },
  { kind: 'docs', label: '文档', keys: ['文档', 'docs', 'readme'] },
];
const OTHER_GROUP: ReleaseGroup = { kind: 'other', label: '其他变更', items: [] };

type GroupHit = { kind: ReleaseGroupKind; key: string };

// 取行内最早出现的关键词作为归属（英文不区分大小写）；各分类都未命中返回 null
const detectGroup = (line: string): GroupHit | null => {
  const lower = line.toLowerCase();
  let best: (GroupHit & { idx: number }) | null = null;
  for (const def of GROUP_DEFS) {
    for (const key of def.keys) {
      const idx = lower.indexOf(key.toLowerCase());
      if (idx >= 0 && (!best || idx < best.idx)) best = { kind: def.kind, key, idx };
    }
  }
  return best ? { kind: best.kind, key: best.key } : null;
};

// 短行只由关键词+标点构成（如 "### 修复"、"【新功能】"）：视为分类标题，只切换归属不产生条目
const isCategoryHeading = (line: string, hit: GroupHit): boolean => {
  if (line.length > 12) return false;
  const rest = line
    .toLowerCase()
    .replace(hit.key.toLowerCase(), '')
    .replace(/[：:，,、。；;（）()\[\]【】\s-]/g, '');
  return rest.length === 0;
};

// 行首是命中关键词、且其后紧跟分隔符/空白时，剥掉关键词与分隔符（分类徽标已表达该语义）；
// 关键词后直接连文字（如"修复了xxx"）则保留原文，避免剥出"了xxx"这类残句
const stripKeyword = (line: string, hit: GroupHit): string => {
  if (!line.toLowerCase().startsWith(hit.key.toLowerCase())) return line;
  const rest = line.slice(hit.key.length);
  if (rest && !/^[：:，,、\s]/.test(rest)) return line;
  return rest.replace(/^[：:，,、\s]+/, '') || line;
};

// 把一段更新说明解析为分类小节：逐行按最早关键词归类，标题行切换后续行的归属；
// 一条都没归类上（自由文本）返回空数组，调用方回退为原文显示。
export const groupReleaseNotes = (notes: string): ReleaseGroup[] => {
  const buckets = new Map<ReleaseGroupKind, string[]>();
  const push = (kind: ReleaseGroupKind, item: string) => {
    const list = buckets.get(kind);
    if (list) list.push(item);
    else buckets.set(kind, [item]);
  };

  let current: ReleaseGroupKind | null = null;
  for (const rawLine of notes.split('\n')) {
    // 去掉 markdown 标题记号与列表符号后归类
    const line = rawLine.replace(/^\s*[#>*•·\-]+\s*/, '').trim();
    if (!line) continue;
    const hit = detectGroup(line);
    if (hit) {
      current = hit.kind;
      if (!isCategoryHeading(line, hit)) push(hit.kind, stripKeyword(line, hit));
    } else if (current) {
      push(current, line);
    } else {
      push('other', line);
    }
  }

  const order: ReleaseGroupKind[] = ['feature', 'improve', 'fix', 'docs', 'other'];
  const groups = order
    .map((kind) => {
      const items = buckets.get(kind);
      const def = GROUP_DEFS.find((d) => d.kind === kind);
      return { kind, label: def?.label ?? OTHER_GROUP.label, items: items ?? [] };
    })
    .filter((g) => g.items.length > 0);
  // 只有"其他变更"没有任何归类命中 → 视为未结构化的自由文本，交回原文显示
  return groups.some((g) => g.kind !== 'other') ? groups : [];
};
