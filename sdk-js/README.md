# CyberGrid JS SDK

项目 v0.7.0 新增的 TypeScript/JavaScript SDK，供浏览器在 Logto 登录后只读查询本账号的应用、设备和端口，并取得端口主动申报的 Cloudflare Quick Tunnel 地址。SDK 不包含运行时依赖，不注册设备、不申报端口、不发送信令，也不实现 WebRTC、iroh 或业务数据转发。v0.6.0 Release 不包含此 SDK。

## 与 Logto 接入

宿主先使用官方 Logto JavaScript SDK 或 React SDK 完成登录，并保持 Logto 授权与 refresh token 的唯一所有权。使用服务端 `LOGTO_BROWSER_CLIENT_ID` 对应的普通 SPA；它与设备的 Native Application ID 是不同用途。SPA 在 Logto 登记前端的精确回跳及 origin，并获得固定 API resource 的 `connect` 权限。

CyberGrid JS SDK 只调用宿主的 `getAccessToken` 回调。它在调用该回调前核对服务返回的 issuer、SPA client ID 和 resource 是否与构造参数完全一致；不能将 ID token 或 M2M token 传入。CyberGrid console session bearer 仅保存在 SDK 内存中，页面刷新后重新连接，不写入 Web Storage 或日志。

会话和 token 回调保存在 JavaScript `#private` 字段中，普通对象枚举、`JSON.stringify()` 与常规对象输出不会带出会话 bearer。宿主仍应避免记录自己的 Logto token 或把 SDK 实例交给不受信代码。

源码可作为本地包接入，先在本目录安装构建依赖并构建，再从宿主项目安装这个本地目录：

```sh
npm ci
npm run build
# 在宿主项目目录中执行：npm install /absolute/path/to/CyberGrid/sdk-js
```

```ts
import { CyberGridClient } from '@cybergrid/sdk-js';

// logtoClient 是宿主已初始化、已完成登录的官方 Logto SDK 实例。
const grid = new CyberGridClient({
  baseUrl: 'https://grid.example.com',
  issuer: 'https://your-tenant.logto.app/oidc',
  clientId: 'your-console-spa-id',
  resource: 'https://grid.example.com/api',
  getAccessToken: async ({ resource }) => {
    const token = await logtoClient.getAccessToken(resource);
    if (!token) throw new Error('请先完成宿主的 Logto 登录');
    return token;
  },
});

await grid.connect();
console.log(grid.getLoginState()); // connected：本 SDK 的 console 会话已建立
const apps = await grid.apps();
// 使用用户选中的真实 Native 应用 ID，不是上面的 SPA clientId。
const devices = await grid.devices(selectedNativeApplicationId);
const url = await grid.getPortTunnelUrl(
  selectedNativeApplicationId,
  selectedDeviceId,
  selectedPortId,
);
if (url) {
  // 这里只取得地址。是否访问以及业务鉴权由宿主应用决定。
}

await grid.disconnect(); // 仅注销本 console 会话，不代替宿主退出 Logto。
```

React 宿主可以将官方 `useLogto()` 返回的 `getAccessToken` 包装成同一回调；应在宿主确认 `isAuthenticated` 后调用 `connect()`。SDK 的 `getLoginState()` 反映 CyberGrid console 会话的本地状态，不代替官方 Logto SDK 的登录状态查询。

## API

| 方法 | 用途 |
| --- | --- |
| `connect({signal?})` | 校验可信配置并建立内存中的只读 console 会话 |
| `getLoginState()` | 返回 `disconnected`、`connecting`、`connected` 或 `recovering` |
| `apps({signal?})` | 查询同租户支持的 Native 应用目录，不代表用户已授权每个应用 |
| `devices(appId, {signal?})` | 查询当前用户在选定 Native 应用下的设备和端口 |
| `getPortTunnelUrl(appId, deviceId, portId, {signal?})` | 取得在线设备上该端口显式申报的 URL，设备离线或未申报时返回 `null` |
| `disconnect()` | 清理本地会话并注销该 console 会话，不影响其他设备 |

`getAccessToken` 接收 `{issuer, clientId, resource, scopes, signal}` 并返回 access token；返回 `undefined` 表示宿主需要先登录。资源权限 `scopes` 固定为 `connect`，宿主自行管理 OIDC 登录权限。构造参数可设置 `timeoutMs`。调用支持 `AbortSignal`；取消和退出不能被迟到的 token 或 HTTP 响应重新激活。协调服务重启后，SDK 在同一可信配置与用户身份下单飞重新交换 console 会话；身份改变时停止并返回 `IDENTITY_CHANGED`，不静默切换账号。宿主 Logto 用户改变时应先 `disconnect()`，再为新账号 `connect()`。错误通过固定的 `CyberGridError.code` 表示，不输出原始 OAuth 错误或凭据。

交换结果未知时，SDK 保留同一 epoch 下的 request ID 供重试，避免一次超时就创建新的服务端会话。每次没有确定结果的 HTTP 交换尝试保守计为一项，即使它复用了 request ID；这些未知尝试与已取得但尚未确认注销的会话合计最多保留 8 项。达到上限时停止新的交换并报告 `SESSION_UNAVAILABLE`。宿主应明确处理这个状态，不要自动重建 SDK 实例来绕过上限。目录超过 2 MiB 时返回独立的 `DIRECTORY_TOO_LARGE` 错误。

## 端口地址与跨域部署

`port.tunnelUrl` 是 Rust 宿主主动申报的业务端口地址。设备级 `device.tunnelUrl` 是 Rust SDK 内部加密传输的 WebSocket 入口，浏览器不能把它直接当作业务 HTTP URL。JS SDK 不会用设备地址填补缺失的端口地址，也不自动创建隧道。

同源部署无需 CORS 配置。前端与 CyberGrid 分属不同 origin 时，在服务端配置：

```dotenv
CYBERGRID_BROWSER_ORIGINS=https://frontend.example.com
```

多个 origin 用逗号分隔，仅接受精确 HTTPS origin；开发时允许 HTTP loopback。仅 console 配置、交换、应用/设备只读查询及 console 专用注销开放这些 origin，不使用通配符或 cookie credentials。前端仍须使用受信 SPA，并在 Logto 登记回跳。原生设备、信令和管理 API 不因此开放跨域。

业务端口自身的访问权限、HTTP 协议和 CORS 由对应业务服务负责。公开的 Quick Tunnel 地址不是访问凭据。完整部署参数见 [部署文档](../docs/deployment.md)。

设备完整发布元数据最多 24 KiB；设备目录响应最多 2 MiB。超限由服务明确报告容量错误，SDK 不静默截断设备或返回不完整列表。
