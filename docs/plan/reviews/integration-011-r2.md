# integration-011 r2 独立复审

结论：**PASS（011 模块范围）**。原 `I011-P1-01` 已由冻结修复关闭，本轮未发现新的实施阻断。此结论不代表 integration-010 整体样例、真实云登录或远程桥接验收完成。

## 冻结与审查范围

- 分支 `task/integration-011`，冻结提交 `ee96a179d789ff7a4b2545efe09d5acd16377d69`；parent 为原冻结 `93798e92c6d24ad082b856e5d96038fb3daa19f8`，原始 base 为 `5cb607d4ed014d527641a7cb7806144163a73584`。
- 原完整 40 路径审查仍见 `integration-011.md`。本轮重新绑定全部 40 路径的工作树字节与 Git blob，只有 `examples/ram-only-workspace/README.md`、`desktop/src-tauri/src/host.rs`、`desktop/src-tauri/src/host/tests.rs` 三个相对路径发生修复变化，其余 37 路径与原审相同。结束时冻结 HEAD、全部 40 hashes 不变且工作树 clean。
- 采用根分支当前 `docs/common-sample.md`、task011 和 INDEX 合同，精确 hashes、实际命令与退出码封于私有证据标签 `integration-011-r2-independent/`。冻结实现未修改；独立 harness 仅在私有镜像追加测试，SDK 只读引用该冻结树。

## 原阻断关闭依据

`Host::enable()` 在等待异步状态锁前，从短同步 `RemoteControl` 锁领取 generation/cancel reservation。后来的 `disable()` 在同一控制锁内先 fence、取消准确 reservation，并登记 owned close；最终 gate commit 也在该锁内核对 generation、cancel 和 closing。旧 enable 不再能够在取得状态锁后替换已经取消的 token。

静态检查全部控制锁持有位置：没有持控制锁跨 await，也没有持控制锁等待状态锁；取得状态锁后短暂检查控制锁的路径不存在反向持锁等待。等待状态锁的 enable 被取消时，其 reservation guard 释放本代受理。已经启动的工作交给 owned task，公开 disable 的整个状态锁等待和清理也交给 owned task；公开调用方停止等待不会丢掉 late factory/bind 或关闭 admission。重复 disable 加入现有 cleanup。

## 独立实际验证

新写三个独立 Rust 场景，实际 **3 pass / 0 fail / 0 ignored，exit 0**；另外运行两个原有受影响回归，各 **1 pass / 0 fail / 0 ignored，exit 0**。合计本轮执行五个不同场景，其中三个为独立新反例；不与开发 17 Rust / 5 JS 或原审计数相加。

1. **公开 queued enable → 后来的 disable，取消关闭等待方。** 仅持现有短状态锁确定公开调用顺序，不修改内部 generation/cancel/closing。disable admission 后丢弃其公开 future，再释放状态锁。旧 enable 明确返回 `CANCELLED`，新 enable 在 owned close 期间返回 `REMOTE_BUSY`；随后公开重复关闭在一秒内完成。实际 factory/upstream/bind 通道均没有派发，目标没有 TCP accept，Client 已被关闭且 admission 回到零。本地同一条已建立 TCP 在关闭前后都实际完成 HTTP 200 请求。这里以明确操作结果、无入口创建和本地正向结果共同判定，未将 timeout 单独视为 PASS。
2. **held bind + held state，enable/disable 两个等待方都被取消。** 真实 guarded TCP 先往返字节，内部专用 bearer 的 HTTP 先得到成功。持状态锁期间受理公开 disable，随后取消其等待和 enable 的等待；真实 gate 两侧在一秒内 EOF，内部 bearer 被拒绝或连接关闭，新 gate 连接被拒绝，而同一条本地 TCP 继续 HTTP 请求。此时借用中的 bind 尚未释放、lease.close 尚未执行，证明关闭入口不依赖慢 bind 收尾。释放状态锁后重复关闭仍等待原工作；释放迟到成功 bind，owned cleanup 在两秒内完成，lease.close 恰一次、startup 清空、closing 回到零；再次双重关闭不增加 close 次数，本地同一 TCP 仍可用。
3. **取消尚未派发的 queued enable。** 丢弃等待状态锁的公开 enable 后，新 enable 能继续进入真正的账号校验并返回 `NOT_LOGGED_IN`，不是永久 `REMOTE_BUSY`；无 factory 派发，无 lease，后续关闭可完成。
4. **默认未注入 Gateway。** 原有实际回归确认 `GATEWAY_UNAVAILABLE`、不注册 Client、复合 gate 保持关闭、重复关闭完成，本地服务和一次性本地打开入口继续工作。
5. **late factory。** 原有实际回归确认启动尚未返回时关闭内部 bearer、本地服务继续；迟到 factory 成功后只 close 一次，重复关闭等待同一清理，不发生远程 commit。

实际日志：`host-independent.log`、`no-factory.log`、`late-factory.log`。复用既有 Cargo target，`DEBUG=0` / `INCREMENTAL=0`，没有 clean 或新建大 target；本轮 Cargo 工作已结束并释放共享 target。

## 原失败证据与开发验证

原 `integration-011-independent/` 的失败测试、日志与原 review 保留，不改写为通过。独立核对开发保留的原测试 tail（包括前导换行）与原件 hash 相同。其原样在修复树重跑的 **exit 101** 发生在无条件等待 bound 地址：正确 early reject 已阻止创建该地址，旧 harness 不能表达此合法分支。r2 使用上述独立新观测分别验证“未启动”及“已启动立即关闭”，没有宣称原样测试通过。

开发 r2 实际日志另经审计：17 Rust / 5 官方 owner JS、严格 Clippy、原生 build 和前端 typecheck/build 均成功。这些是开发证据，不计入本轮独立运行数量。官方 owner、原生 OAuth transport、IPC、两个 loopback listener 的实现及两个 lock 没有在此修复中改变，原完整审查与三个独立官方 owner 受控 OAuth/JWK 用例仍适用。

## 保留边界

运行使用明确的已验证账号 fixture、实际本机 TCP/HTTP 和受控生命周期 lease，不冒充真实 Logto Cloud 登录或实际 Noise。默认 factory/bind unavailable 仍是准确的未安装状态，bridge 仍 unpublished；实际外部 adapter/provider 装配、descriptor 发布/续期、远程 SPA、真实云授权、手机、Wi-Fi/4G、30 分钟/8 小时及 24 小时验收继续由 integration-010 完成。没有启动用户 App、手机或云账号动作，没有修改外部源码、生产、push 或部署。
