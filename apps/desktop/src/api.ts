// 传输适配层（ADR0026 D6）：业务代码只调用 api()/uploadPhoto()/uploadBatch()。
// - tauri：桌面壳，可信直调 Application（现状保留）
// - dev：vite 开发桥（仅 DEV，非生产交付）
// - web：wardrobe-web 本机服务（生产浏览器）；业务经 execute_as_client 会话 API
import { isTauriRuntime } from './platform';

export const platformMode: 'tauri' | 'dev' | 'web' = isTauriRuntime
  ? 'tauri'
  : import.meta.env.DEV
    ? 'dev'
    : 'web';

let csrfCache: string | null = null;

function failWithUnauthorized() {
  csrfCache = null;
  window.dispatchEvent(new CustomEvent('wardrobe:unauthorized'));
}

async function webCsrf(): Promise<string> {
  if (csrfCache) return csrfCache;
  const response = await fetch('/auth/state');
  const body = await response.json();
  const result = body?.result ?? {};
  const csrf: unknown = result.csrf;
  if (!result.authenticated || typeof csrf !== 'string') {
    failWithUnauthorized();
    throw { code: 'unauthorized', message: '请先登录本机衣橱服务' };
  }
  csrfCache = csrf;
  return csrf;
}

async function webFetch(url: string, init: RequestInit): Promise<any> {
  const response = await fetch(url, init);
  let body: any = null;
  try {
    body = await response.json();
  } catch {
    throw { code: 'service_unavailable', message: '本机服务响应异常，请确认 wardrobe-web 正在运行。' };
  }
  if (body?.error) {
    // 撤销/到期与未登录一样：立即回到登录门（ADR0026 D4：撤销即时传播）
    if (
      body.error.code === 'unauthorized' ||
      response.status === 401 ||
      body.error.code === 'authorization_revoked' ||
      body.error.code === 'authorization_expired'
    ) {
      failWithUnauthorized();
    }
    throw body.error;
  }
  return body?.result;
}

async function tauriInvoke<T>(operation: string, input: object): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return (await invoke('application', { operation, input })) as T;
}

async function devBridge<T>(operation: string, input: object): Promise<T> {
  const response = await fetch('/__wardrobe_dev_api', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ operation, input }),
  });
  const body = await response.json();
  if (body.error) throw body.error;
  return body.result;
}

export async function api<T>(operation: string, input: object = {}): Promise<T> {
  if (platformMode === 'tauri') return tauriInvoke<T>(operation, input);
  if (platformMode === 'dev') return devBridge<T>(operation, input);
  const csrf = await webCsrf();
  return webFetch('/api', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-Wardrobe-CSRF': csrf },
    body: JSON.stringify({ operation, input }),
  }) as Promise<T>;
}

export type PhotoUploadOptions = {
  purpose?: 'asset' | 'person';
  personKind?: 'self' | 'model';
  rightsConfirmed?: boolean;
};

/** 单图上传：dev 走开发桥（assets.import）；web 经 /api/upload 落盘后走核心。 */
export async function uploadPhoto(member: string, file: File, options: PhotoUploadOptions = {}): Promise<any> {
  if (platformMode === 'dev') {
    const response = await fetch('/__wardrobe_dev_upload?member=' + encodeURIComponent(member), {
      method: 'POST',
      headers: { 'Content-Type': file.type || 'image/png' },
      body: file,
    });
    const body = await response.json();
    if (body.error) throw body.error;
    return body.result;
  }
  const csrf = await webCsrf();
  const query = new URLSearchParams({ member });
  if (options.purpose === 'person') {
    query.set('purpose', 'person');
    query.set('person_kind', options.personKind ?? 'self');
    query.set('rights_confirmed', String(!!options.rightsConfirmed));
  }
  return webFetch('/api/upload?' + query.toString(), {
    method: 'POST',
    headers: {
      'Content-Type': file.type || 'application/octet-stream',
      'X-Wardrobe-CSRF': csrf,
    },
    body: file,
  });
}

export type BatchImage = { name: string; type: string; data: string };

/** 批量上传：dev 走开发桥；web 经 /api/batch-upload（同一段核心 imports.create 语义）。 */
export async function uploadBatch(
  member: string,
  operationKey: string,
  images: BatchImage[],
): Promise<any> {
  if (platformMode === 'dev') {
    const response = await fetch('/__wardrobe_dev_batch_upload', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ member_id: member, operation_key: operationKey, images }),
    });
    const body = await response.json();
    if (body.error) throw body.error;
    return body.result;
  }
  const csrf = await webCsrf();
  return webFetch('/api/batch-upload', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-Wardrobe-CSRF': csrf },
    body: JSON.stringify({ member_id: member, operation_key: operationKey, images }),
  });
}
