# SDK 与平台接入

| 平台 | Go SDK／CLI | Rust SDK／Lab |
| --- | --- | --- |
| macOS arm64 | 原生CLI，预置Cloudflared | 原生SDK及Tauri2 App，预置Cloudflared |
| Android arm64 API28+ | 原生CLI，宿主配置provider执行位置 | SDK离线资源、Tauri APK安装原生provider、私有ADB控制 |

完整API示例以 [Go README](../sdk/README.md)、[Rust README](../sdk-rust/README.md) 和 [Lab README](../apps/tauri-test/README.md) 为准。下载位于 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases)，不要把网站HTML下载当作SDK压缩包。

接入需要中央HTTPS地址、启用DeviceFlow的真实Logto Native client_id、设备名称，以及应用自己的loopback TCP服务。SDK完成托管登录后取得中央短会话，公开端口声明包含端口号与应用定义的type字符串；发现接口返回同账号、同应用的其他设备及端口；映射接口把选定远端端口暴露为本机loopback监听。

默认策略先尝试WebRTC，再用Cloudflare Quick Tunnel及内层mTLS。两语言SDK均自带固定校验资源，不隐式搜索PATH或在运行时下载执行文件。macOS采用校验后的私有缓存提取；Android应随宿主APK安装provider至原生库目录，按 [Lab集成](../apps/tauri-test/README.md#android-provider) 提供实际DNS与TMPDIR回调。

授权文件与服务器／应用／scope／可信端点绑定。应用应显式设置私有凭据存储与AuthProvider，让refresh轮换在后续调用前checkpoint；磁盘失败时保留最新私有材料供重试或撤销，不用旧token继续，也不复制其他设备授权。

中央RAM状态丢失后，SDK重新注册、重发完整端口快照，原映射监听保留意图并等待当前代际的新连接。关闭操作取消该意图并停止provider／监听；中断的业务流不自动重放。业务消息应使用自己的ack、去重和同步机制。

Android测试使用仓库的 [android-adb.py](../apps/tauri-test/tools/android-adb.py)，通过固定App私有命令队列调用与GUI相同的真实SDK逻辑。它不接受任意shell、凭据、证书或文件路径；登录受理、托管授权完成、SDK Ready和端到端数据确认是不同状态。发生超时或结果未知时先查status，不盲目重放业务命令。
