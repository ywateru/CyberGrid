# 从登录到端口互通

1. 按 [部署说明](deployment.md) 配置 Logto，用 Node / Docker 运行网站、API 与唯一内存协调服务。记录自己的中央 HTTPS 地址和真实 Native Application ID。
2. 从 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases) 下载 Go SDK／CLI、Rust SDK 或 Tauri Lab。核对发布清单中的平台、版本、文件大小和 SHA256。
3. 各设备独立登录同一个 Logto 账号，并使用同一个应用 ID。SDK 不接收账号密码，登录／注册在 Logto 托管页面完成。
4. 通过 SDK 声明本机可提供的 loopback TCP 端口及业务类型字符串，列出同账号、同应用的其他设备及端口。
5. 创建远端端口到本地 loopback 端口的映射。业务通过该本地端口建立普通 TCP 连接；SDK 优先 WebRTC，再使用 Cloudflare 回退。
6. 在应用层确认收到的数据。Lab 的发送操作需要对端 ack，probe 比较 65,536 字节与 SHA256。协调服务返回在线状态本身不能证明业务数据已传送。

Go 的 API／AuthProvider／CLI 示例见 [Go README](../sdk/README.md)，Rust 的 Cargo 接入与凭据存储见 [Rust README](../sdk-rust/README.md)，macOS／Android 构建和 ADB 控制见 [Lab README](../apps/tauri-test/README.md)。

两语言 SDK 都预置了支持平台的 Cloudflared。macOS 默认提取并校验内置资源；Android 将资源随宿主 APK 安装到原生库目录执行，不在 App 可写目录下载执行程序。当前覆盖 macOS arm64 与 Android arm64 API28+。

SDK 的 v4 凭据文件绑定服务器、应用、scope 和可信 OAuth 端点，私有目录0700／文件0600。不要提交、共享或跨设备复制；旧版本文件不自动导入。关闭短会话与撤销持久授权是不同动作，清理文件不等于撤销 Logto grant。

owner 重启后 SDK 自动恢复注册和端口。恢复期间不要把旧状态当作已连接；原本地映射的新 TCP 在恢复后可重新连接，已经中断的 TCP 不承诺无损续传。更多边界见 [架构](architecture.md)。
