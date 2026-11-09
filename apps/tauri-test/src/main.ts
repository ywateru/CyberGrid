import { invoke } from '@tauri-apps/api/core';
import {invalidateBrowserLogin, startBrowserLogin} from './browser-login';
import './style.css';

type Port = { id: string; port: number; type: string; transport: string };
type Device = { id: string; name: string; online: boolean; lastSeen: string; ports: Port[] };
type Config = { baseUrl: string; appId: string; deviceName: string; forceFallback: boolean; directStrategy: 'sequential' | 'parallel'; cloudflaredPath: string };
type RecordItem = { id: number; time: string; direction: string; peer: string; kind: string; message?: string; bytes?: number; sha256?: string; transport?: string };
type View = {
  phase: string; config: Config; presetConfig: Config; canResume: boolean; error?: string | null;
  platform: 'desktop' | 'android'; authMode: 'browser';
  local?: { deviceId: string; deviceName: string; userId: string; ports: Port[] } | null;
  devices: Device[]; records: RecordItem[];
  connection?: { peerId: string; peerName: string; localAddress: string; transport: string; connected: boolean } | null;
  status: string;
};
const app = document.querySelector<HTMLDivElement>('#app')!;
let view: View | null = null;
let draft: Config = { baseUrl: '', appId: '', deviceName: '', forceFallback: false, directStrategy: 'sequential', cloudflaredPath: '' };
let messageDraft = '';
let selectedId = '';
let pending = false;
let activeCommands = 0;
let localError = '';
let showAdvanced = false;
let fingerprint = '';
let initial = true;
const escape = (value: unknown) => String(value ?? '').replace(/[&<>"']/g, (s) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[s]!));
const tag = (transport: string) => ['webrtc', 'webrtc-direct'].includes(transport) ? 'WebRTC 直连' : transport === 'iroh-direct' ? 'iroh 直连' : transport === 'cloudflare-tunnel' ? 'Cloudflare 回退 · 内层 mTLS' : escape(transport);
const datetime = (value: string) => { const d = new Date(value); return Number.isNaN(+d) ? value : d.toLocaleTimeString('zh-CN', { hour12: false }); };
const disabled = () => pending ? 'disabled' : '';

function render() {
  const focus = document.activeElement as HTMLInputElement | null;
  const focusId = focus?.id;
  const start = focus && 'selectionStart' in focus ? focus.selectionStart : null;
  const end = focus && 'selectionEnd' in focus ? focus.selectionEnd : null;
  const ready = view?.phase === 'ready';
  const hasSession = ready || view?.phase === 'recovering';
  const authorizing = view?.phase === 'authorizing';
  const busy = pending || ['connecting', 'closing'].includes(view?.phase ?? '');
  const devices = view?.devices.filter((d) => d.id !== view?.local?.deviceId) ?? [];
  if (!selectedId && devices.some((d) => d.online)) selectedId = devices.find((d) => d.online)!.id;
  const selected = devices.find((d) => d.id === selectedId);
  const hasMapping = view?.connection?.peerId === selectedId;
  const mapped = hasMapping && ready && selected?.online;
  const connected = mapped && view?.connection?.connected;
  app.innerHTML = `
    <header class="topbar"><div class="brand"><span class="brand-symbol" aria-hidden="true"><i></i><i></i><i></i></span><div><strong>CyberGrid Lab</strong><span>Rust SDK / Tauri 2</span></div></div><span class="phase"><i class="dot ${ready ? 'online' : ''}"></i>${ready ? '已登录' : view?.phase === 'recovering' ? '自动恢复中' : authorizing ? '等待授权' : '连接测试'}</span></header>
    <main>
      ${!hasSession ? `<section class="login-pane">
        <div class="intro"><h1>注册／登录后，连接另一端。</h1><p>在 Logto 托管页完成注册／登录，在 Mac 和 Android 使用同一个账号与应用。业务消息通过设备间的加密通道传输。</p></div>
        <form id="login-form" class="config-form">
          <p class="preset-note">已预置公开测试服务与应用。Mac 和 Android 使用同一应用；每台设备分别授权。</p>
          ${!authorizing && !busy && !initial ? '<button id="use-preset" type="button" class="text-button">使用预置测试配置</button>' : ''}
          <label for="device-name">本机名称</label><input id="device-name" name="deviceName" maxlength="80" value="${escape(draft.deviceName)}" placeholder="例如：我的 Mac" ${authorizing || busy ? 'disabled' : ''} required>
          <button id="advanced-toggle" type="button" class="text-button" aria-expanded="${showAdvanced}">${showAdvanced ? '收起连接配置' : '连接配置'}</button>
          ${showAdvanced ? `<div class="advanced"><label for="server-url">中央服务</label><input id="server-url" name="baseUrl" type="url" value="${escape(draft.baseUrl)}" ${authorizing || busy ? 'disabled' : ''}><label for="app-id">Logto Native 应用 ID</label><input id="app-id" name="appId" autocomplete="off" autocapitalize="none" autocorrect="off" spellcheck="false" maxlength="21" value="${escape(draft.appId)}" ${authorizing || busy ? 'disabled' : ''}><p class="subtle">自定义服务与应用须通过可信认证配置核对，不能替换为个人控制台的 SPA 应用。</p><label for="transport-mode">连接策略</label><select id="transport-mode" ${authorizing || busy ? 'disabled' : ''}><option value="auto" ${!draft.forceFallback && draft.directStrategy !== 'parallel' ? 'selected' : ''}>顺序：WebRTC → iroh 直连 → Cloudflare</option><option value="parallel" ${!draft.forceFallback && draft.directStrategy === 'parallel' ? 'selected' : ''}>并行：WebRTC / iroh 竞速，失败后 Cloudflare</option><option value="fallback" ${draft.forceFallback ? 'selected' : ''}>强制 Cloudflare 回退（测试）</option></select><label for="provider-path">macOS cloudflared 路径（留空使用 SDK 内置）</label><input id="provider-path" value="${escape(draft.cloudflaredPath)}" placeholder="默认使用内置 provider；Android 忽略此项" ${authorizing || busy ? 'disabled' : ''}></div>` : ''}
          ${authorizing ? `<div class="authorization" role="status"><h2>在系统浏览器完成网页登录</h2><p>浏览器完成登录后会返回本机。不要在测试 App 中输入账号密码；手机请保持 App 打开。</p><p class="subtle">等待期间可取消，本机将安全处理已取得的授权。</p><div class="actions"><button type="button" id="cancel-login" class="secondary">取消网页登录</button></div></div>` : `<div class="actions login-actions"><button type="submit" class="primary" ${busy || initial ? 'disabled' : ''}>${busy ? '正在处理…' : '网页登录'}</button>${view?.canResume ? `<button type="button" id="resume-login" class="secondary" ${busy ? 'disabled' : ''}>恢复本机授权</button><button type="button" id="revoke-saved" class="text-button" ${busy ? 'disabled' : ''}>退出并撤销本机授权</button>` : ''}${view?.phase === 'connecting' || busy ? '<button type="button" id="cancel-login" class="text-button">取消</button>' : ''}</div>`}
        </form>
        <p class="privacy">账号密码只输入 Logto 托管页面。网页登录使用官方 Logto SDK，授权交接后由原生 SDK 管理本机私有凭据。</p>
      </section>` : `<section class="session-strip"><div><span class="eyebrow">本机</span><h1>${escape(view?.local?.deviceName)}</h1><p class="mono device-id">${escape(view?.local?.deviceId)}</p></div><div class="session-meta"><span>应用 <code>${escape(view?.config.appId)}</code></span><span>账号 <code>${escape(view?.local?.userId)}</code></span><button id="logout" class="secondary">退出登录</button></div></section>
      <section class="workspace"><div class="device-pane"><div class="section-head"><h2>同应用设备</h2><button id="refresh" class="text-button" ${disabled()}>刷新</button></div><p class="subtle">只显示同账号、同应用的真实终端。</p><div class="device-list">${devices.length ? devices.map((d) => `<button class="device-row ${d.id === selectedId ? 'selected' : ''}" data-device="${escape(d.id)}"><div class="device-avatar" aria-hidden="true">${d.name.toLowerCase().includes('android') ? 'A' : 'M'}</div><div class="device-info"><strong>${escape(d.name)}</strong><span><i class="dot ${d.online ? 'online' : ''}"></i>${d.online ? '在线' : '离线'} · ${d.ports.length} 个端口</span><code>${escape(d.id)}</code></div></button>`).join('') : `<div class="empty"><p>还没有另一台设备</p><span>在另一端打开 App，使用同一个账号和应用 ID 登录，再刷新。</span></div>`}</div><div class="local-ports"><h3>本机已发布端口</h3>${view?.local?.ports.map((p) => `<p><span>${escape(p.id)}</span><code>${p.port} / ${escape(p.type)}</code></p>`).join('')}</div></div>
      <div class="communication"><div class="section-head"><h2>${selected ? escape(selected.name) : '选择一个设备'}</h2>${selected ? `<span class="status-label"><i class="dot ${selected.online ? 'online' : ''}"></i>${selected.online ? '在线' : '离线'}</span>` : ''}</div>
        ${selected ? `<div class="remote-ports">${selected.ports.map((p) => `<span><code>${escape(p.id)}</code> ${escape(p.type)} :${p.port}</span>`).join('') || '<span>对端尚未发布端口</span>'}</div><div class="connection-line"><span class="connection-end">本机</span><span class="connection-wire ${connected ? 'connected' : ''}"></span><span class="connection-end">${escape(selected.name)}</span></div><div class="connection-status" role="status">${connected ? `<strong>${tag(view!.connection!.transport)}</strong><code>本地映射 ${escape(view!.connection!.localAddress)}</code>` : hasMapping ? `<span>本地映射监听已保留，恢复就绪后的新连接会重新握手</span><code>${escape(view!.connection!.localAddress)}</code>` : '<span>连接后可以发送消息和校验数据</span>'}</div><div class="actions"><button id="connect" class="${connected ? 'secondary' : 'primary'}" ${pending || !ready || !selected.online ? 'disabled' : ''}>${connected ? '重新连接' : '连接设备'}</button>${hasMapping ? '<button id="disconnect" class="text-button">断开</button>' : ''}</div><form id="message-form"><label for="message">发送消息</label><textarea id="message" rows="3" maxlength="4096" placeholder="输入消息，在另一端查看收件记录" ${!mapped || pending ? 'disabled' : ''}>${escape(messageDraft)}</textarea><div class="actions"><button type="submit" class="primary" ${!mapped || pending ? 'disabled' : ''}>发送并等待对端确认</button><button type="button" id="probe" class="secondary" ${pending || !ready || !selected.online ? 'disabled' : ''}>64 KiB 传输校验</button></div></form>` : `<div class="empty communication-empty"><p>连接从设备列表开始</p><span>选中在线设备，建立加密通道，然后双向发送消息。</span></div>`}
      </div></section><section class="activity"><div class="section-head"><h2>本次收发记录</h2><span class="subtle">仅保留在本机内存</span></div><div class="records" aria-live="polite">${view?.records.length ? [...view.records].reverse().map((r) => `<article class="record ${r.direction === 'received' ? 'incoming' : ''}"><div><span class="record-direction">${r.direction === 'received' ? '收到' : r.direction === 'sent' ? '已确认送达' : '传输校验'}</span><strong>${escape(r.peer)}</strong><time>${escape(datetime(r.time))}</time></div>${r.kind === 'message' ? `<p>${escape(r.message)}</p>` : `<p>${r.bytes?.toLocaleString()} 字节 · ${r.direction === 'received' ? '已计算接收 SHA256' : 'SHA256 一致'}</p><code class="hash">${escape(r.sha256)}</code>`}<span class="record-transport">${tag(r.transport ?? '')}</span></article>`).join('') : '<p class="empty-records">消息送达或收到消息后，记录会显示在这里。</p>'}</div></section>`}
      <div id="error" class="error ${localError || view?.error ? '' : 'hidden'}" role="alert">${escape(localError || view?.error)}</div>
      <footer><span><i class="dot ${pending ? 'working' : ''}"></i>${escape(pending ? '正在执行，请稍候…' : view?.status || '准备连接原生 SDK')}</span><span>中央仅协调连接 · 业务数据端到端加密</span></footer>
    </main>`;
  bind();
  if (focusId) {
    const el = document.getElementById(focusId) as HTMLInputElement | HTMLTextAreaElement | null;
    if (el && !el.disabled) { el.focus(); if (start !== null && end !== null && ['INPUT', 'TEXTAREA'].includes(el.tagName)) { try { el.setSelectionRange(start, end); } catch { /* input type url has no text selection API */ } } }
  }
}
function on(id: string, name: string, fn: EventListener) { document.getElementById(id)?.addEventListener(name, fn); }
function bind() {
  on('app-id', 'input', (e) => draft.appId = (e.target as HTMLInputElement).value);
  on('device-name', 'input', (e) => draft.deviceName = (e.target as HTMLInputElement).value);
  on('server-url', 'input', (e) => draft.baseUrl = (e.target as HTMLInputElement).value);
  on('provider-path', 'input', (e) => draft.cloudflaredPath = (e.target as HTMLInputElement).value);
  on('transport-mode', 'change', (e) => { const mode = (e.target as HTMLSelectElement).value; draft.forceFallback = mode === 'fallback'; draft.directStrategy = mode === 'parallel' ? 'parallel' : 'sequential'; });
  on('message', 'input', (e) => messageDraft = (e.target as HTMLTextAreaElement).value);
  on('use-preset', 'click', () => { if (view && !pending && view.phase === 'signed_out') { draft = {...view.presetConfig}; localError = ''; showAdvanced = true; render(); } });
  on('advanced-toggle', 'click', () => { showAdvanced = !showAdvanced; render(); });
  on('login-form', 'submit', (e) => { e.preventDefault(); void browserLogin(); });
  on('resume-login', 'click', () => void action('resume_login', { config: draft }));
  on('cancel-login', 'click', () => { invalidateBrowserLogin(); void action('cancel_login'); });
  on('logout', 'click', () => { invalidateBrowserLogin(); void action('logout'); });
  on('revoke-saved', 'click', () => { invalidateBrowserLogin(); void action('logout'); });
  on('refresh', 'click', () => void action('refresh_devices'));
  document.querySelectorAll<HTMLButtonElement>('[data-device]').forEach((button) => button.addEventListener('click', () => { selectedId = button.dataset.device!; localError = ''; render(); }));
  on('connect', 'click', () => void action('connect_device', { deviceId: selectedId }));
  on('disconnect', 'click', () => void action('disconnect_device'));
  on('message-form', 'submit', (e) => { e.preventDefault(); if (!messageDraft.trim()) { localError = '请输入要发送的消息。'; render(); return; } void action('send_message', { text: messageDraft }).then((ok) => { if (ok) { messageDraft = ''; render(); } }); });
  on('probe', 'click', () => void action('probe_device', { deviceId: selectedId }));
}
async function action(command: string, args: Record<string, unknown> = {}): Promise<boolean> {
  if (pending && !['logout', 'cancel_login', 'disconnect_device'].includes(command)) return false;
  activeCommands += 1; pending = true; localError = ''; render();
  try { await invoke(command, args); await refresh(true); return true; }
  catch (error) { localError = String(error); return false; }
  finally { activeCommands -= 1; pending = activeCommands > 0; render(); }
}
async function browserLogin(): Promise<void> {
  if (pending || initial) return;
  activeCommands += 1; pending = true; localError = ''; render();
  try { await startBrowserLogin({...draft}); }
  catch (error) { localError = error instanceof Error ? error.message : '网页登录未完成，请检查本机授权状态。'; }
  finally { activeCommands -= 1; pending = activeCommands > 0; await refresh(true); render(); }
}
async function refresh(force = false) {
  try {
    const next = await invoke<View>('lab_view');
    if (initial) { draft = { ...next.config }; initial = false; }
    const nextFingerprint = JSON.stringify(next);
    view = next;
    if (force || nextFingerprint !== fingerprint) { fingerprint = nextFingerprint; render(); }
  } catch (error) {
    if (!view) { localError = `无法连接原生后端：${String(error)}。请在 CyberGrid Lab App 内运行。`; render(); }
  }
}
render();
void refresh();
setInterval(() => { void refresh(); }, 1500);
