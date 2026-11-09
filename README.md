# CyberGrid

CyberGrid 用于同一账号、同一应用的终端发现和 TCP 端口互通。用户与应用由 Logto 管理；中央使用 Redis 保存有有效期的会话、在线状态、端口和连接信令，不保存或转发应用数据。

Rust 是主要 SDK，默认按 WebRTC → iroh 直连 → Cloudflare Quick Tunnel 连接，也支持 WebRTC / iroh 并行直连。Go SDK 保留 WebRTC → Quick Tunnel 策略。业务流使用内层双向 TLS。两语言 SDK 随包预置 macOS arm64／Android arm64 Cloudflared。Tauri 2 测试程序提供登录、设备发现、消息确认及 64 KiB 校验；Android 测试通过 ADB 命令控制。

[使用文档](docs/getting-started.md) · [下载 SDK 和 App](https://github.com/ywateru/CyberGrid/releases) · [部署说明](docs/deployment.md) · [架构](docs/architecture.md)

## 部署网站与 API

网站、Logto 控制台、管理后台和 API 支持 Node.js 22 / Docker 或 Vercel Functions。各实例使用 Redis 共享协调状态，不依赖单进程内存。Docker Compose 包含带持久化卷的 Redis；Vercel 使用外部 Redis。

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2Fywateru%2FCyberGrid&env=LOGTO_ISSUER,LOGTO_RESOURCE,LOGTO_BROWSER_CLIENT_ID,LOGTO_MANAGEMENT_CLIENT_ID,LOGTO_MANAGEMENT_CLIENT_SECRET,UPSTASH_REDIS_REST_URL,UPSTASH_REDIS_REST_TOKEN&envDescription=Configure%20Logto%20and%20a%20dedicated%20Redis%20database&envLink=https%3A%2F%2Fgithub.com%2Fywateru%2FCyberGrid%2Fblob%2Fmain%2Fdocs%2Fdeployment.md)

首次部署请阅读 [Vercel 配置步骤](docs/deployment.md#vercel-functions)，准备自己的 Logto 和 Redis 参数，并登记实际域名的登录回跳。

```sh
cd server
cp .env.example .env
chmod 600 .env
# 填写自己的 Logto 配置；公开 origin 可留空
docker compose up --build -d
```

公开 origin 可留空；回跳按每次访问地址生成，OAuth resource 始终固定。Docker 以非 root 用户运行；也可直接 `npm ci --include=dev && npm start`。通过通用 HTTPS 反向代理连接默认 loopback7400端口。详细配置见 [部署步骤](docs/deployment.md)。

## SDK 与测试程序

| 模块 | 版本 | 用途 |
| --- | --- | --- |
| [Rust SDK](sdk-rust/README.md) | 0.2.0 | 主要开发方向，原生 Rust API，与 Go 互通 |
| [Go SDK／CLI](sdk/README.md) | 0.4.0 | 保持兼容，端口声明、设备发现、TCP 映射、自动协调恢复 |
| [JS／TypeScript SDK](sdk-js/README.md) | 0.1.0（项目 v0.7.0） | Logto 登录后只读查询设备与端口隧道地址 |
| [CyberGrid Lab](apps/tauri-test/README.md) | 0.3.0（项目 v0.7.0） | Tauri 2 macOS／Android 测试程序，官方 SDK 网页登录 |
| [协调服务](server/README.md) | memory-v1 协议 | Logto 身份、Redis 共享设备与信令、只读管理后台 |

后续 SDK 功能以 Rust 为主要实现。项目 v0.7.0 支持 WebRTC → iroh 直连 → Cloudflare 匿名隧道的三级连接，也可配置 WebRTC/iroh 并行直连；iroh 禁用中继，业务流仍由终端相互认证并加密。另有 [只读 JS SDK](sdk-js/README.md)，可在前端 Logto 登录后查看设备和端口主动申报的 Cloudflare 地址。已发布的 v0.6.0 下载包仍为原有两级连接，Go SDK 保持兼容。连接边界见 [架构说明](docs/architecture.md#rust-主-sdk-的连接顺序)。

从 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases) 下载平台测试包、SDK 源码和许可证。每次发布附 SHA256SUMS 与产物清单；Actions 从锁定依赖构建。Rust 当前使用本地 Cargo path dependency，尚未发布到 crates.io。macOS 测试包未公证，Android 为包含 ADB 控制入口的 debug-test 包。

客户端传入中央 HTTPS 地址与 Logto Native Application ID。该 ID 就是 OAuth `client_id`，需在 Logto 创建普通 Native 应用并关闭 Device Flow。各设备独立完成 Logto 托管登录，不能复制另一台设备的授权文件。登录后声明本机 loopback 端口，列出同账号、同应用的设备，再将远端端口映射到本机。

Lab 0.3.0 统一使用官方 Logto SDK 网页登录，已预置公开测试配置；Android 使用官方 Android SDK，Mac 的 Tauri 前端使用官方 JavaScript SDK。Device Flow 已关闭，不提供设备码入口。新协调服务和客户端需配套更新；项目 v0.7.0 提供含新连接策略的 Lab 0.3.0，需与同版协调服务配套使用；v0.6.0 的 Lab 0.3.0 仍使用两级传输；GitHub v0.5.1 的 Lab 0.2.0 不适用于新认证策略。详见 [Lab 使用说明](apps/tauri-test/README.md)。

```sh
# 已安装测试 APK、已启用 USB 调试，使用实际连接的 ADB 序列号
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" start
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" status
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" web-login \
  --server "$CYBERGRID_SERVER" --app "$CYBERGRID_APP_ID" --name 'Android'
```

登录命令受理后仍须在系统浏览器的 Logto 托管页面完成登录，再查询 `status`。设备刷新、连接、发送、校验、关闭与退出命令见 [ADB 使用说明](apps/tauri-test/README.md#android-adb-测试控制)。不需要输入法、坐标点击或 WebView 注入。

## 管理后台

部署后的 `/admin/` 使用 服务 `.env` 中独立配置的 `CYBERGRID_ADMIN_KEY`，可查看当前协调会话、在线客户端、端口与信令概况。密钥不会随前端构建下发，仅在当前页面内存保存；退出或刷新后需重新输入。`/console/` 仍提供 Logto 用户登录后的个人只读设备视图。

在线状态表示设备心跳与协调会话有效。中央不接触业务链路，因此不会将其误报为业务 TCP 已建立，也不能查看业务消息、同步内容或文件。

## 项目结构

```text
server/             Node / Functions 网站与 API、Redis 协调与 Docker
sdk/                Go SDK、CLI、离线 provider 及许可
sdk-rust/           Rust SDK、离线 provider 及许可
apps/tauri-test/    Tauri 2 双平台测试 App 与 ADB 工具
tools/cloudflared/  Provider 来源、Android 补丁与复现材料
scripts/release/   CI 构建、打包与公开发布工具
.github/workflows/ GitHub Actions
docs/              接入、架构与部署文档
```

真实 `.env`、GitHub 凭据、签名私钥、本机路径配置、测试截图／账号记录和构建缓存不进入仓库。构建前分别安装 Node.js 22、Go、Rust 与目标平台工具链，具体版本和命令见模块 README 与 Actions。

## 运行边界

- Node 重启和 Function 冷启动保留 Redis 中的会话与 epoch。Redis 数据被清空或 namespace 重置时，SDK 重新注册与发布；不重放业务字节，不承诺旧 TCP 无损续传。`memory-v1` 是现有线协议标识，不代表生产状态仍保存在单进程 RAM。
- 仅支持 TCP 端口映射；端口 `type` 是业务自定义字符串。默认只监听本机 loopback。
- 历史单进程版本的 macOS Wi-Fi／Android 蜂窝测试完成过双向消息、64 KiB hash 与协调重启后原映射新 TCP 恢复，实际使用 Cloudflare 回退。该记录不证明新增 iroh 或本轮 Redis 部署已通过同一网络验收；尚无该网络下 WebRTC 直连成功证据。
- Quick Tunnel 无 SLA。长期运行需选择适合流量的 Node / Functions 和 Redis 方案；免费 Redis 命令额度可能被持续长轮询耗尽。Android 后台保活、商店发布、Apple 公证及无损网络切换不在当前交付范围。
- Logto 租户方案和自定义 API resource 权限需自行确认。邮箱发件域在 Logto SMTP 连接器中配置；真实邮件投递／注册确认需使用自己的邮箱验证。

源码公开不改变现有许可：原 CyberGrid 代码保留其权利声明，第三方依赖按各自许可证分发。完整声明随源码及发布包提供。
