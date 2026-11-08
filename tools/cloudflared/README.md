# 固定 Cloudflared 资源

本目录管理CyberGrid SDK的macOS arm64／Android arm64固定provider。`bundled-manifest.json` 记录双平台资源；`manifest.json` 记录Android补丁构建依据。Mac来自官方发布压缩包原始字节，Android来自固定源码与最小DNS补丁。运行时不下载资源。

provider只在客户端运行，SDK先以内层mTLS加密业务字节，再通过本地WebSocket origin进入Cloudflare。不能直接把应用的未鉴权端口交给cloudflared暴露。

## 来源与许可

- 上游：[cloudflare/cloudflared](https://github.com/cloudflare/cloudflared)。
- 固定发布：[2026.9.0](https://github.com/cloudflare/cloudflared/releases/tag/2026.9.0)。
- 源码commit：`b749dfd998f4bced81f48727b9c971cfc3056865`。
- Android补丁：`patches/0001-android-explicit-dns.patch`，来源修改与补丁hash记录在manifest。
- 原始Apache-2.0许可保留于LICENSE；双平台依赖及Go runtime许可见BUNDLED_THIRD_PARTY_NOTICES.md，平台实际链接依赖见build-modules文件。

资源二进制SHA256、上游压缩包SHA、编译器版本和构建参数均由manifest记录。公开构建信息移除构建机绝对路径，资源二进制保持原字节；发布构建对所有相关清单重新绑定hash。升级provider需要明确更新pin和校验，不存在自动运行时更新器。

## Android复现构建

需要Git、Python3、固定Go工具链与NDK27.2.12479018，目标Android arm64/API28+。从仓库根目录执行，变量填写自己的工具链：

```sh
GO_BIN="$GO_BIN" ANDROID_NDK_HOME="$ANDROID_NDK_HOME" \
  bash tools/cloudflared/build-android.sh
```

脚本拉取固定官方tag并核commit，应用保留的Android补丁，以 `GOOS=android GOARCH=arm64 CGO_ENABLED=1` 使用NDK bionic编译器，启用 `-trimpath -buildvcs=false`。依赖通过上游go.sum校验，不关闭TLS或checksum。不同Go／NDK版本不保证重建字节相同，采用新资源前必须对照manifest。

Android普通A/AAAA查询走bionic，但部分Go SRV/TXT查询仍可能访问不可用的loopback resolver。补丁仅在 `CLOUDFLARED_DNS_SERVER` 是合法字面IP:port时替换该进程的Go resolver，并保留UDP/TCP及截断重试；IPv6用 `[IPv6]:53`。DNS必须由宿主读取当前网络，不填写历史运营商地址；非法配置明确失败。

不改系统DNS、路由、代理、TUN或TLS证书校验。provider的HTTP/2入口、DNS或Cloudflare握手仍可能被网络阻断，须报告具体失败。Quick Tunnel为开发测试服务，无SLA，限制见 [官方文档](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/)。

## 离线SDK与APK集成

`sync-sdk-assets.py` 校验资源并派生Go／Rust SDK的 `provider-assets/`，包含固定二进制、来源、补丁和许可。SDK运行时不需要本目录或外部下载。

macOS默认使用SDK内置校验提取，支持显式可信外部override。Android宿主使用SDK随附 `scripts/prepare-android-provider.py` 准备单份jniLibs资源，由APK安装到nativeLibraryDir；不在App可写目录解压执行。DNS和TMPDIR在每次启动前从当前宿主环境读取，不继承账号／会话秘密。

具体接口见 [Go资源说明](../../sdk/provider-assets/README.md)、[Rust资源说明](../../sdk-rust/provider-assets/README.md) 和 [Tauri Lab](../../apps/tauri-test/README.md)。设备测试使用Lab的私有ADB控制工具，ADB不承载业务字节。
