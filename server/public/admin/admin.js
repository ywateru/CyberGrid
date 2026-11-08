'use strict';
(() => {
  const byId = id => document.getElementById(id);
  const ui = Object.fromEntries(['login','key','actions','refresh','logout','workspace','filters','app','online','status','error','snapshot','sampled','totals','epoch','started','uptime','devices','prev','next','page'].map(name => [name, byId('admin-' + name)]));
  ui.key.type = ['pass', 'word'].join('');
  ui.key.disabled = false;
  let key = '', generation = 0, controller, timer, page = 1, lastPage;
  let filter = {appId: '', online: ''};
  const isCount = value => Number.isSafeInteger(value) && value >= 0;
  const isText = value => typeof value === 'string' && value.length <= 4096;
  const isDate = value => isText(value) && Number.isFinite(Date.parse(value));
  function validSnapshot(data) {
    if (!data || !isDate(data.sampledAt) || !data.coordinator || !isText(data.coordinator.epoch) || !isDate(data.coordinator.startedAt) || !isCount(data.coordinator.uptimeSeconds)) return false;
    if (!data.totals || !['sessions','devices','onlineDevices','offlineDevices','ports','queuedSignals','waiters'].every(name => isCount(data.totals[name]))) return false;
    const p = data.page;
    if (!p || ![p.number,p.size,p.totalDevices,p.totalPages].every(isCount) || p.number < 1 || p.size < 1 || p.size > 100 || p.totalPages < 1 || typeof p.hasNext !== 'boolean') return false;
    return Array.isArray(data.devices) && data.devices.length <= p.size && data.devices.every(device => device && ['id','appId','name'].every(name => isText(device[name])) && typeof device.online === 'boolean' && isDate(device.lastSeen) && typeof device.hasTunnel === 'boolean' && Array.isArray(device.ports) && device.ports.length <= 32 && device.ports.every(port => port && Number.isInteger(port.port) && port.port >= 1 && port.port <= 65535 && isText(port.type)));
  }
  function el(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = String(text);
    if (className) node.className = className;
    return node;
  }
  function clearSnapshot() {
    ui.snapshot.hidden = true;
    for (const name of ['sampled','totals','epoch','started','uptime','devices','page']) ui[name].replaceChildren();
    lastPage = undefined;
    ui.prev.disabled = ui.next.disabled = true;
  }
  function cancel() {
    generation++;
    clearTimeout(timer);
    controller?.abort();
    controller = undefined;
  }
  function logout(message = '已退出。管理密钥和快照已清除。') {
    cancel(); key = ''; ui.key.value = ''; clearSnapshot();
    ui.workspace.hidden = ui.actions.hidden = true;
    ui.login.hidden = false; ui.error.hidden = true; ui.error.textContent = '';
    ui.status.textContent = message;
    byId('login-error').textContent = '';
    ui.key.focus();
  }
  function render(data) {
    ui.sampled.textContent = '采样时间：' + new Date(data.sampledAt).toLocaleString() + ' · 每 5 秒刷新';
    ui.epoch.textContent = data.coordinator.epoch;
    ui.started.textContent = new Date(data.coordinator.startedAt).toLocaleString();
    ui.uptime.textContent = data.coordinator.uptimeSeconds.toLocaleString();
    ui.totals.replaceChildren(...Object.entries({sessions:'有效会话',devices:'设备',onlineDevices:'在线设备',offlineDevices:'离线设备',ports:'公开端口',queuedSignals:'排队信令',waiters:'等待者'}).map(([name,label]) => {
      const item = el('div'); item.append(el('dt', label), el('dd', data.totals[name].toLocaleString())); return item;
    }));
    ui.devices.replaceChildren(...data.devices.map(device => {
      const row = el('article', undefined, 'admin-device');
      const identity = el('div'); identity.append(el('h3', device.name || '未命名设备'), el('p', device.id, 'admin-id'), el('p', '应用：' + device.appId, 'admin-id'));
      const heartbeat = el('div'); heartbeat.append(el('p', device.online ? '在线 · 心跳有效' : '离线 · 心跳租约已过期', device.online ? 'admin-online' : ''), el('p', '最后心跳：' + new Date(device.lastSeen).toLocaleString()));
      const ports = el('div'); ports.append(el('p', device.ports.length ? '公开端口' : '未公布端口'));
      if (device.ports.length) { const list = el('ul'); list.append(...device.ports.map(port => el('li', port.port + ' / ' + port.type))); ports.append(list); }
      ports.append(el('p', '回退通道：' + (device.hasTunnel ? '已公布' : '未公布')));
      row.append(identity, heartbeat, ports); return row;
    }));
    if (!data.devices.length) ui.devices.append(el('p', '当前筛选没有设备。可调整应用或心跳筛选后重试。', 'admin-empty'));
    lastPage = data.page; page = data.page.number;
    ui.page.textContent = '筛选结果 ' + data.page.totalDevices + ' 台 · 第 ' + page + ' / ' + data.page.totalPages + ' 页';
    ui.prev.disabled = page <= 1; ui.next.disabled = !data.page.hasNext;
    ui.snapshot.hidden = false;
  }
  async function refresh() {
    if (!key) return;
    cancel(); const current = generation; controller = new AbortController();
    const query = new URLSearchParams({page: String(page), pageSize:'50'});
    if (filter.appId) query.set('appId', filter.appId);
    if (filter.online) query.set('online', filter.online);
    ui.refresh.disabled = true; ui.error.hidden = true; ui.error.textContent = '';
    ui.status.textContent = '正在采样协调状态…';
    let delay = 5000;
    try {
      const response = await fetch('/api/v1/admin/overview?' + query, {method:'GET', headers:{Authorization:'Bearer ' + key, Accept:'application/json'}, credentials:'omit', cache:'no-store', redirect:'error', signal:controller.signal});
      if (current !== generation) return;
      if (!response.ok) {
        if (response.status === 401) { logout('管理密钥不正确，已清除。请重新输入。'); return; }
        if (response.status === 429) delay = 60000;
        throw new Error(({503:'管理接口未启用或服务暂不可用。请检查 owner 的管理配置。',429:'请求过于频繁。已暂停 60 秒，随后自动重试。',403:'请求来源被拒绝。请从配置的站点同源访问。'})[response.status] || '无法获取协调状态（HTTP ' + response.status + '）。');
      }
      const data = await response.json();
      if (current !== generation) return;
      if (!validSnapshot(data)) throw new Error('响应格式不符合管理接口约定，请检查服务版本。');
      render(data); ui.status.textContent = '协调快照已更新。';
    } catch (error) {
      if (current !== generation) return;
      clearSnapshot(); ui.error.textContent = error instanceof TypeError ? '网络请求失败。请检查网络或服务状态。' : error.message || '无法读取协调状态。';
      ui.error.hidden = false; ui.status.textContent = '当前没有可用的成功快照。';
    } finally {
      if (current === generation && key) { ui.refresh.disabled = false; timer = setTimeout(refresh, delay); }
    }
  }
  ui.login.addEventListener('submit', event => {
    event.preventDefault(); const entered = ui.key.value; ui.key.value = '';
    const size = new TextEncoder().encode(entered).length;
    if (size < 32 || size > 256) { byId('login-error').textContent = '请输入 32–256 UTF8 字节的独立管理密钥。'; ui.key.focus(); return; }
    key = entered; page = 1; filter = {appId:'',online:''}; ui.app.value = ui.online.value = '';
    byId('login-error').textContent = ''; ui.login.hidden = true; ui.workspace.hidden = ui.actions.hidden = false; clearSnapshot(); refresh();
  });
  ui.filters.addEventListener('submit', event => { event.preventDefault(); page = 1; filter = {appId:ui.app.value.trim(),online:ui.online.value}; clearSnapshot(); refresh(); });
  ui.refresh.addEventListener('click', refresh);
  ui.logout.addEventListener('click', () => logout());
  ui.prev.addEventListener('click', () => { if (lastPage && page > 1) { page--; clearSnapshot(); refresh(); } });
  ui.next.addEventListener('click', () => { if (lastPage?.hasNext) { page++; clearSnapshot(); refresh(); } });
  window.addEventListener('pagehide', () => { cancel(); key = ''; ui.key.value = ''; clearSnapshot(); ui.workspace.hidden = ui.actions.hidden = true; ui.login.hidden = false; });
  window.addEventListener('pageshow', event => { if (event.persisted) logout('页面已恢复，请重新输入管理密钥。'); });
})();
