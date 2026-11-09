# Lab 网页登录

用户已确认关闭 Device Flow，统一使用官方 Logto SDK 的系统浏览器 Authorization Code + PKCE 登录。Mac 与 Android 使用同一个普通 Logto Native `client_id`，中央保持 `(subject, client_id)` 隔离；不建立应用别名或跨应用映射。当前版本按用户授权发布为 v0.6.0，并将配套协调服务部署到 `https://cybergrid.aixgg.com` 进行正式域名测试。

## 应用与认证契约

协调服务只允许未启用 Device Flow 的 Native 应用，认证配置返回 `loginFlow: "authorization_code"`。SPA、M2M、Device Flow Native 均不能注册设备；JWT 验证后的 client_id 必须与请求 AppID 完全一致。个人控制台仍使用独立 SPA。应用目录返回实际的 `isDeviceFlow: false`，不伪造应用能力。

Logto 不支持通过公开 API 修改既有应用的 `isDeviceFlow`。预置测试应用须使用普通 Native 应用，并登记两平台精确回调；旧 Device Flow 应用不自动删除，不复制或迁移其授权。新 client_id 是独立设备命名空间，旧会话或授权不能冒充新应用。旧安装可以通过“使用预置测试配置”切换公开配置。旧授权应在服务升级前通过原客户端退出撤销，或在 Logto 撤销；新服务不再接受旧应用的认证配置请求，不承诺升级后原客户端仍能执行退出流程。

SDK 保留历史 Device Flow API 以维持源码兼容，但服务器声明 `authorization_code` 时，在调用设备授权端点前明确拒绝。Lab 不再提供设备码登录入口。ADB 的 `login` 与 `web-login` 均启动同一个官方 Android SDK 网页登录流程。

## 登录与预置配置

客户端预置公开测试服务地址、Native 应用 ID 和平台回调地址；从该服务的公开认证配置获取并核对可信 Logto endpoint、issuer 和 API resource，用户无需手动填写。预置值不含密码、client secret、管理令牌、refresh token 或管理后台密钥。高级配置仍可连接自己的部署；自定义配置必须通过服务公开认证配置核对 issuer、应用和 resource，不能静默退回公共测试配置。

网页登录使用系统浏览器、Authorization Code 与 PKCE，Android 接入官方 Logto Android SDK，Mac 的 Tauri TypeScript 前端接入官方 Logto JavaScript SDK。每个物理设备独立登录，不复制另一设备的凭据。

回调必须匹配本轮登记的地址与 state，由官方 SDK 验证并消费 code。未发起登录、已取消、超时或已退出后的回调不创建会话。应用关闭、取消、重新登录与退出共享原有代际隔离；晚到的授权材料必须被安全清理或保留在私有待清理所有者中，不能丢弃未撤销的授权。

一个 grant 只能有一个 refresh 所有者。两平台选用一次性交接：官方 SDK 完成浏览器授权并取得指定 resource 的 access token，导出此时最新的 refresh token；Rust 校验并原子保存绑定中央地址、应用、issuer、resource、scope 和可信端点的凭据，再确认交接。确认后官方 SDK 清理自己的临时内存并停止令牌调用，后续恢复、刷新、撤销都由原有 Rust CredentialStore/AuthProvider 负责。不得让 SDK 与 Rust 同时刷新同一 grant，也不得调用 SDK 的注销接口撤销已经移交的授权。

Mac 使用官方 `@logto/client` 的公开 ClientAdapter 扩展点和 `getRefreshToken`、`getAccessToken`、`clearAllTokens`；临时存储不使用 Web Storage。通过本机独占监听的 `http://127.0.0.1:17896/callback` 接收系统浏览器回调；端口被占用时明确报错，释放端口后重试，不静默改变回调或退回设备码。网络请求经受限的原生适配器，仅允许本轮可信 Logto 端点，响应先交私有授权所有者处理，再返回 SDK；不得开放任意 URL 代理。

JS SDK 通过公开 `buildJwtVerifier` 扩展和 JOSE 的 `createLocalJWKSet`／`jwtVerify` 验证 ID token，可信 discovery 指定的精确 JWKS URL 同样经过受限原生 fetch。保持签名、issuer、应用 audience 和签发时间校验，并要求有效 `sub`、`iat`、`exp`；不通过宽泛 CSP 放开网络，也不因自定义部署另换 Logto 租户而跳过验证。

Android 固定官方 `io.logto.sdk:android:3.0.0`，使用其系统 Custom Tabs 登录。该版本没有公开 refresh token 导出接口，因此使用官方 open 类的受保护成员子类扩展，关闭 SDK 持久存储，仅用于读取本轮最新授权并在交接后清理内存。这是对固定版本受保护扩展面的依赖，不声称是官方公开导出 API；不得通过反射、私有字段或读取 SDK 存储实现。SDK 操作串行化，导出后不再调用其刷新；升级 SDK 必须重新编译并验证此边界。若受保护扩展不能满足安全交接，停止该路径并报告明确错误，不能降级为双所有者。

Android 在登录成功但获取 resource token 失败时，用专用原生 abort 入口提交最新 grant 并撤销，不把初始非 resource token 当作最终登录导入。只有收到已接管并清理的确认后才清 SDK 内存；失败则保留所有者供重试。此入口不接受外部 ADB token 注入，不注册设备。

首次授权、获取 resource token 可能导致的轮换、保存失败和确认丢失，均须保留最新待交接材料。Rust 原有 pending owner 和 checkpoint 规则适用于新路径；保存成功但确认丢失时，重试确认须幂等，不能启动第二轮刷新。取消、退出或过期的授权不能注册设备，已取得的 grant 必须保存到私有待清理所有者并撤销或明确报告待重试。Logto 签发与本机第一次收到并保存响应之间的崩溃窗口无法完全消除，不承诺跨外部签发事务的零丢失。

交接确认区分 `OwnedPersisted`（Rust 已持有且保存成功）与 `OwnedPending`（Rust 已持有最新材料，但磁盘 checkpoint 待重试）。两者都要求原 SDK 停止刷新；后者不得继续交换中央会话或显示 Ready。校验失败且未接管则返回明确错误，原 SDK 所有者仍须保留授权并清理。导入不覆盖既有 grant，不触发刷新，也不把本地解析 JWT 当作认证；先完成交接与保存，再由原有中央 exchange 验证 JWT 并注册，验证失败仍走授权清理。

网页登录的 access/refresh/id token 会短暂经过官方 JavaScript SDK 内存，这是此前“令牌从不进入 WebView”规则的明确修订；中央会话密钥与端到端私钥仍只由 Rust 持有。OAuth 材料不可进入 UI View、ADB 输出、普通日志、localStorage、sessionStorage 或 IndexedDB。Rust 接口只向本地可信前端提供一次性事务能力，状态查询永远不返回 token。错误只返回安全的固定描述，不透传 OAuth 原始响应。

## 登录页

```text
┌─────────────────────────────────────────────┐
│ CyberGrid Lab                               │
│ 测试环境 · 已预置服务与应用                   │
│ [使用预置测试配置]（可替换历史公开连接配置）   │
├─────────────────────────────────────────────┤
│ 设备名称 [Mac / Android                  ]  │
│ [ 网页登录 ]                                 │
│ [恢复本机授权]  [退出并撤销]（有授权时）       │
│ ▸ 高级连接配置                              │
├─────────────────────────────────────────────┤
│ 等待网页登录：请在系统浏览器完成登录 [取消]   │
│ 错误：安全的原因与可执行的重试操作           │
└─────────────────────────────────────────────┘
```

登录进行中禁止并发启动另一轮登录，显示等待浏览器并提供取消。不显示设备验证码或设备码登录按钮。恢复授权不自动打开浏览器；需要重新授权时显示明确状态，由用户发起。

登录完成后进入原有设备列表、端口连接、消息确认和内容校验页面。窄屏按单列排列按钮，高级配置折叠，不产生横向滚动。失败保留用户填写的非秘密配置，取消返回登录选择；错误消息不包含 OAuth 响应或凭据。

升级安装保留历史公开连接配置。有历史自定义地址时，可在空闲登录页点击“使用预置测试配置”，将表单恢复为内置测试服务、应用和设备名称；随后通过正常登录路径应用该配置。该按钮不删除或撤销任何凭据，不修改正在执行的授权或会话。公共默认值由 Rust 返回，前端不另维护一份易漂移的应用 ID。

## Android ADB

私有 debug 队列的 `web-login` 和 `login` 均为网页登录入口，与 GUI 调用同一个 LabState；省略公开配置参数时使用客户端相同预置值。ADB 只发起授权和查询安全状态，不能注入 token、code、回调 URL 或账号密码，不使用输入法、坐标、剪贴板或 WebView 注入。浏览器中的用户身份认证与授权仍由 Logto 托管页面完成；命令受理不代表登录成功。

Android 从 Rust 发起或取消 SDK 登录时，必须通过 Tauri/Wry 提供的 Activity 调度桥，不能将 NDK application context 当作 Activity。调度等待有界，排队中的启动仍须核对本轮取消与代际状态。

官方 Android SDK 3.0.0 没有公开的 Custom Tab 强制取消接口。SDK 登录尚未回调时，取消先标记本轮作废，保留源所有者，等待用户关闭浏览器授权页；45 秒未结束则明确返回错误，不能伪报取消完成或允许第二轮并行登录。已取消的页面即使后来成功授权，也只能进入撤销清理。ADB 不通过输入、强停或注入回调来绕过这项限制。

调试诊断只保存有界的固定阶段、能力布尔值和白名单 OAuth 错误类别，用于区分配置、打开浏览器、回调及交接失败。不得记录完整授权或回调 URL、原始错误响应、code、state、token 或账号信息；诊断失败不改变授权所有权。

## 验证范围

验证单一网页登录入口、预置值、普通 Native app 身份、Device Flow 拒绝、PKCE 回调、取消与晚回调、恢复与退出、refresh 单所有者和凭据输出边界，运行相关类型检查、原生构建及针对认证边界的测试。既有传输与公网验收不因本次修改重复运行；不能将编译成功表述为实际用户已完成浏览器授权。
