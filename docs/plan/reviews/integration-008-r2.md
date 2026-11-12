# integration-008 replacement freeze 独立评审

结论：**PASS**。绑定完整冻结 `d953b61371e3e8b09487e9144d4a24605356618b`，基线 `04e43f633d9e1a9854fe5e338bfecb102a36d9ee`。隔离树 clean，全部12个应用路径与开发 manifest 的 SHA256 一致。此结论允许进入合入门禁，不代表已经发布、部署或完成 Android 实机验收。

## 原阻断闭环

- **I008-P2-01 已修复**：控制循环不再等待整个 dispatch 才输出状态。扣住真实协调 HTTP DELETE 响应时，实际 LabState/SDK gate 已关闭，`status.json` 在独立1秒窗口内显示 `localClosed=true`、`remoteAction=disabling`、`coordinatorRevocation=pending`。独立观测约247ms；此值是该次主机测试观测，并非平台实时性保证。
- **I008-P2-02 已修复**：durable claim 后先输出 highwater，再释放已认领邮箱。原 Python CLI 和未修改的 PUBLISH shell 常量，经私有文件 transport 装配，能在目录 GET 被扣住时提交优先 disable；没有覆盖未认领请求。已派发任务由 JoinSet 持有，普通操作与优先关闭各有受控槽位。

同内容且仍有本进程 owner 的 pending 重放不重派、不产生提前 `OUTCOME_UNKNOWN`、不占住邮箱。合法下一序号 BUSY 被明确完成并消费序号；gap 不推进 highwater。较低序号迟到完成不能覆盖较高 result；CLI 根据原 epoch/sequence/requestId 从有界摘要识别结果，单独使用当前 live state，不恢复历史状态。

## 全范围源码审查

审查覆盖12个路径：宿主状态与服务生命周期、IPC 注册、前端按钮/状态/CSS、ADB ledger/dispatch/CLI、测试及应用 README。与首次完整评审逐字比较，7个路径未改变，5个路径为本轮修订；同时重新核对完整冻结而非只看两处补丁。

未发现剩余实施阻断：登录仍默认关闭远程；显式 enable 保存代际 reservation，候选、已安装 Client 与旧 provider/checkpoint 的所有权未交给无主 future；disable 先驱动 SDK 封 gate，再持续驱动其关闭 future，避免与 Mapping 清理锁互相等待。迟到目录、watch、enable 不能覆盖新 Client 状态。LocalServices 使用独立 lifetime，远程关闭保留已有本地连接、收件记录和 listener，重开更新 ACK 身份并沿同一个授权 owner 创建新 Client。

ADB 入口仍受 Android、debug_assertions 与显式 feature 三重条件限制。请求/结果、epoch、序号、改内容重放、固定路径与0700/0600权限边界保留；状态采用显式白名单，completed 最多16项元数据，不含历史 state。持久状态输出失败停止新受理，已派发授权/关闭操作继续收尾，不因控制循环退出而 abort 其唯一 owner。

## 独立实际验证

在源码树外装配冻结应用源码，SDK 指向冻结树；产品模块逐字不变，仅在现有测试 fixture 末尾追加独立测试。复用既有 Cargo target，未 clean 或重建完整 App bundle。实际执行 `cargo test --locked --offline --features adb-test-control independent_ -- --nocapture` 的首组6场景全部通过；随后新增双向流场景单独执行，1/1通过，两次退出码均为0。没有把开发 host28/28、Python22/22计入独立场景数。

独立7个场景覆盖：

1. 迟到目录响应不能恢复关闭前状态或覆盖同 owner 的新 Client。
2. 两个关闭等待同一个旧后台授权 owner；其收尾前不能另起新轮换。
3. 云撤回 Pending 时 connect/send/probe 拒绝，本地身份与 TCP 保留。
4. 实际 WebRTC/inner mTLS 流与旧 peer 在关闭后停止，而本地 TCP 继续。
5. 原 CLI、实际文件循环与扣留 GET/DELETE，验证优先关闭、pending 重放、BUSY、gap、result 高水位、权限与白名单。
6. status 写入失败时，已派发的授权轮换和关闭 owner 仍完成；随后沿同 owner 重开。
7. 实际 WebRTC/mTLS 两个方向各传输并逐字核对65,536字节；扣住 DELETE 后，两个活跃远程流均在2秒窗口内 EOF，旧 peer 新 dial 拒绝。同一个已接受本地 TCP 和本地收件服务继续工作；同 owner 新 Client 重开后，原本地连接和记录仍保留。

另对原 Python CLI 做5项独立断言：PUBLISH 常量与首次冻结一致、较早结果从有界摘要解析并使用 live state、无当前状态时不输出历史 state、超限摘要拒绝。退出码0。开发构建/Clippy/frontend/Python日志及完整 manifest 已核对，未无理由重跑全部构建。

## 证据与边界

私有证据目录为源码树外 `integration-008-r2-independent`，包含 `source-binding.json`、`first-freeze-comparison.json`、追加测试源码、实际日志与 `review-pass.json`。共享 target 已释放；冻结实现与用户 App、手机、生产环境均未修改。

实际加密路径为本机 LabState → Rust SDK → 本地协调 HTTP fixture → WebRTC/mTLS。授权和协调身份为明确 fixture，不是正式 Logto 登录/刷新。此轮没有 Android APK 构建/安装、手机 ADB 操作、WebView 交互、Wi-Fi/4G、真实 Cloudflare provider 或长跑验收；不将主机测试等同于这些结果。首次 BLOCKED 报告继续保留作为历史，当前 PASS 只针对上述 replacement freeze。
