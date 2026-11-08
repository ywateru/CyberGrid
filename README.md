# CyberGrid

CyberGrid 用于同一账号、同一应用的终端发现和 TCP 端口互通。用户与应用由 Logto 管理；中央只在内存中保存短会话、在线状态、端口和连接信令，不保存或转发应用数据。

Go 和 Rust SDK 优先建立 WebRTC 直连，失败后使用 Cloudflare Quick Tunnel 与内层双向 TLS。两语言 SDK 随包预置 macOS arm64／Android arm64 Cloudflared。Tauri 2 测试程序提供登录、设备发现、消息确认及 64 KiB 校验；Android 测试通过 ADB 命令控制。

[使用文档](docs/getting-started.md) · [下载 SDK 和 App](https://github.com/ywateru/CyberGrid/releases) · [部署说明](docs/deployment.md) · [架构](docs/architecture.md)

## 部署网站与 API

网站、Logto 控制台、管理后台和 API 由一个长期运行的 Node.js22 进程提供，协调状态只在同进程有界内存中。无需数据库，不使用多副本或 serverless Function。

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
| [Go SDK／CLI](sdk/README.md) | 0.4.0 | 端口声明、设备发现、TCP 映射、自动协调恢复 |
| [Rust SDK](sdk-rust/README.md) | 0.2.0 | 原生 Rust API，与 Go 互通 |
| [CyberGrid Lab](apps/tauri-test/README.md) | 0.2.0 | Tauri 2 macOS／Android 测试程序 |
| [内存协调服务](server/README.md) | memory-v1 | Logto 身份、设备与端口信令、只读管理后台 |

从 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases) 下载平台测试包、SDK 源码和许可证。每次发布附 SHA256SUMS 与产物清单；Actions 从锁定依赖构建。Rust 当前使用本地 Cargo path dependency，尚未发布到 crates.io。macOS 测试包未公证，Android 为包含 ADB 控制入口的 debug-test 包。

客户端传入中央 HTTPS 地址与 Logto Native Application ID。该 ID 就是 OAuth `client_id`，需在 Logto 创建应用时启用 Device Flow。各设备独立完成 Logto 托管登录，不能复制另一台设备的授权文件。登录后声明本机 loopback 端口，列出同账号、同应用的设备，再将远端端口映射到本机。

```sh
# 已安装测试 APK、已启用 USB 调试，使用实际连接的 ADB 序列号
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" start
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" status
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" login \
  --server "$CYBERGRID_SERVER" --app "$CYBERGRID_APP_ID" --name 'Android'
```

登录命令受理后仍须在 Logto 托管页面批准授权，再查询 `status`。设备刷新、连接、发送、校验、关闭与退出命令见 [ADB 使用说明](apps/tauri-test/README.md#android-adb-测试控制)。不需要输入法、坐标点击或 WebView 注入。

## 管理后台

部署后的 `/admin/` 使用 服务 `.env` 中独立配置的 `CYBERGRID_ADMIN_KEY`，可查看当前协调会话、在线客户端、端口与信令概况。密钥不会随前端构建下发，仅在当前页面内存保存；退出或刷新后需重新输入。`/console/` 仍提供 Logto 用户登录后的个人只读设备视图。

在线状态表示设备心跳与协调会话有效。中央不接触业务链路，因此不会将其误报为业务 TCP 已建立，也不能查看业务消息、同步内容或文件。

## 项目结构

```text
server/             单进程 Node 网站、API、内存协调与 Docker
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

- owner 重启会清空内存并更换 epoch。SDK 保留本地服务和映射意图，协调恢复后新的 TCP 连接可重新握手；不重放业务字节，不承诺旧 TCP 无损续传。
- 仅支持 TCP 端口映射；端口 `type` 是业务自定义字符串。默认只监听本机 loopback。
- 真实 macOS Wi-Fi／Android 蜂窝测试完成了双向消息、64 KiB hash 与 owner 重启后原映射新 TCP 恢复。公网实际使用 Cloudflare 回退；尚无该网络下 WebRTC 直连成功证据。
- Quick Tunnel 无 SLA。长期运行需自行选择可靠的 Node 宿主和 HTTPS 入口；Android 后台保活、商店发布、Apple 公证及无损网络切换不在当前交付范围。
- Logto 租户方案和自定义 API resource 权限需自行确认。邮箱发件域在 Logto SMTP 连接器中配置；真实邮件投递／注册确认需使用自己的邮箱验证。

源码公开不改变现有许可：原 CyberGrid 代码保留其权利声明，第三方依赖按各自许可证分发。完整声明随源码及发布包提供。
