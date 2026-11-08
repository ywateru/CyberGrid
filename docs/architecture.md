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
       或 Cloudflare Quick Tunnel + 内层 mTLS
       业务内容不经过 CyberGrid 服务
```

一个部署由一个长期运行的 Node.js 进程提供完整网站和 API，并持有唯一有界 MemoryStore。API 在进程内直接调用协调服务，不进行上游 HTTP 转发，不配置 `CYBERGRID_OWNER_URL` 或 `CYBERGRID_PROXY_SECRET`。不使用 Node cluster、多副本或 serverless Function 分散协调状态。保留已有 POSIX flock 单进程租约，强杀后由内核释放；该锁不存储协调数据。

`CYBERGRID_PUBLIC_ORIGIN` 是可选的对外 HTTPS origin。配置后固定使用它，不让请求头覆盖；留空时从本次请求的合法 Host 和 HTTP/TLS 连接识别访问地址，也支持严格的单值 `X-Forwarded-Host` / `X-Forwarded-Proto` 供 HTTPS 反向代理声明外部地址。代理必须覆盖这两个头，不追加或透传客户端自带值；多值、非法 authority、userinfo 或非 HTTP(S) scheme 均拒绝。派生地址仅用于本请求 URL、同源检查及 OAuth 回跳，不存入全局配置、不采用首请求绑定，也不触发任何向该地址的服务端网络请求。客户端 IP 始终取连接的 socket 地址，忽略 Forwarded 和 X-Forwarded-For，不能用转发头绕过限流。外部 HTTPS 代理是可选基础设施，Node.js 服务本身不依赖特定平台；Docker 使用同一服务入口，单副本运行。

JWT 验签的 Logto issuer、JWKS 和 API resource 始终由启动配置固定，与请求地址无关。API resource 优先使用 `LOGTO_RESOURCE`；若未指定且配置了公开 origin，兼容默认值为该 origin 加 `/api`；两者均未指定时使用固定标识 `https://cybergrid.local/api`。该标识只用于 OAuth audience，不需要域名解析或实际 HTTP 服务，须在 Logto 创建相同 API resource。已有部署会显式保存原 API resource，因此删除公开 origin 不会改变其验签范围。默认回跳按本次有效访问地址加 `/console/callback/` 生成，始终返回绝对 URI；显式回跳须与本次有效 origin 一致并在 Logto 登记，禁止保留路径。不同 Host 请求不会改变其他请求的回跳或 JWT audience。

网站、文档、个人控制台和管理页面都由该 Node 进程提供，并设置精确放行可信 Logto origin 的 CSP。默认与自定义控制台回调路径均落到本进程的控制台页面，保留 OAuth 查询参数。静态文件仅从公开资源目录读取，拒绝路径穿越；健康检查只反映进程存活，不向 Logto发请求或暴露配置、密钥和协调内容。关闭时停止接受新请求、终止协调等待者并释放租约。

HTTP 适配器依据实际请求字节判断 GET / HEAD 是否携带请求体。反向代理使用合法的空分块传输时，应在同一请求总预算、容量上限与断连取消机制下读完并确认零字节，不能仅因 `Transfer-Encoding: chunked` 就拒绝无请求体的访问。实际非空 GET / HEAD 请求体仍在调用业务 handler 前拒绝；传输声明超出容量或实际读取超限仍按原容量边界拒绝，不放宽 HTTP 解析器对冲突或非法 framing 的校验。

Logto 管理用户与应用。SDK 的 AppID 是启用 Device Flow 的 Native 应用 `client_id`；通过 JWT 验签后的 subject 与 client_id 划分命名空间。设备只能发现和连接同账号、同应用的终端。CyberGrid 不提供应用 CRUD。浏览器个人控制台使用独立 SPA PKCE 身份，只读设备信息，不注册设备。

服务内存保存有界的会话 hash、设备心跳、端口、证书指纹和短期信令。它不保存用户密码、refresh token、端到端私钥或业务 payload。设备内存元数据会过期，服务重启清空所有协调状态并生成新 epoch。

SDK 检测协调状态丢失后，以本机已保存的授权重新注册、重发端口快照和重建通道。本地映射监听与业务意图可保留；旧 TCP 流可能中断，不缓冲或自动重放业务字节。业务同步需要自己的消息确认和去重策略。

Cloudflare 终止外层 HTTPS，设备之间的内层 TLS 1.3 双向认证按受信设备指纹校验。中央仍是身份信任根；不承诺抵御恶意中央管理员。Quick Tunnel 是开发测试级服务，不能把它视为可用性保证。

管理后台有独立的 `.env` 密钥。它只能读取安全投影后的协调状态，不返回会话密钥、完整信令、证书、回退URL或业务内容；心跳在线不等同于端到端业务连接成功。
