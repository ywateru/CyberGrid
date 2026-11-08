# 用自己的服务连接 Mac 与 Android

这条流程从 GitHub 源码开始，用一份服务端 `.env` 启动 CyberGrid，再用已打包的 CyberGrid Lab 完成两端登录、设备互见和端口传输。首次使用无需编译 SDK 或 App。支持 Apple Silicon Mac、Android arm64（Android 9 / API 28 及以上）。

准备 Docker / Compose、Git、Python 3，以及 Android Platform Tools 中的 `adb`。手机开启 USB 调试并认可这台电脑，`adb devices` 应显示 `device`。准备自己的 Logto 租户；用户、应用和登录页面由该租户提供。

## 1. 在 Logto 准备应用和权限

以下项目均在同一个 Logto 租户配置。首次测试的两端共用一个 Native Application ID，分别登录同一个用户账号。

| 项目 | 操作与后续用途 |
| --- | --- |
| 租户 Endpoint | 使用默认租户地址，issuer 为 `https://你的租户.logto.app/oidc`，填入 `LOGTO_ISSUER` |
| API resource | 创建标识为 `https://cybergrid.local/api` 的资源，添加 `connect` permission。将该 permission 分配给测试用户的 User role，已有用户也要分配该角色。资源标识是固定的 OAuth audience，无需解析这个域名 |
| Native Application | 创建 Native 类型应用并启用 Device Flow，记录公开 Application ID。Mac 和 Android 都填写这个 ID，保留大小写；不要填写 SPA 或 M2M 的 ID |
| SPA Application | 为网站个人控制台创建独立 SPA 应用，其 Application ID 填入 `LOGTO_BROWSER_CLIENT_ID`。第 3 步拿到 HTTPS 地址后再登记回跳；Lab 登录不依赖 SPA 回跳 |
| Machine-to-machine Application | 创建 M2M 应用，授予内置 Logto Management API 的 `all` 权限（通过相应 M2M role）。ID 和 secret 填入两个 `LOGTO_MANAGEMENT_*` 配置，供服务读取应用目录 |

Logto 应签发面向上述 API resource、包含 `connect` 的 ES384 用户 access token。API resource 的用户权限与 Management API 的 M2M 权限是两项独立配置。新注册用户也需要测试角色；可在 Logto 将该 User role 配置为新用户的默认角色。登录／注册方式及邮件连接器在 Logto 配置。

具体操作参考 Logto 的 [Device Flow](https://docs.logto.io/quick-starts/device-flow)、[角色与权限](https://docs.logto.io/authorization/role-based-access-control) 和 [Management API](https://docs.logto.io/integrate-logto/interact-with-management-api)。M2M secret 仅保存在服务端；客户端只需 HTTPS 地址和公开 Native Application ID。

## 2. 下载源码，用一份 `.env` 启动

服务端使用 GitHub `main` 分支的当前源码；后文的 `v0.5.1` Release 用于下载原生客户端，其旧 tag 源码不包含这次 Node 服务改造。也可在 GitHub 主分支选择 **Code → Download ZIP** 并解压，再进入其中的 `server/`。

```sh
git clone --branch main --single-branch https://github.com/ywateru/CyberGrid.git
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

`CYBERGRID_PUBLIC_ORIGIN` 留空无需重启或回填临时域名。若也使用网站 `/console/`，在 Logto SPA 应用登记实际 HTTPS origin 和精确回跳 `https://实际地址/console/callback/`。换域名时同步更新 SPA 登记；Native Device Flow 无需此回跳。

## 4. 下载并安装两个测试客户端

从 [Release v0.5.1](https://github.com/ywateru/CyberGrid/releases/tag/v0.5.1) 下载下列文件到仓库根目录的 `lab-downloads/`（自行创建）：

- `CyberGrid-Lab-0.2.0-macos-arm64-test.zip`
- `CyberGrid-Lab-0.2.0-android-arm64-test.zip`，包含 APK、ADB 工具和许可
- `SHA256SUMS` 和 `release-manifest.json`

核对下载 ZIP 的 SHA256。下面只校验这两个文件，不要求下载整份 Release：

```sh
cd lab-downloads
python3 - <<'PY'
import hashlib
from pathlib import Path
checksums = {}
for line in Path('SHA256SUMS').read_text().splitlines():
    fields = line.split(maxsplit=1)
    if len(fields) == 2:
        checksums[fields[1].lstrip('*')] = fields[0]
for name in ('CyberGrid-Lab-0.2.0-macos-arm64-test.zip',
             'CyberGrid-Lab-0.2.0-android-arm64-test.zip'):
    actual = hashlib.sha256(Path(name).read_bytes()).hexdigest()
    if actual != checksums[name]:
        raise SystemExit(f'SHA256 不匹配：{name}')
    print(f'OK {name}')
PY
unzip CyberGrid-Lab-0.2.0-macos-arm64-test.zip -d macos
unzip CyberGrid-Lab-0.2.0-android-arm64-test.zip -d android
cd ..
adb devices
export ANDROID_SERIAL='替换为adb devices显示的实际序列号'
adb -s "$ANDROID_SERIAL" install -r lab-downloads/android/CyberGrid-Lab-0.2.0-android-arm64-test.apk
open 'lab-downloads/macos/CyberGrid Lab.app'
```

两个 ZIP 都输出 `OK` 后才继续。Mac App 未经 Apple 公证；确认下载来源与哈希后，按系统的隐私与安全性提示批准打开。Android 为有 ADB 控制入口的 debug-test APK，测试期间保持前台运行。无需另装客户端 Cloudflared，App 已随包提供。

## 5. 两端分别登录同一个账号

Mac 填写 **Logto Native 应用 ID** 和 **本机名称**（例如 `Mac`）。展开 **连接配置**，把 **中央服务** 从默认官方地址改为第 3 步的 `CYBERGRID_SERVER`。连接策略保持默认，cloudflared 路径留空。点击 **注册／登录（Logto）**，再点击 **打开浏览器授权**，核对本次设备码后完成 Logto 用户登录。

Android 从仓库根目录通过 ADB 控制：

```sh
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" start
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" login \
  --server "$CYBERGRID_SERVER" --app "$CYBERGRID_APP_ID" --name Android
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" status
```

从 `status` 的 `state.authorization.verificationUri` 和 `userCode` 取得 Android 本次授权地址和码。在可信浏览器打开地址、核对码，用与 Mac **同一个 Logto 用户**批准授权；可在 Mac 浏览器完成，手机保持 App 打开。不要复制 Mac 的授权文件。

批准后再次查询 `status`。受理回执不代表登录成功：Android 的 `state.phase` 应为 `ready`、`state.local.deviceId` 有值，Mac 顶部显示 **已登录**。已有这台设备自己的登录时，可用 **恢复本机登录** 或 ADB `resume`（参数同 `login`）。

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
| 应用不存在或不支持 | Native ID 的大小写、是否误填 SPA / M2M ID、Device Flow 是否开启 |
| audience / permission 错误 | `LOGTO_RESOURCE` 与 Logto API resource 是否完全一致，测试用户角色是否包含该资源的 `connect` |
| 应用目录不可用 | M2M ID / secret、内置 Management API `all` 权限及租户是否对应；用 `docker compose logs --tail=50` 查看错误，分享前去除私密信息 |
| 登录一直等待 | 是否批准本次设备码，是否用了正确租户页面；重新查 `status`，不要把受理回执当成功 |
| 看不到对端 | 两端中央地址、Native ID、账号是否一致，对端是否仍前台运行；两端分别刷新 |
| 看到设备但连接失败 | 读取 App 的连接错误，保持两端联网并等待完整连接预算。Quick Tunnel 可能不可用，不关闭 TLS 校验来解决 |
| ADB 结果未知 | 先查询 `status` 确认实际状态，不自动重复发送消息 |

测试结束可执行 ADB `close` 并关闭 Mac 窗口，下次可恢复本机登录。选择 **退出登录** / ADB `logout` 会撤销本机授权。服务可用 `cd server && docker compose down` 停止；重新启动后客户端自动重新注册，已中断的业务 TCP 需要重新连接。

要集成自己的业务，再阅读 [Rust SDK](../sdk-rust/README.md) 或 [Go SDK / CLI](../sdk/README.md)：显式配置自己的中央地址与 Native ID，声明已运行的本机 TCP 服务，获取远端设备与端口，再映射到本地 loopback。平台与运行边界见 [Lab README](../apps/tauri-test/README.md)，内部设计见 [架构](architecture.md)。
