// 共享错误文案提取器（#74）：六份页面内联实现（ImageImport/ProductDisplay/
// ImageMerge/ImageEditor/ItemEditor/DiagnosticsPanel）收敛为一个实现。
// API 适配层抛出的是 {code,message[,trace_id]} 形态的错误对象；有 message 就
// 展示核心文案，否则用各页面自己的兜底文案（保持迁移前逐页现状，由调用方传入）。
export function errorMessage(e: unknown, fallback: string): string {
  return typeof e === 'object' && e !== null && 'message' in e ? String(e.message) : fallback;
}
