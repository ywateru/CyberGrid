# CyberGrid Go SDK 与原生测试程序

Go SDK 0.4.0 支持 Logto 登录、同账号同应用设备发现、TCP 端口映射和 memory-v1 自动恢复。源码与平台产物发布到 [GitHub](https://github.com/ywateru/CyberGrid/releases)，第三方许可与离线 Cloudflared 资源随包提供。

## memory-v1 与恢复

当前服务端使用 Redis 保存跨实例共享协调状态，支持 Node / Docker 和 Vercel Functions。Node 重启或 Function 冷启动不更换 epoch。SDK协商 `memory-v1`，以私密 registrationKey/revision/requestId 保持设备逻辑ID，校验 epoch/incarnation/publishRevision，长轮询信令最多15秒。Redis namespace 重置、中央会话或65秒lease到期后关闭旧流；显式配置 `Config.AuthProvider` 或 `SetAuthProvider` 才能自动恢复。没有 provider 会发送 `requires-login`，不会偷偷重用旧 access token 或弹出授权页面。

`AuthProvider.AccessToken(context.Context,*Client) (OAuthTokens,error)` 由宿主私有凭据所有者实现，返回前必须保存最新 refresh rotation。注册恢复 single-flight，暂时故障指数退避和 jitter，并尊重有界 Retry-After；身份变化、应用拒绝、会话被接管则停止。完整恢复后重新 Expose、本代证书和 provider URL；同一个 Map 保留 loopback listener，恢复期间新连接被拒绝，Ready 后新 TCP 使用当前 peer 重新握手。已经断开的 TCP 不重放字节，不承诺无损续传；`Mapping.Transport/Peer` 是创建时的诊断快照，后续状态请读事件。

安全事件包括 `coordination-ready`、`recovering`、`requires-login`、`mapping-disconnected`、`mapping-ready`、`provider-restarting`、`provider-failed`、`closed`，带公开 generation/epoch/reason。宿主应据此更新状态并由业务协议决定重试，不能以设备名字重新认领对端。

SDK在首次 device grant 后、resource交换前立即调用 `SetGrantCheckpoint` 的私有回调；之后每次rotation同样checkpoint。回调成功意味着宿主已持久保存或接管最新grant，不能记录token或仅假装保存。回调失败会留下私有pending，可用 `RetryPendingGrant` 或 `RevokePendingGrant` 解决；`HasPendingGrant` 只暴露布尔状态。没有hook的显式宿主必须保存返回值后调用 `AcknowledgePendingGrant`，禁止携旧refresh继续轮换。CLI额外持有文件OS锁，写盘失败时lease保留到保存或撤销完成，其他进程有界等待；异常退出会继续保留pending任务重试。进程崩溃在provider签发与本机保存之间仍有不可原子窗口。

持久凭据升级为 **version4**：固定 server/appId/issuer/clientId/resource/scope/deviceAuthorizationEndpoint/tokenEndpoint/revocationEndpoint，刷新前跨Client、跨进程逐项核对。动态coordination不属于固定绑定。旧v3/v2文件在网络刷新与覆盖前拒绝，原字节保留；请使用新的独立 `*-v4.json` 文件重新授权。旧grant可用旧版按原配置撤销或由Logto维护者显式撤销，本轮不扫描或导入旧文件。CLI不提供隐式默认凭据路径。

## 下载包快速使用

官网与下载入口：https://cybergrid.aixgg.com/ 。完整接入说明：https://cybergrid.aixgg.com/docs/ 。本 README 随源码包和两种原生 CLI 包一起提供，请按你解压的包选择命令。

- macOS arm64：进入 `cybergrid-sdk-0.4.0-macos-arm64/` 解压目录，运行 `./cybergrid-macos version`；下文 Mac CLI 命令也从该目录运行。
- Android arm64：进入 `cybergrid-sdk-0.4.0-android-arm64/` 解压目录，设置目标设备的 `ANDROID_SERIAL`，运行 `adb -s "$ANDROID_SERIAL" push ./cybergrid-android-arm64 /data/local/tmp/cybergrid`，随后按 Android 说明授权并启动。
- Go 源码：进入 `cybergrid-sdk-0.4.0/` 解压目录后，当前目录已是 module 根目录，无需再 `cd sdk`，可直接 `go build ./...`、`go test ./...` 或 `go build ./examples/embed`。源码构建的 CLI 在 `bin/`，可进入该目录运行下文的 `./cybergrid-macos` 命令。

SDK 已预置固定 cloudflared：macOS arm64 CLI 默认内置提取，无需外装；Android arm64 的离线 jniLibs 资源与打包脚本随 SDK 交付，宿主安装后由 NativeLibraryDir 使用。原生 CLI 包不包含 Go 构建环境。详见 [provider 集成说明](provider-assets/README.md)。

Go SDK 通过 Logto Cloud 托管注册／登录，当前服务使用关闭 Device Flow 的普通 Native 应用，由宿主官方 Logto SDK 完成 Authorization Code + PKCE。历史设备码 API 仅为旧服务保留，新服务会明确拒绝设备码请求。SDK 用 API resource access token 向中央交换 app/device 绑定的 CyberGrid session bearer；后续协调 API 仅使用该会话。中央和 SDK 均不接收账号密码或 Logto 管理令牌。中央不接收业务字节。第一版提供 **TCP** 端口映射，`Port.Type` 为应用自定义字符串，`Transport` 必须是 `tcp`。

CLI 默认中央地址为 `https://cybergrid.aixgg.com`，自建部署请显式配置 `-server` 或 `CYBERGRID_SERVER`。应用只来自 Logto Native Applications；配置说明见仓库的部署文档。

本模块提供 Go 库、嵌入示例和 macOS／Android arm64 CLI。图形化测试与 APK 由同仓库 Tauri Lab 提供，其 Android 测试使用私有 ADB 命令工具。Go 集成到正式 Android App 需要宿主 JNI/gomobile 与网络生命周期处理。

## 使用 SDK

SDK 的 AppID 就是 Logto Console 中 Native Application 的公开 ID／OAuth `client_id`，长度 1..21，保留原大小写。由租户维护者在 Logto 创建普通 Native 应用并关闭 Device Flow，再从该应用复制 ID；不能填应用名称、旧业务 slug 或控制台 SPA 的 ID。API resource 需配置 `connect` 用户权限。CyberGrid 没有独立应用表、预制 ID、别名转换或管理员 CRUD。

将真实 Native client_id 配置为进程环境 `CYBERGRID_APP_ID`，或显式传入 Config.AppID。它是公开标识，不是管理密钥。

`AuthConfig` 请求 `GET /api/v1/auth/config?appId=ID` 并核对返回 `clientId == AppID`；交换响应必须同时满足 `purpose=device`、`appId == AppID`。未知应用、非 Native 或启用 Device Flow 明确失败，不会换用其他应用。设备发现和业务连接始终限制在同用户、同真实 Logto Application ID 下。

```go
client, err := cybergrid.New(cybergrid.Config{
    BaseURL: "https://cybergrid.aixgg.com",
    AppID: os.Getenv("CYBERGRID_APP_ID"), // 真实 Logto Native client_id，区分大小写
    OnError: func(err error) { log.Print(err) },
})
if err != nil { return err }
defer client.Close()

// resourceAccessToken comes from the host's official Logto browser SDK.
// Request the pinned API resource and connect scope; never log the token.
// The host retains the sole refresh owner and provides AuthProvider recovery.
session, err := client.LoginWithAccessToken(ctx, resourceAccessToken, "my-device")
if err != nil { return err }
_ = session.DeviceID // 不记录 session.SessionKey

// 登录后只读列出同租户的普通 Native 应用，不代表用户授权清单。
apps, err := client.Apps(ctx)
if err != nil { return err }
for _, app := range apps { fmt.Println(app.ID, app.Name) }

err = client.Expose(ctx, cybergrid.Port{
    ID: "api", Port: 8080, Type: "my-json-api", Transport: "tcp",
})
if err != nil { return err }

// 同账号、同应用下的其他设备和它们声明的端口。
devices, err := client.Devices(ctx)
if err != nil { return err }
for _, device := range devices {
    if device.ID == session.DeviceID || !device.Online { continue }
    for _, port := range device.Ports {
        mapping, err := client.Map(ctx, device.ID, port.ID, "127.0.0.1:0")
        if err != nil { return err }
        // 通过 mapping.Addr 访问远端端口；用毕 mapping.Close()。
        fmt.Println(mapping.Addr, mapping.Transport)
        mapping.Close()
    }
}
```

源码包中的 `examples/embed` 初次设备码授权仅适用于旧服务；接入当前服务须替换为上文宿主网页登录入口。可先构建示例：

```sh
tar -xzf cybergrid-sdk-0.4.0-source.tar.gz
cd cybergrid-sdk-0.4.0
go build ./examples/embed
```

当前网页登录策略请使用项目 v0.6.0 的 Go 源码包。在自己的项目中引用解压后的模块（将路径替换为实际位置）：

```sh
go mod edit -require=github.com/ywateru/CyberGrid/sdk@v0.0.0
go mod edit -replace=github.com/ywateru/CyberGrid/sdk=/absolute/path/cybergrid-sdk-0.4.0
go mod tidy
```

历史子模块 tag `sdk/v0.4.0` 对应较早的设备码实现，不包含本次网页登录适配。完整示例见 [examples/embed/main.go](examples/embed/main.go)，其中安装 GrantCheckpoint 和 AuthProvider，保存私有 grant 并在退出时完成撤销；初次授权入口需按上文接入宿主网页登录。上面的片段仅说明已配置客户端的操作顺序，不能跳过凭据 checkpoint。

完整嵌入入口在源码包 `examples/embed/main.go`。远端 Expose 只连接远端的 `127.0.0.1:Port`；本地 Map 只允许监听 loopback，防止意外向局域网开放。`Map` 创建映射前会实际打开一次远端端口验证可用性。映射端口可为 0，由系统分配。`Close()` 会关闭设备 origin、隧道进程、peer、映射、流，并注销会话；客户端关闭后不可重新登录，需 `New`。传入 Map 的 context 取消会关闭映射。本次中央会话撤销、会话过期或心跳 lease 超时同样关闭现有数据路径。Logto 删除应用或将其改为不受支持类型后，服务端最迟在 30 秒目录缓存失效后拒绝后续协调调用，SDK 再按心跳／错误／lease 生命周期关闭连接；不承诺瞬时切断业务字节。

## 连接、安全和限制

1. 先尝试 Pion WebRTC DataChannel。启用 trickle ICE、多个可配置 STUN、IPv4/IPv6、可靠有序 SCTP；禁止配置 TURN。默认直连预算 12 秒，回退预算另有 35 秒，包含等待远端 provider 发布 URL（provider 启动最多 30 秒）及鉴权；回退前重新读取在线状态和 fingerprint，避免旧元数据。可用 `Config.FallbackTimeout` / CLI `-fallback-timeout` 调整。`InterfaceName` 限制 ICE 接口；macOS 同时对 SDK 自己的 UDP socket 使用 `IP_BOUND_IF` / `IPV6_BOUND_IF`，不改系统网络配置。Android 当前依据候选源地址绑定，不调用 App 层 `Network.bindSocket`。
2. 直连失败后，使用远端已发布的 Cloudflare Quick Tunnel URL，建立 WebSocket。只有 root `https://<name>.trycloudflare.com` 可被 SDK 接受。中央会话密钥不会发送到 Cloudflare。
3. **两种路径内部都运行 TLS 1.3 双向认证**。临时证书 SHA256 由同账号同应用的授权设备元数据发布并双向核验；私钥仅在内存中。直连另有 WebRTC DTLS；回退中 Cloudflare 终止的外层 HTTPS 无法读取内层 TLS 业务字节。中央是身份信任根；恶意中央管理员不在此模型防护范围内。
4. TLS 上使用 smux 多路复用。每条 TCP 流具有独立的目标端口 ID 和 FIN 帧，保留 TCP `CloseWrite` 后读取响应的行为。端口必须由远端显式 Expose，不能请求任意 IP/端口。

Quick Tunnel 适合测试，无可用性保证；参见 [Cloudflare 官方说明](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/)。SDK 会启动 `cloudflared tunnel --config <临时空配置> --no-autoupdate --url http://127.0.0.1:<SDK-origin> --protocol http2 --metrics 127.0.0.1:0`，不使用或修改用户命名隧道配置，不改变用户 HOME。子进程清空环境后只传非秘密白名单；每次启动通过 EnvProvider 重读当前Android DNS/TMPDIR，不继承账号／会话或进程代理变量。日志只提取随机域名，不记录 provider 原始输出。provider 退出时清除已发布 URL 并报告异步错误。

provider 只从独立完整 URL 行或 cloudflared 的分配 banner 提取地址，拒绝错误日志中 `https://api.trycloudflare.com/tunnel` 的部分匹配。只有同时取得分配地址并观察到 `Registered tunnel connection` 后才发布设备的 tunnel URL；30 秒启动总预算包含两步，超时／提前退出错误会明确指出哪一步尚未完成。Cloudflare 的公开地址传播仍可能存在短暂延迟，Quick Tunnel 无 SLA。

默认 `Cloudflared{}` 在 macOS arm64 使用 SDK 内置固定资源，不查询 PATH，不运行时下载。`Cloudflared{CacheDir: absoluteCache}` 可指定 cache base；专用目录0700、资源0700和固定inode锁0600，逐次校验并原子提取。Android宿主先运行随SDK的 `python3 scripts/prepare-android-provider.py /absolute/host/jniLibs`，以Gradle native包装安装唯一 `libcloudflared.so`，再传 `Cloudflared{NativeLibraryDir: nativeDir, TempDir: appCache, EnvProvider: readCurrentDNS}`。SDK不在Android可写files/cache解压执行。显式 `Cloudflared{Binary: path}`／CLI `-cloudflared path` 是调用方信任的external override，不具内置hash/安装路径保证。完整来源、离线打包与平台约束见 [provider-assets/README.md](provider-assets/README.md)。这里的增强仅指上述标准 ICE 工程配置，不宣称提高过真实网络打洞成功率；未实现 ICE restart 或应用业务数据自动重试。SDK 暂无 UDP 映射、自研 NAT 端口预测、自动 TCP 重连/数据重放、移动端后台保活或无损路径切换。

`OnError` 和可选 `OnTransportEvent` 是同步短回调，需迅速返回；不要在回调里直接调用 `Close()`（可在另一个 goroutine 中关闭），以免等待当前后台任务。网络中断最长 65 秒 lease 后停止；同一会话的 `expiresAt` 为绝对到期时间。业务应用自行决定重试和恢复数据，不由中央缓存。

## 构建与验证

```sh
# 在源码包 module 根目录运行
go test ./...
go test -race ./...
GO=go sh scripts/build.sh
```

输出 `bin/cybergrid-macos` 和 `bin/cybergrid-android-arm64`。Go 1.27 已构建，Android 默认使用 NDK API28 clang、`CGO_ENABLED=1 GOOS=android GOARCH=arm64`，通过 bionic `getaddrinfo` 解析域名。可用 `CYBERGRID_ANDROID_NDK` 指定 NDK 路径；macOS 默认发现 `~/Library/Android/sdk/ndk/`。纯 Go Android 构建已在真机确认把域名查询发向 `[::1]:53` 并失败，因此不能用于隧道域名，不作为默认产物。Pion 间接依赖 `github.com/wlynxg/anet v0.0.5`，其 Android 接口发现依赖 Go 内部符号，按 [依赖说明](https://github.com/wlynxg/anet#compatibility) 在 Android 构建使用 `-ldflags=-checklinkname=0`；需持续复测 Go 升级兼容性。Mac 不需该 linker flag。

自动网络测试启动独立内存测试控制面，分别真实运行本地 Pion WebRTC 和本地 WebSocket + mTLS，验证 TCP 字节、FIN 半关闭、deadline、peer 断开关闭本地监听、错误 fingerprint、会话撤销、provider 退出 URL 清理。测试中的 WS origin 替换只存在包内私有测试入口，不接受环境变量或公开配置；本地 WS 成功不能代表公网 Cloudflare 成功，本地 WebRTC 成功也不能代表 Wi-Fi/4G NAT 直连成功。

## CLI（历史设备码兼容）

以下设备码登录仅适用于旧服务，不能用于当前声明 `loginFlow: "authorization_code"` 的协调服务。当前 Go CLI 未实现网页登录，测试新服务请使用 [Lab 0.3.0](../apps/tauri-test/README.md)，或由宿主官方 Logto SDK 提供资源 access token。旧客户端授权不能迁作新普通 Native 的凭据。

每台设备执行一次 `login`，在可信浏览器打开显示的 URL、核对设备码，再完成 Logto 注册或登录。macOS 可选 `-open-browser`；Android 不依赖本机浏览器，可以在 Mac 批准。**不要批准非自己发起的设备码**。没有密码参数／账号密码环境变量、PAT、client secret 或中央注册接口。旧 `Register`／`Login` 方法保留编译兼容，但总是明确返回 unsupported 错误；迁移代码必须使用新授权方法。

`-credentials` 或 `CYBERGRID_CREDENTIALS` 指定版本 4 私有 JSON 文件。`login` 原子写入权限 0600；文件包含 server／appId／issuer／公共 clientId／resource／scope／deviceAuthorizationEndpoint／tokenEndpoint／revocationEndpoint 和敏感 refreshToken。不要手工填写、复制给另一台设备、提交仓库或放网页目录。每台设备创建独立 grant。命令每次使用前先核对 version4 与 `appId == clientId == 本次 AppID`，再读取实时配置逐项核对完整 OAuth scope／端点绑定，串行刷新并原子保存最新旋转 refresh token，再交换一个新的 CyberGrid 会话；不保存跨进程复用的 CyberGrid session 或私钥。并发 CLI 使用同一文件通过 `<credentials>.lock` 串行刷新；锁文件需保留，不能在其他进程使用时删除。

```sh
# Mac：创建私有目录；在 Logto 托管页登录／注册，保存此 Mac 自己的授权。
mkdir -p "$HOME/.config/cybergrid"
chmod 700 "$HOME/.config/cybergrid"
./cybergrid-macos login -app "$CYBERGRID_APP_ID" -credentials "$HOME/.config/cybergrid/mac-logto-v4.json" -open-browser

# Mac 主机：创建 loopback echo 服务、公布端口并自动启动 Quick Tunnel。
./cybergrid-macos host -app "$CYBERGRID_APP_ID" -credentials "$HOME/.config/cybergrid/mac-logto-v4.json" \
  -name mac -interface en0 -echo-port 0

# 登录后读取 Logto 应用目录；不是用户已授权应用清单。
./cybergrid-macos apps -app "$CYBERGRID_APP_ID" -credentials "$HOME/.config/cybergrid/mac-logto-v4.json" -no-tunnel

# 查看同账号／同应用设备；退出时注销本次短期设备会话。
./cybergrid-macos devices -app "$CYBERGRID_APP_ID" -credentials "$HOME/.config/cybergrid/mac-logto-v4.json" -no-tunnel

# 从 host-ready 输出取得 DEVICE_ID，再映射远端 echo。
./cybergrid-macos map -app "$CYBERGRID_APP_ID" -credentials "$HOME/.config/cybergrid/mac-logto-v4.json" \
  -peer DEVICE_ID -port echo -local 127.0.0.1:7891 -no-tunnel

# 仅针对 echo 端口验证传输与 hash，默认 1 MiB。
./cybergrid-macos probe -app "$CYBERGRID_APP_ID" -credentials "$HOME/.config/cybergrid/mac-logto-v4.json" \
  -peer DEVICE_ID -port echo -bytes 1048576 -no-tunnel -direct-timeout 12s

# 撤销此 refresh grant，成功后删除本地凭据；失败保留文件并报错。
./cybergrid-macos logout -app "$CYBERGRID_APP_ID" -credentials "$HOME/.config/cybergrid/mac-logto-v4.json"
```

`-server`／`CYBERGRID_SERVER` 默认 `https://cybergrid.aixgg.com`。`-app`／进程环境 `CYBERGRID_APP_ID` 必须显式指定真实 Native ID，命令行优先；无全局默认应用，也不自动读取或执行 `.env`。`echo`／`version` 不需应用 ID，其余认证网络命令均需 ID。

`Apps()`／CLI `apps` 必须先登录，使用中央会话读取 `{source:"logto",apps:[{id,name,type:"Native",isDeviceFlow:false}]}`。这份同租户目录不代表用户已经授权每个应用，不包含 secret／enabled／管理员字段；选另一个应用需在该真实 ID 下独立授权并使用另一个凭据文件。SDK 不实现控制台 SPA 登录或跨应用设备查看。

**从 0.2 升级：** 新版在刷新前明确拒绝 v2 或跨应用 v3 文件，不自动升级、覆盖或删除。旧 API 尚未切换时，可以先用 0.2 `logout` 撤销旧 grant；旧 API 已切换后，0.2 的无 appId 配置请求也会失败，应由 Logto 维护者撤销遗留 grant。删除本地文件不等于撤销 provider 授权。随后用 0.4 在新的v4文件中重新独立授权。

`Close` 关闭当前数据路径并请求注销本次 CyberGrid 会话，不删除持久 Logto grant。`logout` 先核对 v4 应用绑定，再撤销 refresh token（须 provider 广告 revocation endpoint），成功后删除本地凭据；校验或撤销失败保留文件并报错，不会把删文件描述为撤销。它不退出其他浏览器、其他设备或已经运行的中央会话。请先停止运行中的 SDK 进程；中央会话独立最长 24 小时，本次注销／到期仍关闭业务连接，Logto 用户退出或停用不承诺实时撤销已经交换的中央会话。

SDK 的基础公开认证方法为 `AuthConfig`、`BeginDeviceAuthorization`、`CompleteDeviceAuthorization`、`RefreshAccessToken`、`LoginWithAccessToken` 与 `RevokeRefreshToken`。认证端点必须是与 issuer 同源的 HTTPS URL，HTTP 仅允许开发中央 API loopback，OAuth 仍需 HTTPS。轮询至少间隔 5 秒，处理 `authorization_pending`、`slow_down`、拒绝、过期和 context 取消。`CompleteDeviceAuthorization` 在初次 device grant 成功后，必须使用返回的 refresh token 再请求指定 resource 的 API access token，封装 Logto 初次只返回 opaque OIDC token 的实际行为。缺 refresh token 或资源刷新失败会明确报错，不把初次 opaque token／ID token 交给中央；返回值包含最新旋转 refresh token（若 provider 未轮换则保留当前 token）。`RefreshAccessToken` 会请求同一 resource 的 access token；嵌入应用应在自己的安全存储中序列化刷新、原子保存最新 refreshToken。access token 的签名／issuer／audience／权限由中央验证，SDK 不把 ID token 当成 API 身份。

历史设备码流程依据：[Logto Device Flow](https://docs.logto.io/quick-starts/device-flow)。当前服务须配置关闭 Device Flow 的普通 Native 应用、API resource 和 `connect` 用户权限，使用官方 SDK 浏览器授权。SDK 不使用不受支持的授权回退；配置不完整或授权失败会明确报错。
`host -expose api:8080:my-api` 可发布现有服务；`-echo-port 0` 可与其一起使用。`-stun none` 禁用公共 STUN，`-stun stun:IP:PORT,stun:IP:PORT` 可指定通过当批 preflight 验证的端点。不要永久硬编码历史 STUN IP。`-interface en0` / `rmnet_data0` 限制候选接口。`probe -probe-timeout 180s` 可显式调整传输截止时间（默认 45 秒，与 SDK 连接预算分开）；`-diagnostics` 输出连接阶段和 probe 已写入／收到的字节数，不输出业务内容或凭据。`probe` 输出 `mapped.transport`、直连时实际选中 ICE pair，以及 `verified.bytes/sha256/seconds`；仅 hash 一致才退出成功。

### Android 集成

Android arm64 CLI 使用 NDK bionic 的 CGO DNS。宿主每次启动 provider 时应从当前 `ConnectivityManager`／`LinkProperties.dnsServers` 读取实际 DNS，通过 SDK 的 EnvProvider 提供 `CLOUDFLARED_DNS_SERVER` 与私有 TMPDIR；不使用历史运营商 IP，不改系统 DNS、代理或路由。

APK 应运行随包的离线 `scripts/prepare-android-provider.py`，将单份 `libcloudflared.so` 安装到 `ApplicationInfo.nativeLibraryDir`，而不是从 App 可写目录解压执行。详细配置见 [provider-assets/README.md](provider-assets/README.md)。

macOS／Android 图形化测试程序及正式 ADB 命令工具见仓库 `apps/tauri-test/`。ADB 仅控制应用，业务字节走设备间通道；两个设备始终分别授权，不能复制凭据。

## 正式下载与许可

发行资产由 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases) 提供。GitHub Actions 构建平台 CLI、源码和许可证；网站只提供下载入口，不托管大型 SDK 文件。Go 系列包含：

- `cybergrid-sdk-0.4.0-source.tar.gz`：Go SDK、CLI／嵌入源码、测试、构建与打包脚本。
- `cybergrid-sdk-0.4.0-macos-arm64.tar.gz`：Apple Silicon macOS CLI。
- `cybergrid-sdk-0.4.0-android-arm64.tar.gz`：Android arm64／API28+ 原生 CLI。
- `SHA256SUMS`、`manifest.json`：包哈希、原始 binary 哈希、版本、源代码树哈希和构建信息。

每个压缩包包含 README、CHANGELOG、LICENSE、第三方许可证原文及 `BUILD-INFO.json`。下载后先在同目录运行 `shasum -a 256 -c SHA256SUMS`（会校验三个包和 manifest；只下载单包时与清单相应行比对），再解压运行。源码包包含双平台固定 provider、原始许可、补丁／构建信息及离线 Android 打包脚本，不包含真实凭据、测试日志、设备状态、SDK 构建产物或依赖缓存。原始 CyberGrid 源码本轮保留版权，未另授开源许可；第三方自身许可与原文完整保留于 `third_party/`，详见 `THIRD_PARTY_NOTICES.md`。

重新生成发行包：

```sh
# 在源码包 module 根目录运行
GO=go sh scripts/build.sh
GO=go python3 scripts/package.py
```

`package.py` 对显式 SDK 文件列表打包，不扫描用户目录；源树哈希覆盖所有 Go 源码、go.mod／go.sum 和 provider-assets 中的固定资源及许可。归档元数据固定，`SOURCE_DATE_EPOCH` 默认为0并记录于manifest；相同源码、binary与该值会产生相同包字节。`manifest.packages` 列出下载文件 `file`、`bytes`、`sha256`；`manifest.artifacts` 列出压缩包内原始可执行文件的对应信息。
