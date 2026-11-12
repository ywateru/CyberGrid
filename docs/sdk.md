# SDK 与平台接入

| 平台 | Go SDK／CLI | Rust SDK／Lab |
| --- | --- | --- |
| macOS arm64 | 原生CLI，预置Cloudflared | 原生SDK及Tauri2 App，预置Cloudflared |
| Android arm64 API28+ | 原生CLI，宿主配置provider执行位置 | SDK离线资源、Tauri APK安装原生provider、私有ADB控制 |

项目 v0.7.0 新增 [JS SDK](../sdk-js/README.md)，供已使用 Logto 登录的前端只读查看本账号的应用、设备、端口及端口主动申报的 Cloudflare 地址，不注册设备或建立传输通道。

后续 SDK 功能以 Rust 为主要实现；Go SDK 保持兼容。完整API示例以 [Rust README](../sdk-rust/README.md)、[Lab README](../apps/tauri-test/README.md) 和 [Go README](../sdk/README.md) 为准。下载位于 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases)，不要把网站HTML下载当作SDK压缩包。

本次集成改进的 Rust 源码默认关闭远程入站：登录后 `expose()` 只记录本地端口意图，宿主明确调用 `enable_remote()` 后才发布并开放。`disable_remote()` 先关闭本机入口和远程流，再返回当前协调域的撤销确认/待确认；本地业务服务由宿主独立管理。重新启用需新建 Client 并沿同一授权 owner 恢复意图，不能复活已经关闭的 Client。它是行为变更，现有已发布 Lab/SDK 下载包不会自动获得或适配这些接口；[Lab 开关集成](lab-remote-controls.md) 单独验收。

Rust 的 `login_with_auth_provider()` 支持宿主内存 token owner，不必创建 CredentialStore；`access_token_for()` 请求最短有效期、刷新原因与受限强制刷新。JS 回调提供相同上下文。未知交换保持原私有 token/幂等操作；只有明确身份拒绝且此前没有未知结果时才允许一次强制刷新，或经完整可信配置证明协调 epoch 已改变后进入新域。Rust `watch_state()` 和 JS `snapshot()/subscribe()` 分开身份、协调、发布、可达性及宿主业务授权，不将 online 当成业务允许。

部署者可用 [静态产品策略](integration-contract.md) 显式绑定同产品的 SPA/Native 集合，无需运行期 Management 凭据。当前浏览器业务桥接、finite-v2 准入和共同样例的交付状态以 [改进覆盖表](plan/analysis/integration-progress.md) 为准；接口设计不代表现网已支持。元数据、兼容矩阵和分发授权边界见 [元数据与兼容性](metadata-and-compatibility.md)。

接入需要中央HTTPS地址、关闭 Device Flow 的普通 Logto Native client_id、设备名称，以及应用自己的loopback TCP服务。SDK完成托管登录后取得中央短会话，公开端口声明包含端口号与应用定义的type字符串；发现接口返回同账号、同应用的其他设备及端口；映射接口把选定远端端口暴露为本机loopback监听。

Lab 0.3.0 统一使用官方 SDK 网页登录，预置公开测试服务和普通 Native 应用 ID。Mac 使用官方 Logto JavaScript SDK，Android 使用官方 Android SDK；授权交接给 Rust 后，由同一个 CredentialStore/AuthProvider 恢复和刷新。Device Flow 入口已移除；历史 SDK API 在服务器声明 `loginFlow: "authorization_code"` 时明确拒绝设备码请求。平台回调和令牌边界见 [Lab 网页登录](lab-browser-login.md)。新服务和客户端需配套更新，现有旧版本下载包不会自动更新。

Rust 当前源码默认依次尝试 WebRTC、iroh QUIC 直连、Cloudflare Quick Tunnel，也可配置 WebRTC/iroh 并行直连，以先完成内层双向认证的一方为准；败方清理完成后才返回结果，两方都失败才进入匿名隧道。iroh 不使用 relay 或默认第三方地址发现；三条路径均保留端到端 mTLS 和当前设备指纹校验。旧对端缺少 iroh 元数据时自动跳过中间阶段。指定网络接口时若 iroh 无法提供同等约束，则跳过 iroh，不能绕过接口选择。已发布 v0.6.0 和 Go SDK 仍使用原有 WebRTC → Quick Tunnel 两级策略；这些新功能随项目 v0.7.0 提供。

Rust 端口声明可附带独立的 `tunnel_url`，也可在保留端口声明的同时更新或撤回该地址。宿主负责创建对应业务端口的 Quick Tunnel；该地址与 SDK 内部的设备级加密 WebSocket 回退地址用途不同。JS SDK 仅返回端口显式申报的地址，不把设备级 URL 当作 HTTP API 地址，也不自动访问业务端口。

两语言SDK均自带固定 Cloudflared 校验资源，不隐式搜索PATH或在运行时下载执行文件。macOS采用校验后的私有缓存提取；Android应随宿主APK安装provider至原生库目录，按 [Lab集成](../apps/tauri-test/README.md#android-provider) 提供实际DNS与TMPDIR回调。

授权文件与服务器／应用／scope／可信端点绑定。应用应显式设置私有凭据存储与AuthProvider，让refresh轮换在后续调用前checkpoint；磁盘失败时保留最新私有材料供重试或撤销，不用旧token继续，也不复制其他设备授权。

Node 重启或 Function 冷启动时 Redis 状态保留；Redis namespace 被重置或租约失效后，SDK重新注册、重发完整端口快照，原映射监听保留意图并等待当前代际的新连接。关闭操作取消该意图并停止provider／监听；中断的业务流不自动重放。业务消息应使用自己的ack、去重和同步机制。

Android测试使用仓库的 [android-adb.py](../apps/tauri-test/tools/android-adb.py)，通过固定App私有命令队列调用与GUI相同的真实SDK逻辑。它不接受任意shell、凭据、证书或文件路径；登录受理、托管授权完成、SDK Ready和端到端数据确认是不同状态。发生超时或结果未知时先查status，不盲目重放业务命令。
