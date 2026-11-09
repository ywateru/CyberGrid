# CyberGrid Lab 0.3.0

Tauri 2 测试 App，macOS 与 Android 共用原生 Rust SDK。两端独立完成 Logto 托管登录，查看同账号同应用设备、连接端口、发送带对端确认的消息，并执行 65,536 字节 SHA256 校验。中央只协调身份和设备元数据，不接触业务内容。

本目录依赖同仓库 `../../sdk-rust`，不运行 Go CLI 或业务代理。完整源码和测试包见 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases)。保留仓库目录结构以便 Cargo path dependency 正确解析。

0.3.0 统一使用官方 SDK 网页登录，关闭 Device Flow 并移除设备码入口。已预置公开测试服务与普通 Native 应用 ID，不预置用户密码。新客户端和协调服务需配套更新；v0.6.0 发布提供本版源码和测试包，现有 v0.5.1 的 Lab 0.2.0 不适用于新服务认证策略。

项目 v0.7.0 进一步增加 Rust SDK 的 iroh 直连阶段、并行连接策略与已有授权的明确恢复提示。v0.6.0 已下载的测试包不会自动获得这些更新。
## 构建

依赖 Node.js22、Rust（`Cargo.lock`／Actions固定工具链）、Tauri CLI2；Android 使用 JDK17、SDK37、NDK27.2.12479018、`aarch64-linux-android` target，最低API28。Gradle与AGP版本以项目wrapper及配置为准。

从仓库根目录进入：

```sh
cd apps/tauri-test
npm ci
sh scripts/build-test.sh macos
```

macOS 目标为arm64，使用Command Line Tools。默认输出在仓库 `.build/tauri-target/debug/bundle/macos/`，可通过 `CARGO_TARGET_DIR` 覆盖。测试App未公证或作商店签名。

Android先设置自己安装的JDK与SDK路径，NDK版本保持一致：

```sh
rustup target add aarch64-linux-android
export NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
sh scripts/build-test.sh android
```

`JAVA_HOME` 和 `ANDROID_HOME` 必须指向实际工具链。脚本自动离线准备provider，生成仅arm64的debug-test APK，包名 `tech.tinadev.cybergrid.lab`。APK位于生成的Android项目 `app/build/outputs/apk/` 下，最终分发由仓库Actions汇总。构建脚本不安装App，也不操作手机或浏览器。

## 双端操作

1. 默认已预置公开测试服务和同一个真实 Logto Native Application ID，可直接选择登录。设备名称按平台预设；两端分别登录同一用户即可互见。自己的部署可在高级连接配置中修改服务和应用；Native 应用需关闭 Device Flow。
2. 点击 **网页登录**，在系统浏览器中打开 Logto 托管登录页。Android 使用官方 Android SDK，Mac 的 Tauri 前端使用官方 JavaScript SDK。Android 也可通过下述 ADB 命令发起。两端分别授权，不复制登录文件。
3. 登录后刷新设备，选择在线对端并连接消息端口。发送必须收到对端ack才算成功；两端分别probe比较字节与SHA256。
4. 当前源码默认依次尝试 WebRTC、iroh 直连、Cloudflare 匿名隧道，也可在连接策略中选择 WebRTC/iroh 并行直连；三条路径均使用内层 mTLS。已发布 v0.6.0 包仍为 WebRTC → Cloudflare。实际 transport 显示在 App 中，不能根据设备在线状态推断业务已连接。“强制 Cloudflare 回退”测试策略会同时跳过 WebRTC 和 iroh 直连；它不是默认策略。
5. 关闭短会话与撤销持久授权是不同操作。退出登录会关闭本轮服务／映射／provider后撤销本机授权；网络失败保留必要材料并如实显示错误，供后续重试。

两平台均使用SDK随包预置并校验的Cloudflared，不需要额外下载。Mac可显式指定可信provider绝对路径作为外部覆盖；默认不搜索PATH。Android使用APK安装的原生库资源。

自己的 Logto Native 应用还需登记以下精确 Redirect URI，关闭 Device Flow：

| 平台 | Redirect URI |
| --- | --- |
| macOS 网页登录 | `http://127.0.0.1:17896/callback` |
| Android 网页登录 | `tech.tinadev.cybergrid.lab://tech.tinadev.cybergrid.lab/callback` |

Mac 只在登录期间监听本机回环地址；端口占用时显示错误，释放端口后重试。两端必须使用同一个 Native 应用 ID，不能用个人控制台的 SPA 应用代替。预置测试应用已登记这两个回调；公共测试环境不预置用户密码，每台设备仍需自行完成 Logto 登录。

升级安装保留旧公开配置，可用“使用预置测试配置”切换到新普通 Native 应用。旧 client 的授权不能迁移；如需撤销，应在服务升级前通过原客户端退出，或在 Logto 中撤销。不要删除、复制或改写旧凭据来绕过应用绑定。

## 协调恢复与凭据

服务端需要 `memory-v1`。App 使用 v4 私有凭据目录，不自动导入旧 v3 文件或另一设备的 grant。Mac 网页登录的 OAuth token 会短暂经过官方 JavaScript SDK 内存，交接后清理；不写入 Web Storage，不进入 View、ADB 输出或日志。后续 refresh、恢复与撤销统一由 Rust 管理；中央会话密钥与端到端私钥只留在 Rust。私有目录 0700、文件 0600。完整交接、取消和回调约束见 [网页登录设计](../../docs/lab-browser-login.md)。

Node 重启或 Function 冷启动不会主动清空 Redis 状态。Redis namespace 重置或 lease 失效后，SDK恢复注册和完整端口快照，保留本地服务与原映射监听意图。恢复中UI清除旧成功状态并暂停发送；Ready后的新TCP重新握手，不重放旧消息或中断流。RequiresLogin需要显式恢复／登录，不后台打开授权页。

首次grant及每次轮换都先保存，再进行后续交换。磁盘失败时Rust保留私有pending所有者，禁止用旧文件刷新；应重试保存或完成撤销。跨Logto签发与本机保存的进程崩溃窗口无法保证为零。窗口关闭会等待checkpoint／清理；若未完成会留在App报告原因。

## Android provider

`scripts/prepare-android-provider.sh` 调用Rust SDK随附的离线脚本，校验资源后写入 `jniLibs/arm64-v8a/libcloudflared.so`。APK通过 `useLegacyPackaging=true`／`extractNativeLibs=true` 和no-strip保留资源字节。SDK读取实际 `ApplicationInfo.nativeLibraryDir`，不从App可写files/cache下载或解压执行程序。

每次启动provider前，JNI读取当前ConnectivityManager／LinkProperties DNS和App cache TMPDIR，仅提供给该子进程。读取失败明确报错，不使用历史运营商地址，不改系统DNS、路由、代理或TLS信任。

完整来源、SHA256、官方Cloudflared pin与Android DNS补丁见 [SDK资源说明](../../sdk-rust/provider-assets/README.md)及 [provider构建](../../tools/cloudflared/README.md)。原始许可、Cargo／npm／Android runtime依赖声明随源码和发布包提供。

## Android ADB 测试控制

以下命令从仓库根目录执行，`ANDROID_SERIAL` 为 `adb devices` 中的目标序列号。0.3.0 可省略服务与应用参数，直接使用公开测试配置发起网页登录：

```sh
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" start
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" web-login
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" status
```

`web-login` 在手机系统浏览器打开 Logto，由用户完成登录后返回 App。`login` 是同一网页登录流程的别名。

Android SDK 等待系统浏览器结果期间，`cancel-login` 先标记取消，仍须用户关闭本轮 Custom Tab 才能结束 SDK 回调。等待超过 45 秒会报告操作失败并保留授权所有者，不自动关闭浏览器或启动新登录。请关闭已取消的授权页，待 App 返回未登录状态后重新发起；不要在已取消的页面继续登录。

若连接自己的部署，`CYBERGRID_SERVER` 为中央 HTTPS 地址，`CYBERGRID_APP_ID` 为该部署可信的真实 Native ID：

```sh
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" start
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" status
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" web-login \
  --server "$CYBERGRID_SERVER" --app "$CYBERGRID_APP_ID" --name 'Android'
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" refresh
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" connect --device-id "$PEER_DEVICE_ID"
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" send --text '来自 Android 的消息'
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" probe --device-id "$PEER_DEVICE_ID"
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" close
```

另有 `resume`（配置同login）、`cancel-login`、`disconnect`、`logout`。命令返回受理不代表 Logto 授权完成，须在系统浏览器完成登录并查看 status。它只输出白名单公开信息，不输出token、OAuth内部device code、注册密钥、私钥或底层响应。

ADB通过 `run-as` 访问固定App私有队列，调用与GUI相同LabState；无需输入法、坐标点击、剪贴板或WebView注入，也没有外部HTTP测试桥。debug-test构建显式启用 `adb-test-control`；入口同时要求Android与debug_assertions，普通release即使误传feature也不编译该入口。

请求由 `(controlEpoch, sequence)` 去重，启动生成新epoch、序号严格递增。UUID不是永久业务去重键。旧序号结果回收、宿主超时或执行状态不确定时返回 `OUTCOME_UNKNOWN`，不自动重放；先查状态再判断。不要并发提交，忙时拒绝写入。固定路径不接受任意shell、文件名、provider路径、身份秘密或证书参数。

默认超时90秒，`--timeout` 可设至300秒。退出码2：校验／ADB错误；3：App拒绝／操作失败；4：未运行／启动／ADB超时；5：结果未知。协议自测不接触设备，可运行 `python3 -m unittest discover -s apps/tauri-test/tools -p 'test_*.py'`。

## 分发与限制

GitHub Actions从当次源码提交构建macOS测试ZIP、Android APK／ZIP，生成源码与许可包、manifest和SHA256SUMS。发布输入只使用构建所需文件，不包含本机凭据、测试账号、截图、设备ID或缓存。

Android当前按前台使用验证；后台保活、商店发布、无损网络切换和macOS公证需另行配置。Quick Tunnel无SLA，公网WebRTC是否可用取决于实际网络；本地直连成功不能替代公网证据。
