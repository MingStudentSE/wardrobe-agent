// 平台能力适配层（ADR0026 D6）：桌面 Tauri 与本机 Web 服务共用同一 UI，
// 平台差异在这里声明。Web 不具备的能力必须如实标注，不伪称与原生等价。
export const isTauriRuntime = '__TAURI_INTERNALS__' in window;

const IMAGE_ACCEPT = 'image/jpeg,image/png,image/webp';

/** 浏览器文件选择（<input type="file">）。取消或未选择返回 null。 */
export function pickFiles(options: { multiple?: boolean; accept?: string } = {}): Promise<File[] | null> {
  return new Promise(resolve => {
    const input = document.createElement('input');
    input.type = 'file';
    if (options.multiple) input.multiple = true;
    input.accept = options.accept ?? IMAGE_ACCEPT;
    input.style.display = 'none';
    input.setAttribute('aria-hidden', 'true');
    document.body.appendChild(input);
    let settled = false;
    const finish = (files: File[] | null) => {
      if (settled) return;
      settled = true;
      input.remove();
      resolve(files);
    };
    input.addEventListener('change', () => finish(Array.from(input.files ?? [])));
    input.addEventListener('cancel', () => finish(null));
    input.click();
  });
}

/** Tauri 本机路径选择（仅在桌面运行时调用；动态导入，Web 构建不依赖它启动）。 */
export async function pickImagePaths(options: { multiple?: boolean } = {}): Promise<string[] | null> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const result = await open({
    multiple: !!options.multiple,
    filters: [{ name: '衣物图片', extensions: ['jpg', 'jpeg', 'png', 'webp', 'heic', 'heif'] }],
  });
  if (!result) return null;
  return Array.isArray(result) ? result : [result];
}

/** 浏览器端不具备的平台能力说明（设置页如实呈现，不用占位按钮）。 */
export const BROWSER_LIMITS = {
  clientAuthorize:
    '浏览器会话不能登记、查看或撤销外部客户端授权：client.* 属于仅可信本机入口。请使用桌面端「设置 → CLI / MCP 与外部 Agent」，或在本机运行 wardrobe-cli。',
  credentials:
    '浏览器端不能写入系统安全存储：模型凭据（API key）请在桌面端「设置 → 模型与连接」配置，或使用 wardrobe-cli。此页的模型连接信息仍可查看与保存。',
  encryption:
    '加密空间、设备配对与安全密钥由本机系统安全存储保护，浏览器端不提供管理入口。请使用桌面端或 wardrobe-cli。',
  folderPicker:
    '浏览器端不能选择本机文件夹：此操作请在桌面端完成，或在输入框中直接填写本机绝对路径。',
} as const;
