# CyberGrid Lab 0.4.0（待发布）

0.4.0 配套 Rust SDK 0.3.0 的显式远程 gate 与新版 ADB 控制，项目目标为尚未发布的 v0.8.0。Lab 是持有既有授权所有者的测试 App，与 RAM-only 共同样例分别交付。以下 0.3.0 / v0.6.0 / v0.7.0 说明保留历史含义，旧下载不会自动更新。

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
3. 登录后默认**远程访问关闭**，本机消息/校验服务只在 loopback 工作，SDK 只记录本地端口意图。两端分别点击 **开启远程访问**，查看发布层“服务已确认”，再刷新设备并连接消息端口。发送必须收到对端 ack 才算成功；两端分别 probe 比较字节与 SHA256。
4. 当前源码默认依次尝试 WebRTC、iroh 直连、Cloudflare 匿名隧道，也可在连接策略中选择 WebRTC/iroh 并行直连；三条路径均使用内层 mTLS。已发布 v0.6.0 包仍为 WebRTC → Cloudflare。实际 transport 显示在 App 中，不能根据设备在线状态推断业务已连接。“强制 Cloudflare 回退”测试策略会同时跳过 WebRTC 和 iroh 直连；它不是默认策略。

连接配置另提供两个测试偏好：`webrtc-only` 关闭本 Client 的 iroh，仍允许 WebRTC 失败后使用 Cloudflare；`iroh-candidate` 保留 iroh，并复用强制回退中接近零的 WebRTC 等待预算，**不是在协议层禁用 WebRTC**，iroh 失败后仍可能使用 Cloudflare。双向测试须分别设置两端，只按消息 ACK／校验记录中的实际 `webrtc-direct` 或 `iroh-direct` 判定相应直连；返回 `cloudflare-tunnel` 只表示回退成功。两个偏好默认均不启用，不改变既有顺序／并行策略。底层配置同时指定 `forceFallback=true` 时仍以强制回退优先；这些选项不改变登录、授权 owner、凭据存储或远程开关。
5. **关闭远程访问**先封闭本机 SDK gate、断开双向远程流并撤回协调会话，保留 Logto 本机授权、本地 listener、已有本地 TCP 与收件记录。离线时显示 `localClosed=true` 和“云端撤回待确认”，不会因撤回失败重新开启。再次开启使用同一个授权 owner 创建新 Client，恢复端口意图；本地端口不重启，消息 ACK 使用新的 deviceId。**退出并撤销授权**另行关闭本地业务并结束授权。

开启等待期间仍可关闭；已派发的 refresh/授权交接会由原 owner 完成 checkpoint，不丢弃未来结果，也不允许新 Client 并发消费同一 grant。设备目录在关闭或协调不可用时显示未知，connect/send/probe 被宿主拒绝。界面分别展示身份、协调、发布、传输和宿主业务授权，在线租约与传输成功都不代表业务授权。云端撤回“已确认”仅指当前可信 epoch 中本 Client 已知会话的精确撤销，不保证历史部署已全部清理。

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
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" enable-remote
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" refresh
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" connect --device-id "$PEER_DEVICE_ID"
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" send --text '来自 Android 的消息'
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" probe --device-id "$PEER_DEVICE_ID"
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" disable-remote
python3 apps/tauri-test/tools/android-adb.py --serial "$ANDROID_SERIAL" close
```

另有 `resume`（配置同 login）、`cancel-login`、`disconnect`、`logout`。`enable-remote` / `disable-remote` 均为空 payload，不接受 token 或配置；重复操作幂等。开启命令返回受理后，用 status 查看 `remoteAction`、`sdkSnapshot` 和发布确认；关闭可在开启尚未完成时执行，`localClosed` 与 `coordinatorRevocation` 分别表示本机关闭和云端撤回结果。`close` / `logout` 是完整退出，与远程关闭不同。命令返回受理不代表 Logto 授权完成，须在系统浏览器完成登录并查看 status。它只输出白名单公开信息，不输出 token、OAuth 内部 device code、注册密钥、私钥或底层响应。

`web-login` / `resume` 的可选 `--direct-transport` 只接受 `auto`、`webrtc-only`、`iroh-candidate`；省略保持旧行为。可与原 `--direct-strategy sequential|parallel` 配合，`--force-fallback` 继续优先。切换测试策略前通过原 owner 的正常关闭／恢复流程结束当前会话，不能为换策略创建第二个 grant 消费者。CLI 可显式传入服务与应用这类公开配置，无需向手机手工输入地址或 token。

ADB通过 `run-as` 访问固定App私有队列，调用与GUI相同LabState；无需输入法、坐标点击、剪贴板或WebView注入，也没有外部HTTP测试桥。debug-test构建显式启用 `adb-test-control`；入口同时要求Android与debug_assertions，普通release即使误传feature也不编译该入口。

请求由 `(controlEpoch, sequence)` 去重，启动生成新 epoch、序号严格递增。UUID 不是永久业务去重键。durable claim 后先发布 highwater，再移走该请求文件并保留原操作任务，因此慢刷新期间也能用同一 CLI 提交 `disable-remote`；不会覆盖尚未受理的邮箱。控制循环每 200ms 写入当前公开状态，云端撤回尚未结束时也能看到本机关闭和 Pending。

最多一个普通操作与一个优先关闭同时执行。普通槽忙或已有优先关闭时，合法下一序号请求会被明确记录为 `BUSY` 并消费该序号，不执行业务。已证明仍由本进程执行的同内容重放等待原结果，不创建重复任务或观察者队列；修改在途请求返回 `SEQUENCE_CONFLICT`。原操作完成前不会因输出失败丢弃授权或清理 future。普通业务命令保持串行提交，优先关闭可以抢先受理。

`result.json` 保留最高已完成序号；状态另带最多 16 条固定 outcome 摘要，供 CLI 按原 epoch、sequence、requestId 识别较早操作的迟到结果。摘要不包含历史 state；CLI 单独读取当前状态，避免旧快照覆盖关闭结果。旧序号结果回收、宿主超时或执行状态不确定时返回 `OUTCOME_UNKNOWN`，不自动重放；先查状态再判断。固定路径不接受任意 shell、文件名、provider 路径、身份秘密或证书参数。

默认超时90秒，`--timeout` 可设至300秒。退出码2：校验／ADB错误；3：App拒绝／操作失败；4：未运行／启动／ADB超时；5：结果未知。协议自测不接触设备，可运行 `python3 -m unittest discover -s apps/tauri-test/tools -p 'test_*.py'`。

## 分发与限制

GitHub Actions从当次源码提交构建macOS测试ZIP、Android APK／ZIP，生成源码与许可包、manifest和SHA256SUMS。发布输入只使用构建所需文件，不包含本机凭据、测试账号、截图、设备ID或缓存。

Android当前按前台使用验证；后台保活、商店发布、无损网络切换和macOS公证需另行配置。Quick Tunnel无SLA，公网WebRTC是否可用取决于实际网络；本地直连成功不能替代公网证据。
