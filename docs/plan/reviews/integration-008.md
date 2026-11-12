# integration-008 独立评审

结论：**blocked**。评审绑定冻结提交 `6237eb08b8a646aecabc2cdd77dd2433fae7779c`，基线 `04e43f6`。独立核对隔离树 clean、全部12个变更路径及开发 manifest 的 SHA256 一致。本报告不代表该功能已合入、发布或完成 Android 实机验收。

## Findings

### I008-P2-01 — blocking：ADB 状态文件被云撤回等待阻塞

- 合同：`docs/lab-remote-controls.md` 的交互和生命周期、Android ADB 控制及状态要求；本机关闭与云端 Pending 分开呈现，`local_closed=true` 即显示关闭。
- 实现：`apps/tauri-test/src-tauri/src/adb_control.rs:492` 写入派发前状态，随后在 `:502` 直接等待 `dispatch`。循环只有在整个命令返回后才再次写状态。
- 独立反例：经实际 Ledger / 文件控制循环派发 `disable-remote`，真实 LabState / SDK 已封闭本机 gate，HTTP fixture 扣住 DELETE 响应；此时内存 `localClosed=true`、`remoteAction=disabling`，而 `status.json` 仍为 `localClosed=false`、旧 `on`。直到云响应释放，文件才更新。
- 影响：Android ADB 使用者不能在云端等待期间观察已经发生的本机关闭；不是 gate 未关闭，但公开状态与实际状态不符。
- 必要修复：在完整持有派发/清理操作所有权的同时，及时输出白名单状态。不得丢弃已派发的授权轮换或清理 future；保持文件权限和公开投影，不输出内部错误。

### I008-P2-02 — blocking：旧目录请求占据 ADB 邮箱，优先关闭不能受理

- 合同：`docs/lab-remote-controls.md` 状态和布局要求“关闭优先，不被正在等待的云操作阻塞”；Android 控制必须可执行相同关闭行为。
- 实现：同一 `adb_control.rs:502` 串行等待普通命令，完成后才删除 `request.json`；`apps/tauri-test/tools/android-adb.py:20` 为防覆盖明确拒绝向已占用邮箱提交命令。
- 独立反例：通过实际 ADB 文件循环派发 `refresh`，扣住真实 HTTP 目录响应。超过一个控制循环周期后，原 `request.json` 仍占用，SDK 远程 gate 仍开放。现有 CLI 的固定无覆盖提交协议不能提交 `disable-remote`，关闭只能等旧云操作完成。
- 影响：新开启动作虽然已经后台拥有，但其他在途云操作仍会阻塞 Android 的关闭入口。
- 必要修复：提供受控的优先关闭受理路径；保留 controlEpoch、sequence、requestId、持久 claim/result 与改内容重放拒绝，不通过盲覆盖邮箱或重复执行未知结果命令解决。后台操作仍须拥有取消、授权/checkpoint 收尾和结果归属。

两项已告 root 与开发 owner；后续必须绑定修复后的新冻结提交复验。本轮评审没有修改实现。

## 独立验证

复用既有 host target，在源码树外构造验证装配。`state.rs`、`services.rs`、`browser.rs`、`platform.rs`、`protocol.rs`、`lib.rs` 与冻结树逐字一致；`adb_control.rs` 原实现逐字保留，仅在末尾增加测试包装。HTTP 测试 fixture 增加目录响应暂停点，另加入独立测试；未改变产品实现。

实际命令使用 `cargo test --locked --offline --features adb-test-control independent_ -- --nocapture`，`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_INCREMENTAL=0`，复用已有 target；退出码0，**6个独立场景断言通过**。其中两项断言专门证实上述缺陷，不能把该测试退出码称为产品验收 PASS：

1. 旧目录 HTTP 响应迟到至关闭/重开之后，不能恢复旧目录、身份或覆盖新 Client 状态。
2. 两个并发关闭共同等待旧后台 AuthProvider，关闭期间新开启被拒；收尾后沿同 owner 新建 Client，不发生重叠轮换。
3. 云撤回被扣住时，连接、消息、probe 全部拒绝，原本地 TCP 和本地设备身份继续可用，503 返回独立 Pending。
4. 实际 ADB 文件循环的正向开启/关闭、同请求幂等、root目录0700、状态/结果0600、白名单无 fixture 凭据；同时观察到 I008-P2-01。
5. 实际 WebRTC + inner mTLS / SDK 流传输65,536字节并逐字核对；关闭后旧流在2秒窗口内 EOF、旧 peer 新 dial 被拒，扣住 DELETE 时原本地 TCP 仍继续。
6. 实际 ADB 文件循环的慢目录请求占用邮箱，证实 I008-P2-02。

已有开发证据另报告 host25/25、Clippy/build、frontend、Python19/19。独立评审核对其冻结清单与日志，未把开发计数混入上述6个独立场景，也未无理由重跑全部构建。

## 边界与后续

- 真实流验证是本机 host → 实际 Rust SDK → 本地 HTTP fixture → WebRTC/mTLS；AuthProvider 和协调身份为明确 fixture，不是正式 Logto 刷新或生产 DELETE。
- 本轮没有 Android APK 构建/安装、设备 ADB 操作、实际 WebView 交互、Wi-Fi/4G 或公网 provider 测试。Android release 排除由 cfg 源码审查佐证，不声称实机完成。
- 独立私有证据位于源码树外 `integration-008-independent`，包括 `review-blocked.json`、冻结/实现绑定、实际命令日志和反例测试源。共享 target 已释放。
- 本报告只评审008的12条应用路径。共同样例的 Gateway 内部认证设计修订不构成008实现或验收证据。
