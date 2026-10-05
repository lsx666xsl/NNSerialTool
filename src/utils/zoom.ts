/*
 * 根节点 zoom 等比缩放下的 fixed 浮层坐标换算。
 *
 * 背景：App.vue 在根节点设置 zoom 做整体等比缩放；SelfSelect / SessionForm 的浮层
 * 用 Teleport 到 body + position:fixed 定位，坐标来自 getBoundingClientRect()
 * （视觉像素）。各引擎对"fixed 后代是否继承祖先 zoom"实现不同：
 *   - Chromium（Windows WebView2）：继承 → fixed 坐标被 zoom 放大，视觉像素需除以 zoom；
 *   - WebKitGTK（Linux）：标准化 zoom 不作用于 fixed 后代 → 直接用视觉像素，除以 zoom 反而错位。
 * 差异无法从 UA 区分（WebKitGTK 不自报家门），改为运行时实测：临时把根节点 zoom
 * 设为 2，放一个 top:100px 的隐藏 fixed 探针，量它的视觉位置——落在 200 说明
 * fixed 坐标被 zoom 放大（需补偿），落在 100 说明没有。结果缓存，引擎行为运行期不变。
 */
let zoomAffectsFixed: boolean | null = null;

function detectZoomAffectsFixed(): boolean {
  const html = document.documentElement;
  const prev = html.style.zoom;
  try {
    html.style.zoom = '2';
    const probe = document.createElement('div');
    probe.style.cssText = 'position:fixed;top:100px;left:0;width:0;height:0;visibility:hidden;pointer-events:none;';
    document.body.appendChild(probe);
    const top = probe.getBoundingClientRect().top;
    probe.remove();
    return Math.abs(top - 200) < Math.abs(top - 100);
  } catch {
    return true; // 探测失败时保守沿用 Chromium 行为（历史默认）
  } finally {
    html.style.zoom = prev;
  }
}

/**
 * 视觉像素 → fixed 坐标像素 的除数。
 * @param zoom 根节点当前 zoom 值（document.documentElement.style.zoom）
 */
export function fixedPxUnit(zoom: number): number {
  if (zoomAffectsFixed === null) zoomAffectsFixed = detectZoomAffectsFixed();
  return zoomAffectsFixed ? zoom : 1;
}
