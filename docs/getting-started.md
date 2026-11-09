# 用自己的服务连接 Mac 与 Android

这条流程对应关闭 Device Flow 后的当前源码：用一份服务端 `.env` 启动 CyberGrid，再使用 Lab 0.3.0 的官方 SDK 网页登录完成两端登录、设备互见和端口传输。项目发布 v0.6.0 提供本版源码和客户端；旧 v0.5.1 的 Lab 0.2.0 使用设备码，不适用于新认证策略。支持 Apple Silicon Mac、Android arm64（Android 9 / API 28 及以上）。

准备 Docker / Compose、Git、Python 3，以及 Android Platform Tools 中的 `adb`。手机开启 USB 调试并认可这台电脑，`adb devices` 应显示 `device`。准备自己的 Logto 租户；用户、应用和登录页面由该租户提供。

## 1. 在 Logto 准备应用和权限

以下项目均在同一个 Logto 租户配置。首次测试的两端共用一个 Native Application ID，分别登录同一个用户账号。

| 项目 | 操作与后续用途 |
| --- | --- |
| 租户 Endpoint | 使用默认租户地址，issuer 为 `https://你的租户.logto.app/oidc`，填入 `LOGTO_ISSUER` |
| API resource | 创建标识为 `https://cybergrid.local/api` 的资源，添加 `connect` permission。将该 permission 分配给测试用户的 User role，已有用户也要分配该角色。资源标识是固定的 OAuth audience，无需解析这个域名 |
| Native Application | 创建普通 Native 类型应用并关闭 Device Flow，登记 `http://127.0.0.1:17896/callback` 和 `tech.tinadev.cybergrid.lab://tech.tinadev.cybergrid.lab/callback`，记录公开 Application ID。Mac 和 Android 都填写这个 ID，保留大小写；不要填写 SPA 或 M2M 的 ID |
| SPA Application | 为网站个人控制台创建独立 SPA 应用，其 Application ID 填入 `LOGTO_BROWSER_CLIENT_ID`。第 3 步拿到 HTTPS 地址后再登记回跳；Lab 登录不依赖 SPA 回跳 |
| Machine-to-machine Application | 创建 M2M 应用，授予内置 Logto Management API 的 `all` 权限（通过相应 M2M role）。ID 和 secret 填入两个 `LOGTO_MANAGEMENT_*` 配置，供服务读取应用目录 |

Logto 应签发面向上述 API resource、包含 `connect` 的 ES384 用户 access token。API resource 的用户权限与 Management API 的 M2M 权限是两项独立配置。新注册用户也需要测试角色；可在 Logto 将该 User role 配置为新用户的默认角色。登录／注册方式及邮件连接器在 Logto 配置。

具体操作参考 Logto 的 [Android SDK](https://docs.logto.io/quick-starts/android)、[角色与权限](https://docs.logto.io/authorization/role-based-access-control) 和 [Management API](https://docs.logto.io/integrate-logto/interact-with-management-api)。M2M secret 仅保存在服务端；客户端只需 HTTPS 地址和公开 Native Application ID。

## 2. 下载源码，用一份 `.env` 启动

使用与 Lab 0.3.0 匹配的当前源码进入 `server/`。从 GitHub 下载 v0.6.0 对应源码与客户端，不能将旧客户端与新认证策略混用。下面使用精确项目 tag，避免主分支后续变更；已有同版本工作副本直接进入 `server/`。

```sh
git clone --branch v0.6.0 --single-branch https://github.com/ywateru/CyberGrid.git
cd CyberGrid/server
cp .env.example .env
chmod 600 .env
```

编辑这份 `.env`，替换下面的占位内容。`LOGTO_RESOURCE` 与第 1 步的资源完全一致；`CYBERGRID_PUBLIC_ORIGIN` 可以一直留空。

```dotenv
CYBERGRID_PUBLIC_ORIGIN=
LOGTO_ISSUER=https://你的租户.logto.app/oidc
LOGTO_RESOURCE=https://cybergrid.local/api
LOGTO_BROWSER_CLIENT_ID=你的SPA应用ID
LOGTO_MANAGEMENT_CLIENT_ID=你的M2M应用ID
LOGTO_MANAGEMENT_CLIENT_SECRET=你的M2M应用secret
CYBERGRID_ADMIN_KEY=
CYBERGRID_BIND=127.0.0.1
PORT=7400
```

需要管理后台时，在同一文件填入独立随机的 32–256 UTF-8 字节 `CYBERGRID_ADMIN_KEY`，更新服务后访问 `/admin/`。不需要时保持空。不要把 secret 或管理密钥写进 Git、客户端或截图。

```sh
docker compose up --build -d
curl --fail http://127.0.0.1:7400/healthz
```

健康检查应返回 HTTP 200；本机浏览器打开 `http://127.0.0.1:7400/` 应看到官网。Compose 已包含服务运行依赖，不需要数据库、第二个 CyberGrid 服务或其他 CyberGrid 配置文件。也可改用 [直接运行 Node](deployment.md#直接运行-node) 的步骤。

## 3. 取得两端都能访问的 HTTPS 地址

手机上的 `localhost` 指手机自身。双端测试需要两端都可访问、证书可信的 HTTPS 地址，不能直接把 Mac 的 loopback 或局域网明文地址填给手机。已有 HTTPS 域名入口时，将它指向刚启动的 7400 服务；入口配置要求见 [部署说明](deployment.md#https限流与存活检查)。

首次本地测试可在运行 Docker 的电脑上安装 [cloudflared](https://developers.cloudflare.com/tunnel/downloads/)，另开终端执行：

```sh
# macOS 使用 Homebrew 时：brew install cloudflared
cloudflared tunnel --url http://127.0.0.1:7400
```

按 [Quick Tunnel 官方说明](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/)，复制终端打印的完整 `https://随机名称.trycloudflare.com` 地址，保持进程运行。此方式用于开发测试，无需 Cloudflare 账号或自己的域名，没有长期可用性保证。机器已有命名隧道配置时按官方说明隔离配置，保留现有隧道。这个入口提供网站和协调 API，设备间的业务传输由 SDK 另行建立。

回到 **CyberGrid 仓库根目录**，在后续 ADB 操作的终端填入真实值：

```sh
export CYBERGRID_SERVER='https://替换为你的实际HTTPS地址'
export CYBERGRID_APP_ID='替换为第1步的Native应用ID'
curl --fail "$CYBERGRID_SERVER/healthz"
```

在 Mac 和手机浏览器分别打开这个地址，确认看到官网后再登录客户端。临时入口地址变化会改变客户端连接地址，首次测试期间保持它不变。

`CYBERGRID_PUBLIC_ORIGIN` 留空无需重启或回填临时域名。若也使用网站 `/console/`，在 Logto SPA 应用登记实际 HTTPS origin 和精确回跳 `https://实际地址/console/callback/`。换域名时同步更新 SPA 登记；Native 应用始终使用第 1 步登记的两平台固定回调，与协调服务临时域名无关。

## 4. 下载并安装两个测试客户端

从 [Release v0.6.0](https://github.com/ywateru/CyberGrid/releases/tag/v0.6.0) 下载：

- `CyberGrid-Lab-0.3.0-macos-arm64-test.zip`
- `CyberGrid-Lab-0.3.0-android-arm64-test.zip`，包含 APK、ADB 工具及完整许可
- 同次发布的 `SHA256SUMS` 和 `release-manifest.json`

按 [发布校验说明](releasing.md#create-a-project-release) 核对所选文件的 SHA256，再解压。不要使用旧 0.2.0 包连接新服务。也可按 [Lab 构建说明](../apps/tauri-test/README.md#构建) 从同一 tag 构建。

Mac 打开 ZIP 中的 `CyberGrid Lab.app`。Android 开启 USB 调试，使用实际解压路径安装：

```sh
adb devices
export ANDROID_SERIAL='替换为adb devices显示的实际序列号'
adb -s "$ANDROID_SERIAL" install -r /实际解压路径/CyberGrid-Lab-0.3.0-android-arm64-test.apk
```

Mac App 未经 Apple 公证，Android 为有 ADB 控制入口的 debug-test APK。升级须签名兼容；签名不匹配时停止，不自动卸载或删除旧授权。无需另装客户端 Cloudflared，App 已随包提供。

## 5. 两端分别登录同一个账号

Mac 填写 **Logto Native 应用 ID** 和 **本机名称**（例如 `Mac`）。展开 **高级连接配置**，将中央服务设为第 3 步的 `CYBERGRID_SERVER`，应用设为同租户的普通 Native ID。连接策略保持默认，cloudflared 路径留空。点击 **网页登录**，在系统浏览器完成 Logto 登录并返回 App。使用公开预置测试环境时可直接选择网页登录；旧安装可点击 **使用预置测试配置** 恢复内置公开值。

Android 从仓库根目录通过 ADB 控制：

```sh
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" start
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" web-login \
  --server "$CYBERGRID_SERVER" --app "$CYBERGRID_APP_ID" --name Android
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" status
```

`web-login` 在手机系统浏览器打开 Logto，用与 Mac **同一个 Logto 用户**完成登录后返回 App。ADB 仅控制应用，不输入账号密码或注入授权材料；不要复制 Mac 的授权文件。`login` 是相同网页登录流程的兼容别名。

完成后再次查询 `status`。受理回执不代表登录成功：Android 的 `state.phase` 应为 `ready`、`state.local.deviceId` 有值，Mac 显示已登录。已有这台设备自己的登录时，可用 **恢复本机授权** 或 ADB `resume`（公开配置参数同 `web-login`）。换成新 Native ID 后须独立授权，旧 client 的授权不会被自动迁用。

## 6. 互见、连接、双向收发

两端保持运行。Mac 点击 **刷新**；Android 执行：

```sh
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" refresh
```

Mac 应看到名为 `Android` 的在线设备和已发布端口；Android 的 `state.devices` 应包含 Mac 及端口。Mac 选择 Android，点击 **连接设备**，应显示实际传输方式及 **本地映射** 地址。Lab 已为自己的消息和校验服务声明端口，这个地址是 SDK 创建的本地 TCP 映射，无需手工开放业务端口。

Mac 输入消息后点击 **发送并等待对端确认**。记录应显示 **已确认送达**，Android App 应显示实际收件内容。随后让 Android 连接 Mac 并反向发送；`PEER_DEVICE_ID` 填 Android 设备列表中 **Mac 的真实 `id`**，不是名称或 Android 自己的 ID：

```sh
export PEER_DEVICE_ID='替换为Android设备列表中Mac的实际id'
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" connect --device-id "$PEER_DEVICE_ID"
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" send --text '来自 Android 的消息'
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" probe --device-id "$PEER_DEVICE_ID"
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" status
```

确认 Mac 收到 Android 的内容，Android 记录确认送达。再在 Mac 选择 Android，点击 **64 KiB 传输校验**。两个方向都应报告 **65,536 字节、SHA256 一致**；ADB `state.records` 提供对应 `bytes` / `sha256`，连接状态见 `state.connection.connected`。默认先尝试 WebRTC；显示 Cloudflare 回退也属于有效的端到端互通，不代表公网 WebRTC 已成功。

至此完成三项验收：同账号同应用设备互见、双方消息经对端确认、双向数据校验通过。设备在线仅证明发现和心跳有效，不能代替后两项。

## 遇到问题时

| 现象 | 先检查 |
| --- | --- |
| 手机无法访问官网 | 两端是否使用同一个公网 HTTPS 地址，入口进程是否仍运行；不要使用 `localhost` 或自签名证书 |
| 应用不存在或不支持 | Native ID 的大小写、是否误填 SPA / M2M ID、Device Flow 是否关闭、是否登记两平台精确回调 |
| audience / permission 错误 | `LOGTO_RESOURCE` 与 Logto API resource 是否完全一致，测试用户角色是否包含该资源的 `connect` |
| 应用目录不可用 | M2M ID / secret、内置 Management API `all` 权限及租户是否对应；用 `docker compose logs --tail=50` 查看错误，分享前去除私密信息 |
| 登录一直等待 | 是否在系统浏览器完成登录、是否用了正确租户页面；重新查 `status`，不要把受理回执当成功 |
| 看不到对端 | 两端中央地址、Native ID、账号是否一致，对端是否仍前台运行；两端分别刷新 |
| 看到设备但连接失败 | 读取 App 的连接错误，保持两端联网并等待完整连接预算。Quick Tunnel 可能不可用，不关闭 TLS 校验来解决 |
| ADB 结果未知 | 先查询 `status` 确认实际状态，不自动重复发送消息 |

测试结束可执行 ADB `close` 并关闭 Mac 窗口，下次可恢复本机登录。选择 **退出登录** / ADB `logout` 会撤销本机授权。服务可用 `cd server && docker compose down` 停止；重新启动后客户端自动重新注册，已中断的业务 TCP 需要重新连接。

要集成自己的业务，再阅读 [Rust SDK](../sdk-rust/README.md) 或 [Go SDK / CLI](../sdk/README.md)：显式配置自己的中央地址与 Native ID，声明已运行的本机 TCP 服务，获取远端设备与端口，再映射到本地 loopback。平台与运行边界见 [Lab README](../apps/tauri-test/README.md)，内部设计见 [架构](architecture.md)。
