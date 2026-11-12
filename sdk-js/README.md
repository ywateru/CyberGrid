# CyberGrid JS SDK

当前元数据为待发布的 JS SDK 0.2.0（项目目标 v0.8.0），包含 fresh-token、snapshot、directory/watch 与 gated adapter。已发布 v0.7.0 的 JS 0.1.0 保持原内容；版本准备不等于发布或完整样例验收。

项目 v0.7.0 新增的 TypeScript/JavaScript SDK，供浏览器在 Logto 登录后只读查询本账号的应用、设备和端口，并取得端口主动申报的受信地址。SDK 不包含运行时依赖，不注册设备、不申报端口、不发送信令；新增桥接扩展将业务操作交给宿主明确注入的认证 adapter，不内置 WebRTC、iroh 或新的密码协议。v0.6.0 Release 不包含此 SDK。

本文新增的 token 请求上下文、快照、完整目录与安全桥接 API 属于当前未发布源码；已发布归档的行为以对应 tag 的说明为准。

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
import { CyberGridClient, type AccessTokenRequest } from '@cybergrid/sdk-js';

// logtoClient 是宿主已初始化、已完成登录的官方 Logto SDK 实例。
// 宿主串行拥有刷新与缓存操作；SDK 取消等待后，已开始的 OAuth 操作仍需完成其所有权处理。
let tokenOwner = Promise.resolve();
function hostAccessToken(request: AccessTokenRequest) {
  const operation = tokenOwner.then(async () => {
    if (request.signal.aborted) return undefined;
    if (request.forceRefresh) await logtoClient.clearAccessToken();
    let token = await logtoClient.getAccessToken(request.resource);
    let claims = await logtoClient.getAccessTokenClaims(request.resource);
    const remaining = () => typeof claims.exp === 'number'
      ? (claims.exp * 1000 - Date.now()) / 1000 : 0;
    // exp 仅用于缓存新鲜度提示，不能据此信任用户身份；服务端仍会验签。
    if (!request.forceRefresh &&
        remaining() < request.minValidity) {
      await logtoClient.clearAccessToken(); // 不删除 refresh token / 宿主 grant。
      token = await logtoClient.getAccessToken(request.resource);
      claims = await logtoClient.getAccessTokenClaims(request.resource);
    }
    if (request.signal.aborted || !Number.isFinite(remaining()) ||
        remaining() < request.minValidity) return undefined;
    return token;
  });
  tokenOwner = operation.then(() => undefined, () => undefined);
  return operation;
}
const grid = new CyberGridClient({
  baseUrl: 'https://grid.example.com',
  issuer: 'https://your-tenant.logto.app/oidc',
  clientId: 'your-console-spa-id',
  resource: 'https://grid.example.com/api',
  getAccessToken: hostAccessToken,
  minValidity: 60, // 秒，1..3600；默认 60。
});

await grid.connect();
console.log(grid.getLoginState()); // connected：本 SDK 的 console 会话已建立
const apps = await grid.apps();
// apps() 返回 canonical product ID；动态目录中它就是 Native 应用 ID，不能用 SPA 或次级 Native 别名。
const devices = await grid.devices(selectedCanonicalProductId);
const url = await grid.getPortTunnelUrl(
  selectedCanonicalProductId,
  selectedDeviceId,
  selectedPortId,
);
if (url) {
  // 这里只取得地址。是否访问以及业务鉴权由宿主应用决定。
}

await grid.disconnect(); // 仅注销本 console 会话，不代替宿主退出 Logto。
```

官方 Logto client 的 `getAccessToken(resource, organizationId?)` 第二参数不是强制刷新布尔值。上例使用官方 `clearAccessToken()` 清 access cache，再由同一宿主 owner 获取新 token；不要传入 `getAccessToken(resource, true)`。真实宿主应将这个串行 owner 与其所有其他刷新调用统一，不能另建一个并行刷新者。

React 宿主可以将官方 `useLogto()` 返回的 `getAccessToken` 包装成同一回调；应在宿主确认 `isAuthenticated` 后调用 `connect()`。SDK 的 `getLoginState()` 反映 CyberGrid console 会话的本地状态，不代替官方 Logto SDK 的登录状态查询。

## API

| 方法 | 用途 |
| --- | --- |
| `connect({signal?})` | 校验可信配置并建立内存中的只读 console 会话 |
| `snapshot()` | 返回不可变的分层状态快照 |
| `subscribe(callback)` | 同步先回调当前快照，再通知递增 revision；返回取消订阅函数 |
| `setHostBusinessAuthorization(value)` | 宿主设置 `unknown`、`authorized` 或 `denied`，不改变协调权限 |
| `getLoginState()` | 返回 `disconnected`、`connecting`、`connected` 或 `recovering` |
| `apps({signal?})` | 查询同租户支持的 Native 应用目录，不代表用户已授权每个应用 |
| `devices(appId, {signal?})` | 查询当前用户在选定 Native 应用下的设备和端口 |
| `getPortTunnelUrl(appId, deviceId, portId, {signal?})` | 取得在线设备上该端口显式申报的 URL，设备离线或未申报时返回 `null` |
| `directory(productId, {cursor?, signal?})` | 读取当前用户/产品/console 投影的完整原子快照 |
| `watchDirectory(productId, listener, {intervalMs?, signal?})` | 立即读取、串行有界轮询；返回 `{stop, done}` |
| `connectBridge(productId, deviceId, bridgeId, adapter, {timeoutMs?, signal?})` | 用当前完整目录与受信 adapter 建立同步 gate 保护的业务通道 |
| `disconnect()` | 清理本地会话并注销该 console 会话，不影响其他设备 |

`getAccessToken` 接收 `{issuer, clientId, resource, scopes, minValidity, forceRefresh, requestReason, generation, signal}` 并返回 access token；返回 `undefined` 表示宿主无法提供所需授权。`minValidity` 的单位是秒，默认 60，构造参数接受 1..3600 的整数。`scopes` 固定为 `connect`。`requestReason` 仅有 `connect`、`coordination-recovery`、`authorization-rejected`；首次请求的 `forceRefresh` 为 `false`，同一轮明确授权拒绝后的请求才为 `true`。generation 与 AbortSignal 帮助宿主拒绝旧结果，但宿主仍须完成已经开始的 OAuth rotation 的保存或撤销，不能因 SDK 停止等待而丢弃 grant owner。

配置读取使用固定 clientId 查询，严格核对 issuer/clientId/resource/endpoints/scopes。新服务可返回 `policyVersion` 与当前 SPA 允许的 `products`：二者必须同时存在，版本为 64 位小写十六进制，产品数为 1..32，ID 无重复且满足原应用 ID 限制，名称非空且最多 256 字符；老服务同时缺少二者时仍可连接。SDK 保留当前可信产品集合；读取完整配置或恢复会话后，已选 canonical product 不在新集合中时，目录、设备查询及新桥接明确返回 `CONFIGURATION_MISMATCH`，关闭既有业务 gate，不自动切换产品。产品清单不会替代服务端身份验证。`apps()` 只接受已知来源 `logto` 或 `static`，并投影原有只读应用字段；`devices(appId)` 使用目录返回的 canonical product ID。

只有首次交换收到可信配置下明确的 `401` 且固定错误码为 `INVALID_IDENTITY`，并且该 pending 交换此前从未出现未知结果时，SDK 才向同一宿主请求一次 `forceRefresh=true`，并用原 request ID 重试。第二次拒绝、其他 401、`APPLICATION_MISMATCH`、403、网络异常或 5xx 都不会触发这一刷新。已经验证过的用户身份在恢复后仍须一致，变化时清理新 session 并返回 `IDENTITY_CHANGED`；宿主账号变更必须先 `disconnect()` 再连接，不能依赖未验签的 JWT 自行切换身份。

交换结果未知时，SDK 在私有 RAM 中保留原 token、epoch 和 request ID。显式再次 `connect()` 只重放同一交换，不再索取新 token、不更换 request ID。即使原 token 后来过期并收到 `401 INVALID_IDENTITY`，也不能证明早先交换未提交：此时返回 `SESSION_UNAVAILABLE`，固定 reason 为 `exchange-outcome-unknown`、`retryable=false`，不归类为需要重新登录并自动刷新。配置确证 epoch 已改变时可在新 epoch 下重建交换，原有未知尝试仍计入总上限。

每一次未确定结果的 HTTP 交换尝试都独立计入预算，即使复用了同一 request ID。未知尝试、已知但未确认注销的 session 与待处理已知回执合计最多 8 项；达到上限会停止新交换，报告 `SESSION_UNAVAILABLE`、不可自动重试。`disconnect()` 不清空这些未知债务，不能通过重建 SDK 实例或新登录来绕过它们。已知 session 只有在确定清理后才能退役；未知尝试只有在其可能创建的 session 全部失效、原最长 24 小时 TTL 加上所有在途请求完成预算都确定过去后，才有安全退役的基础。当前 SDK 不实现这个基于单调经过时间的自动 GC，也不会根据 `Date.now()` 前跳提前释放预算；它会保持 fail closed，等待能确定处理的迟到回执或由宿主明确处理未决状态，不能假装注销已确认。

### 快照、订阅与固定错误

快照分开 `identity`、`coordination`、`publication`、`reachability` 与 `hostBusinessAuthorization`。Console SDK 不发布设备，也不验证业务流，因此 publication/reachability 恒为 `unknown`；业务授权也默认 `unknown`，只能由宿主主动设置。`identity=verified` 与 `coordination=connected` 只说明当前已验证的只读协调会话，不能证明业务端口已经授权。退出会将业务授权恢复为 `unknown`。

快照包含单调 `revision`、`generation`、`epoch`、固定 `stage` / `reason`、`retryable`、`retryAfterMs`、下一次配置重试时间 `nextRetryAt` 和本地 `diagnosticId`。订阅同步先发当前不可变快照，之后只发送更高 revision，订阅者抛错或重入不能倒置版本或重新激活退出后的客户端。普通只读调用不启动后台轮询；显式 watch 与每个活跃桥接会持有自己的有界目录监测。协调故障通过只读调用单飞恢复，宿主可用快照决定何时重试或停止。

```ts
const unsubscribe = grid.subscribe((state) => {
  renderStatus(state); // 只有固定状态字段，不含 token、私钥或带凭据 URL。
});
grid.setHostBusinessAuthorization('authorized'); // 仅在宿主完成自己的业务鉴权后设置。
// 在视图销毁时调用 unsubscribe()；需要清理会话时另行 await grid.disconnect()。
```

`CyberGridError` 保留原有 `code`，增加固定 `stage`、`reason`、`retryable`、数值 `retryAfterMs` 与本地 UUID `diagnosticId`；不携带原始 provider 错误、响应文本或 cause。错误的诊断 ID 同时出现在对应快照，方便宿主关联本地观察。HTTP `Retry-After` 只接受整数秒，最多 30 秒；不把任意服务端字符串放进错误或快照。目录超过 2 MiB 仍返回独立的 `DIRECTORY_TOO_LARGE`。

只有幂等配置 GET 会自动退避：网络失败、429 或 5xx 最多尝试 3 次，默认退避 100 / 200ms，采用有效 Retry-After 时单次等待最多 1 秒，整个配置阶段共用 `timeoutMs` 总预算。鉴权拒绝、配置不匹配和无效成功响应立即失败。后续 token / 交换与清理请求各自仍有超时，整轮连接最多 `3 * timeoutMs`；所有等待支持 AbortSignal，取消和退出阻止迟到成功重新激活。SDK 不自动重试注册 POST、目录 GET 或业务请求，也不重放业务字节。

## 端口地址与跨域部署

`port.tunnelUrl` 是 Rust 宿主主动申报的业务端口地址，默认只接受合法 Quick Tunnel HTTPS 根地址；显式受信 provider 可以授权精确 HTTPS origin/path。设备级 `device.tunnelUrl` 是 Rust SDK 内部加密传输的 WebSocket 入口，浏览器不能把它直接当作业务 HTTP URL。JS SDK 不会用设备地址填补缺失的端口地址，也不自动创建隧道。

同源部署无需 CORS 配置。前端与 CyberGrid 分属不同 origin 时，在服务端配置：

```dotenv
CYBERGRID_BROWSER_ORIGINS=https://frontend.example.com
```

多个 origin 用逗号分隔，仅接受精确 HTTPS origin；开发时允许 HTTP loopback。仅 console 配置、交换、应用/设备只读查询及 console 专用注销开放这些 origin，不使用通配符或 cookie credentials。前端仍须使用受信 SPA，并在 Logto 登记回跳。原生设备、信令和管理 API 不因此开放跨域。

业务端口自身的访问权限、HTTP 协议和 CORS 由对应业务服务负责。公开的 Quick Tunnel 地址不是访问凭据。完整部署参数见 [部署文档](../docs/deployment.md)。

设备完整发布元数据最多 24 KiB；设备目录响应最多 2 MiB。超限由服务明确报告容量错误，SDK 不静默截断设备或返回不完整列表。

## 完整目录、provider 策略与 watch

`directory()` 返回不可变 `DirectorySnapshot`：`version:1`、`productId`、`epoch`、`endpointPolicyVersion`、不透明 `revision`、服务原子采样 `sampledAt`、`reset`、`changed` 和完整 `devices`。设备包含原公开字段以及 `tunnelUrl`、`certificateFingerprint`、`incarnation`、`publishRevision`、`bridges`；console 投影不含 iroh、会话 hash 或私有修订历史。旧 `devices()` 继续只返回原设备/端口投影，不增加隐式权限。

`revision` 不是可比较的整数，也不是信令 cursor。它绑定已验证用户、产品、角色、epoch 和有效内容。只把本次返回的 cursor 用于相同查询，`reset=true` 时完整替换；`changed=false` 也有完整设备列表，仍需处理较新发布确认和采样期限。SDK 不缓存旧完整目录以维持 ready，完整目录获取、解析、权限或容量失败时，SDK 先同步关闭现有业务 gate，再通知状态观察者及 watch 的 `unknown`；宿主应把显示标为未知。明确的调用方取消、本地输入错误及端口选择未找到不会单独撤销仍有效的完整目录。

```ts
const watch = grid.watchDirectory(selectedCanonicalProductId, (event) => {
  if (event.type === 'unknown') renderDirectoryUnknown(event.error.code);
  else replaceDevices(event.snapshot.devices); // reset 和 snapshot 都是完整替换。
}, { intervalMs: 2000, signal: viewLifetime.signal });
// 不再使用时：watch.stop(); await watch.done;
```

每个 watch 立即发首个请求，前一请求结束后才等待下一轮，默认2秒，整数范围1..30秒。失败以 `unknown` 通知，退避最多30秒；观察者异常隔离，stop/注销取消等待并禁止迟到通知。同一 Client 最多32个活跃 watch；达到上限明确返回 `BRIDGE_CAPACITY_EXCEEDED`，不排无界队列。事件的 `eventRevision` 是本地递增通知序号，与服务的不透明 `revision` 分开。注销、epoch/会话恢复会关闭既有桥接，宿主显式重连；不会恢复旧 channel。

可信配置的 `endpointPolicy` 必须通过固定 issuer/client/resource/endpoints/scopes 校验和策略 SHA256 再使用。其 `providers` 最多16项、每项最多16个精确规范 HTTPS origin 和16个 path，整体最多32KiB；不支持通配或重定向。Quick Tunnel 是固定内置规则，不能从公网响应覆盖。新的目录/桥接要求服务显式返回该策略；旧服务缺省策略时，仅维持原 Quick Tunnel `devices()` 行为。

目录宣告策略版本变化时，SDK 先关闭依赖原策略的业务 gate，再按固定 SPA client ID 有界重取完整配置；并发 reload 共享一个请求，配置失败或与目录版本不同则拒绝，不从目录临时添加 origin。构造时固定的 OAuth 绑定不变，受信的产品/provider 展示元数据可以更新。目录最多2MiB，不截断返回。设备 HTTPS URL 保留完整 authority，首版仅根地址；端口可用 provider 精确 path；bridge 必须是显式 WSS endpoint。URL 原文必须规范，无凭据、query、fragment、编码、反斜线或 dot segment，不能依赖 URL parser 规范化后放行。

## 受信认证 adapter 与同步业务 gate

宿主明确选择支持目标协议的 `TrustedBridgeAdapter<Req, Res, StreamInput>`。SDK 不向 adapter 传 CyberGrid bearer 或 Logto refresh grant；业务 access token 的来源和刷新由宿主私有闭包拥有。首次公钥发现依赖固定协调服务及其授权目录；独立带外 pin 应由 adapter 同时验证。`installationId` 只是同 owner/product 下的关联展示值，相同 ID 配不同 key 不会合并安全身份。

公开类型的关键边界如下（完整声明以构建出的类型为准）：

```ts
interface TrustedBridgeAdapter<Req, Res, StreamInput> {
  readonly protocol: string;
  connect(target: BridgeTarget, context: {
    readonly signal: AbortSignal;
    readonly remainingMs: number;
  }): Promise<AuthenticatedBridgePeer<Req, Res, StreamInput>>;
}
// target 是深度不可变的公开身份/地址：userId、productId、deviceId、epoch、
// generation、incarnation、publishRevision、endpointPolicyVersion、descriptor。
// descriptor 包含 protocol/providerId/url/installationId/publicKey/addressRevision/
// expiresAt/limits；不含授权 bearer、业务 token 或私钥。
interface AuthenticatedBridgePeer<Req, Res, StreamInput> {
  readonly encrypted: true;
  readonly identity: { protocol: string; publicKey: string; installationId: string };
  readonly limits: BridgeLimits;
  request(input: Req, options: {signal: AbortSignal}): Promise<Res>;
  openStream(input: StreamInput, options: {signal: AbortSignal}): Promise<{
    readable: ReadableStream<Uint8Array>;
    writable: WritableStream<Uint8Array>;
  }>;
  readonly closed: Promise<void>;
  close(): Promise<void>;
}
```

adapter 在真实密码学握手中验证 descriptor 的 key、installation 和协议，再返回 peer。SDK 再比较返回的身份，要求所有实际 limits 为正安全整数且不超过 descriptor：帧≤65535字节、流≤32、请求≤128。类型或 `encrypted:true` 声明不能代替真实加密；不受信的 adapter 可以绕过自己提供的密码学，不能交给第三方不受信代码实现。

```ts
const channel = await grid.connectBridge<Request, Reply, StreamParameters>(
  productId, deviceId, bridgeId, yourVerifiedAdapter,
  { timeoutMs: 60_000, signal: connectionAttempt.signal },
);
const reply = await channel.request(request, { signal: requestLifetime.signal });
const stream = await channel.openStream(parameters);
await stream.write(bytes);
const nextBytes = await stream.read(); // Uint8Array 或 EOF 的 null。
await stream.close();
const outcome = await channel.close();
if (outcome.cleanupPending) showCleanupPending();
```

wrapper 只暴露 `request`、`openStream`、`state`、`close`，流只暴露 `read`、`write`、`close`；不返回 raw peer、WebSocket、reader/writer 或绕过 gate 的闭包。每次调用同步检查 gate，关闭同步封闭入口并中止在途调用，随后异步清理。没有业务请求重放。取消后 raw 操作未结束仍计入请求/流上限；流取消会永久关闭该流，避免未确认的读取悄悄消费字节后继续使用。同一流最多一个待完成 read 和一个 write；SDK 复制字节、限制单帧并使用背压，单次缓冲不超过1MiB。adapter 必须约束自身底层队列、帧分块、总业务大小及真实取消/关闭，泛型 request 不由 SDK 猜测序列化格式。

连接总预算默认60秒，整数范围1..120秒，涵盖目录/协调恢复/握手与握手后再次目录核对。每个 Client 最多16个连接尝试或活跃通道，未确认清理仍占槽。连接自身持有 watch，不依赖宿主另开订阅。目标离线、消失、epoch/generation/incarnation/key/address/provider/limits 变化，目录不可达或宿主明确声明业务 `denied`，均先永久封闭旧 gate。新建通道须由宿主明确请求；宿主 `authorized` 不扩大协调权限。

到期采用本地单调请求起点加服务的 `expiresAt-sampledAt`，设独立计时器，不依赖下一轮 poll 或可回拨的 wall clock。仅同绑定、有序采样与更高 publishRevision 的 expiry-only 续期能延长期限；重复快照/心跳不能延期。握手后再次核对当前完整 descriptor 和代际。取消/超时调用及时结束，迟到认证 peer 仍由 SDK 持有并关闭。清理等待最多5秒；`state()` / `close()` 仅有固定 phase/reason 和 `cleanupPending`，未确认不能冒充已释放，也不能再用该 gate。

## 外部宿主与验证边界

CyberGrid 不复制或打包 AnyWork 的 AGPL 实现，不把它作为本 SDK 的依赖。外部宿主可在自己的许可范围内通过公开接口实现 adapter。当前已核对的 AnyWork `RemoteTransport.connect` 只接受合法 Quick Tunnel HTTPS 根，并按既定协议派生同 authority 的 `/remote/v1/ws`；这样的 adapter 必须先要求 `providerId=quick-tunnel`、descriptor 为精确 WSS `/remote/v1/ws` endpoint，才能传同源 HTTPS root。不能忽略 descriptor path，也不能把通用 SDK 的 custom provider 支持说成当前 AnyWork 接口已支持。

`npm test` 验证解析、watch、gate、限额、计时和资源所有权；`npm run test:integration` 保留原实际 HTTP 兼容回归；`npm run test:bridge-integration` 需要真实 `redis-server`（可用 `REDIS_TEST_SERVER` 指定），运行构建 ESM、真实 HTTP、ES384/JWK、Redis 与发布/目录失效链。该公开 fixture 的 adapter 是明确的生命周期替身，不证明密码学互通。外部真实 Noise 互通证据在私有验收装配中固定宿主 commit、实际 source/lock/binary hashes，另行记录；本地隔离传输映射不等同真实 Quick Tunnel、公网、正式 Logto、手机浏览器或长跑完成。
