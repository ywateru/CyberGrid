# 架构与数据边界

```text
浏览器 / SDK
     │ 身份与授权                 Logto
     │                           用户 / 应用 / JWT / refresh
     │ 同源 API
可选的 HTTPS 反向代理 / 负载入口
     │ HTTP（可信私网或本机 loopback）
单一 CyberGrid Node.js 服务（原生运行或 Docker）
     ├ 网站 / 文档 / 管理后台 / Logto 个人控制台
     ├ /api/v1/* 直接调用本进程协调逻辑
     │ 有界 RAM：短会话 / 设备 / 端口 / 信令 / TTL / 限流
     └ 不写数据库或磁盘快照

设备 A ── WebRTC + mTLS ──────────── 设备 B
       → iroh QUIC 直连 + 内层 mTLS
       → Cloudflare Quick Tunnel + 内层 mTLS
       业务内容不经过 CyberGrid 服务
```

一个部署由一个长期运行的 Node.js 进程提供完整网站和 API，并持有唯一有界 MemoryStore。API 在进程内直接调用协调服务，不进行上游 HTTP 转发，不配置 `CYBERGRID_OWNER_URL` 或 `CYBERGRID_PROXY_SECRET`。不使用 Node cluster、多副本或 serverless Function 分散协调状态。保留已有 POSIX flock 单进程租约，强杀后由内核释放；该锁不存储协调数据。

`CYBERGRID_PUBLIC_ORIGIN` 是可选的对外 HTTPS origin。配置后固定使用它，不让请求头覆盖；留空时从本次请求的合法 Host 和 HTTP/TLS 连接识别访问地址，也支持严格的单值 `X-Forwarded-Host` / `X-Forwarded-Proto` 供 HTTPS 反向代理声明外部地址。代理必须覆盖这两个头，不追加或透传客户端自带值；多值、非法 authority、userinfo 或非 HTTP(S) scheme 均拒绝。派生地址仅用于本请求 URL、同源检查及 OAuth 回跳，不存入全局配置、不采用首请求绑定，也不触发任何向该地址的服务端网络请求。客户端 IP 始终取连接的 socket 地址，忽略 Forwarded 和 X-Forwarded-For，不能用转发头绕过限流。外部 HTTPS 代理是可选基础设施，Node.js 服务本身不依赖特定平台；Docker 使用同一服务入口，单副本运行。

JWT 验签的 Logto issuer、JWKS 和 API resource 始终由启动配置固定，与请求地址无关。API resource 优先使用 `LOGTO_RESOURCE`；若未指定且配置了公开 origin，兼容默认值为该 origin 加 `/api`；两者均未指定时使用固定标识 `https://cybergrid.local/api`。该标识只用于 OAuth audience，不需要域名解析或实际 HTTP 服务，须在 Logto 创建相同 API resource。已有部署会显式保存原 API resource，因此删除公开 origin 不会改变其验签范围。默认回跳按本次有效访问地址加 `/console/callback/` 生成，始终返回绝对 URI；显式回跳须与本次有效 origin 一致并在 Logto 登记，禁止保留路径。不同 Host 请求不会改变其他请求的回跳或 JWT audience。

网站、文档、个人控制台和管理页面都由该 Node 进程提供，并设置精确放行可信 Logto origin 的 CSP。默认与自定义控制台回调路径均落到本进程的控制台页面，保留 OAuth 查询参数。静态文件仅从公开资源目录读取，拒绝路径穿越；健康检查只反映进程存活，不向 Logto发请求或暴露配置、密钥和协调内容。关闭时停止接受新请求、终止协调等待者并释放租约。

HTTP 适配器依据实际请求字节判断 GET / HEAD 是否携带请求体。反向代理使用合法的空分块传输时，应在同一请求总预算、容量上限与断连取消机制下读完并确认零字节，不能仅因 `Transfer-Encoding: chunked` 就拒绝无请求体的访问。实际非空 GET / HEAD 请求体仍在调用业务 handler 前拒绝；传输声明超出容量或实际读取超限仍按原容量边界拒绝，不放宽 HTTP 解析器对冲突或非法 framing 的校验。

Logto 管理用户与应用。SDK 的 AppID 是关闭 Device Flow 的普通 Native 应用 `client_id`；通过 JWT 验签后的 subject 与 client_id 划分命名空间。设备只能发现和连接同账号、同应用的终端。CyberGrid 不提供应用 CRUD。浏览器个人控制台使用独立 SPA PKCE 身份，只读设备信息，不注册设备。

Lab 测试客户端统一使用官方 Logto SDK 网页登录，移除设备码入口。Mac 和 Android 使用同一个普通 Native 应用，不改变服务端应用隔离规则；启用 Device Flow 的 Native 应用明确拒绝。平台接入、预置公开配置及凭据交接契约见 [Lab 网页登录](lab-browser-login.md)。

服务内存保存有界的会话 hash、设备心跳、端口、证书指纹和短期信令。它不保存用户密码、refresh token、端到端私钥或业务 payload。设备内存元数据会过期，服务重启清空所有协调状态并生成新 epoch。

SDK 检测协调状态丢失后，以本机已保存的授权重新注册、重发端口快照和重建通道。本地映射监听与业务意图可保留；旧 TCP 流可能中断，不缓冲或自动重放业务字节。业务同步需要自己的消息确认和去重策略。

## Rust 主 SDK 的连接顺序

后续 SDK 功能以 Rust 为主要实现，Tauri 2 测试应用直接使用 Rust SDK。项目 v0.7.0 新增的连接顺序为 WebRTC → iroh 直连 → Cloudflare Quick Tunnel；已发布的 v0.6.0 下载包仍使用 WebRTC → Quick Tunnel，升级源码不等于已经更新发布包。Go SDK 保留兼容接口与原有两级连接，不通过 Go FFI 实现 Rust 传输。

直连策略可配置为串行或并行，默认保持串行。串行按 WebRTC → iroh 顺序使用各自独立预算；并行同时启动两种直连，只有完成内层双向认证的连接可以获胜。获胜后取消并等待败方清理，禁止遗留晚到的 peer、QUIC 连接或 WebRTC attempt。并行模式的直连等待预算为两种预算的最大值，而非相加。两种直连都失败后才进入原有匿名隧道预算。对端没有 iroh 元数据、显式禁用或无法遵守指定网络接口约束时跳过 iroh。三层均受本次运行代际和取消令牌约束，映射重新连接的总预算依据策略计算；不因降级重放已经发送的业务字节。

iroh 端点只使用显式发布的 IP/端口和端点公钥，禁用 iroh relay 和默认的第三方地址发现，不允许应用数据经 iroh 中继。禁用 relay 也限制其跨 NAT 协调能力：新增 iroh 层不等于保证 Wi-Fi/4G 或任意 NAT 网络可直连。每个运行代际生成临时端点身份，关闭、租约失效和重新注册时销毁旧端点及连接。iroh 的 QUIC 身份不能代替 CyberGrid 用户和应用认证；其字节流继续经过与其他传输相同的设备指纹校验、TLS 1.3 双向认证和端口复用。

设备快照可包含可选的 `iroh` 协调元数据：由 64 个小写十六进制字符组成的 `endpointId` 和最多 8 个数字 IP/端口候选 `addresses`，整体最多 1024 UTF-8 字节。不接受 DNS 名称、URL、relay 地址或任意扩展字段；缺省或 `null` 表示不支持。该元数据与端口、证书指纹一起受完整快照 revision、incarnation 和 lease 规则约束，只向同用户、同应用的已授权设备公开。中央仅存储并分发这些有界协调信息，不拨号、不探测候选地址、不转发业务数据；管理后台继续使用安全投影，不显示候选 IP 或端点公钥。

## 端口地址与只读 JS SDK

端口声明可附带可选的 `tunnelUrl`，由宿主主动申报、更新或撤回。它是该业务端口自己的 Cloudflare Quick Tunnel HTTPS 根地址，不是设备级的 SDK 加密 WebSocket 回退地址。中央只校验并保存 URL，不替宿主启动隧道、不主动访问地址、不拼接任意代理路径。未申报时保持空；JS SDK 不得用设备级地址冒充端口业务 URL。端口地址与端口完整快照一起参与 revision、lease 和恢复发布，旧客户端省略字段仍可使用。

每台设备的完整发布元数据按实际 JSON UTF-8 字节限制为 24 KiB，包含端口地址和 iroh 信息；超限返回 `DEVICE_METADATA_TOO_LARGE`，不得修改 revision、心跳或已有快照。目录响应另有 2 MiB 上限，超出时明确返回 `DIRECTORY_TOO_LARGE`，不截断设备数据，也不把容量问题伪装成上游不可用。字段的字符数限制不代替实际序列化字节限制。

JS SDK 是浏览器的只读协调客户端，不实现 WebRTC、iroh、端口映射、设备注册或信令发送。宿主使用官方 Logto JS/React SDK 登录，向它提供取得固定 API resource access token 的回调。JS SDK 在请求 token 前核对可信 issuer、SPA client ID 和 resource，再交换独立 `purpose: console` 会话。用户只能读取自己的应用目录和所选应用下的设备、端口及主动申报的端口 URL；不能写设备信息或跨用户访问。Logto 登录及 refresh 所有权始终留在宿主 SDK，CyberGrid console bearer 仅保存在内存。退出只注销本 console 会话，不注销其他终端或代替宿主退出 Logto。

JS SDK 使用 `#private` 字段隐藏 bearer，普通序列化和对象枚举不包含凭据。恢复交换必须保持原用户 ID，身份改变返回 `IDENTITY_CHANGED`，由宿主显式退出后重新连接。结果未知的交换复用原 epoch/request ID；每次未知 HTTP 尝试分别计数，与已取得但未确认注销的会话合计最多 8 项，达到上限后拒绝新的交换并返回 `SESSION_UNAVAILABLE`，避免取消或退出循环无限增发服务端会话。迟到响应不能恢复已退出的本地状态。

同源部署无需额外配置。第三方前端需显式加入 `CYBERGRID_BROWSER_ORIGINS` 的精确 origin 白名单，只对 console 配置/交换、应用/设备只读查询及 console 专用注销接口开放 CORS。设备、信令和管理接口继续拒绝跨站访问；无通配 origin、无 cookie 凭据模式。CORS 不代替 JWT 与会话授权。业务端口若由前端直接访问，其鉴权、内容类型和 CORS 由业务服务负责；公开隧道 URL 不是访问凭据，JS SDK 仅返回地址，不自动请求业务数据。

Cloudflare 终止外层 HTTPS，设备之间的 SDK 内层 TLS 1.3 双向认证按受信设备指纹校验。中央仍是身份信任根；不承诺抵御恶意中央管理员。Quick Tunnel 是开发测试级服务，不能把它视为可用性保证。

管理后台有独立的 `.env` 密钥。它只能读取安全投影后的协调状态，不返回会话密钥、完整信令、证书、回退URL或业务内容；心跳在线不等同于端到端业务连接成功。
