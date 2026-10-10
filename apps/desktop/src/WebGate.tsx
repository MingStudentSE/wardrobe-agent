// Web 门禁（ADR0029 D4）：生产浏览器模式下的首启引导与会话登录。
// Tauri 与 dev 模式直接放行（桌面壳与开发桥有自己的可信模型）。
import React, { useEffect, useState } from 'react';
import { platformMode } from './api';
import './web-gate.css';

type Mode = 'loading' | 'bootstrap' | 'login' | 'ready';

export function WebGate({ children }: { children: React.ReactNode }) {
  const [mode, setMode] = useState<Mode>('loading');
  const [bootstrapPath, setBootstrapPath] = useState('');
  const [token, setToken] = useState('');
  const [connectionToken, setConnectionToken] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const gated = platformMode === 'web';

  async function refresh() {
    setError('');
    try {
      const response = await fetch('/auth/state');
      const body = await response.json();
      const result = body?.result ?? {};
      setBootstrapPath(result.bootstrap_token_path ?? '');
      if (result.authenticated) {
        setMode('ready');
      } else {
        setMode(result.bootstrap_required ? 'bootstrap' : 'login');
      }
    } catch {
      setMode('login');
      setError('暂时无法连接本机衣橱服务。请确认 wardrobe-web 正在运行，然后刷新页面。');
    }
  }

  useEffect(() => {
    if (!gated) {
      setMode('ready');
      return;
    }
    void refresh();
    const onUnauthorized = () => {
      setToken('');
      setMode(current => (current === 'ready' ? 'login' : current));
    };
    window.addEventListener('wardrobe:unauthorized', onUnauthorized);
    return () => window.removeEventListener('wardrobe:unauthorized', onUnauthorized);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!gated || mode === 'ready') return <>{children}</>;

  async function submit(event: React.FormEvent, path: '/auth/bootstrap' | '/auth/login') {
    event.preventDefault();
    if (busy || !token.trim()) return;
    setBusy(true);
    setError('');
    try {
      const response = await fetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token: token.trim() }),
      });
      const body = await response.json();
      if (body?.error) {
        setError(body.error.message ?? '校验失败，请重试。');
        return;
      }
      setToken('');
      if (path === '/auth/bootstrap' && body?.result?.token) {
        // 连接令牌只显示一次：必须让操作员先保存。
        setConnectionToken(String(body.result.token));
      } else {
        await refresh();
      }
    } catch {
      setError('暂时无法连接本机衣橱服务，请确认 wardrobe-web 正在运行。');
    } finally {
      setBusy(false);
    }
  }

  return (
    <main id="main-content" className="web-gate" tabIndex={-1}>
      <div className="settings-content web-gate-card">
        <p className="studio-eyebrow">WARDROBE WEB · 本机服务</p>
        {mode === 'bootstrap' ? (
          <>
            <h1>首次使用：登记本机浏览器</h1>
            <p>
              本机衣橱还没有任何客户端授权。为防止其他程序冒充你，首次登记需要数据目录里的一次性引导令牌（仅本机操作员可读）。
            </p>
            <ol className="web-gate-steps">
              <li>
                在本机终端查看引导令牌文件内容（使用一次后作废）：
                <code>{bootstrapPath || '数据目录/web-bootstrap-token'}</code>
              </li>
              <li>把文件内容完整粘贴到下方，提交后浏览器会拿到自己的连接令牌。</li>
            </ol>
            <form onSubmit={event => void submit(event, '/auth/bootstrap')}>
              <label>
                引导令牌
                <input
                  required
                  value={token}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={event => setToken(event.target.value)}
                  placeholder="数据目录 web-bootstrap-token 文件的内容"
                />
              </label>
              <button disabled={busy} aria-busy={busy}>
                {busy ? '正在登记…' : '登记本机浏览器'}
              </button>
            </form>
          </>
        ) : (
          <>
            <h1>登录本机衣橱</h1>
            <p>粘贴本机连接令牌登录。令牌只在登记时显示一次，请保存在你的密码管理器。</p>
            <form onSubmit={event => void submit(event, '/auth/login')}>
              <label>
                连接令牌
                <input
                  required
                  type="password"
                  value={token}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={event => setToken(event.target.value)}
                  placeholder="64 位十六进制连接令牌"
                />
              </label>
              <button disabled={busy} aria-busy={busy}>
                {busy ? '正在登录…' : '登录'}
              </button>
            </form>
          </>
        )}
        {connectionToken && (
          <div className="client-token web-gate-token" aria-live="polite">
            <label>
              本浏览器的连接令牌（只显示这一次）
              <textarea readOnly value={connectionToken} rows={3} />
            </label>
            <p className="helper">
              请立即复制并保存（例如密码管理器）。服务重启或更换浏览器时用它重新登录；不要放进聊天、截图或普通文件。
            </p>
            <button onClick={() => void refresh()}>我已保存，进入衣橱</button>
          </div>
        )}
        {error && (
          <p className="feedback error" role="alert">
            {error}
          </p>
        )}
      </div>
    </main>
  );
}
