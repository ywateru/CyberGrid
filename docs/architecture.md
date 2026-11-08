# 架构与数据边界

```text
浏览器 / SDK
     │ 身份与授权                 Logto
     │                           用户 / 应用 / JWT / refresh
     │ 同源 API
Vercel 静态网站 / 无状态固定代理
     │ HTTPS + 私密 proxy key
单一 Node owner
     │ 有界 RAM：短会话 / 设备 / 端口 / 信令 / TTL / 限流
     └ 不写数据库或磁盘快照

设备 A ── WebRTC + mTLS ──────────── 设备 B
       或 Cloudflare Quick Tunnel + 内层 mTLS
       业务内容不经过 CyberGrid owner
```

一个部署只能由一个 owner 接受同一组客户端的协调调用。Vercel Function 的实例不共享内存，因此 Vercel 只代理到固定 owner；上游不可用时明确返回 503，不创建临时本地副本。owner 使用 POSIX flock 保护唯一进程，强杀后由内核释放租约。

Logto 管理用户与应用。SDK 的 AppID 是启用 Device Flow 的 Native 应用 `client_id`；通过 JWT 验签后的 subject 与 client_id 划分命名空间。设备只能发现和连接同账号、同应用的终端。CyberGrid 不提供应用 CRUD。浏览器个人控制台使用独立 SPA PKCE 身份，只读设备信息，不注册设备。

owner 内存保存有界的会话 hash、设备心跳、端口、证书指纹和短期信令。它不保存用户密码、refresh token、端到端私钥或业务 payload。设备内存元数据会过期，owner 重启清空所有协调状态并生成新 epoch。

SDK 检测协调状态丢失后，以本机已保存的授权重新注册、重发端口快照和重建通道。本地映射监听与业务意图可保留；旧 TCP 流可能中断，不缓冲或自动重放业务字节。业务同步需要自己的消息确认和去重策略。

Cloudflare 终止外层 HTTPS，设备之间的内层 TLS 1.3 双向认证按受信设备指纹校验。中央仍是身份信任根；不承诺抵御恶意中央管理员。Quick Tunnel 是开发测试级服务，不能把它视为可用性保证。

管理后台有独立的 `.env` 密钥。它只能读取安全投影后的协调状态，不返回会话密钥、完整信令、证书、回退URL或业务内容；心跳在线不等同于端到端业务连接成功。
