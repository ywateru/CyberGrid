# integration-011 独立评审

结论：**BLOCKED**。发现并实际复现一项 P1 关闭优先竞态；原实现不能合入作为已通过的远程关闭模块。没有确认其他 blocking，修复后需按新 freeze 复验。

## 绑定与范围

- 冻结提交：`93798e92c6d24ad082b856e5d96038fb3daa19f8`；base：`5cb607d4ed014d527641a7cb7806144163a73584`。
- 全部 40 个新增路径均在 `examples/ram-only-workspace/`，已核对 Git blob、开发 `source-sha256.txt` 与实际工作树。静态审查包含 Rust/TS 实现和测试、两个 lock、Tauri 配置/capability、静态页面与图标；未修改冻结实现，结束时 HEAD 与全部 40 hashes 不变且 clean。
- 使用根分支当前 `common-sample.md`、task011、INDEX 与准入/桥接相关合同；不是仅用隔离树旧合同。当前合同 hashes 保存在私有 `root-contract-binding.json`。
- 独立证据位于私有 archive 的 `integration-011-independent/`，含 `INDEX.md`、`source-binding.json`、`commands.json`、原始日志与 source-after。复用了既有 Cargo target，DEBUG=0/INCREMENTAL=0，没有 clean 或新建大 target。

## I011-P1-01 — 已受理但等待状态锁的 enable 会覆盖后续 disable 的取消

位置：`examples/ram-only-workspace/desktop/src-tauri/src/host.rs:347`、`:359`、`:456`、`:462`。

合同：`docs/common-sample.md:39` 要求关闭先同步封闭复合 gate，关闭代际的迟到 Gateway 不得复活；`:43` 要求 bind 尚未收尾时也必须封闭受控 TCP gate 与内部 upstream；`:45` 要求本机关闭结论覆盖实际两条入口。根合同确认该优先级覆盖已受理但仍等待短锁的 enable。

`enable()` 先等待 `state.lock()`，获得锁后才创建新 action/cancel 并替换 `self.cancel`。`disable()` 在等待同一状态锁之前已经调用 `seal()`，只取消当时的旧 token。若 enable 已经排在状态锁队列前方，disable 随后 seal 并排队，enable 获锁后便把已取消的旧 token 换成一个新且未取消的 token。disable 随后取到 startup 并等待其完成，却没有封住该新 token 对应的 ingress/upstream。最后的 commit 检查虽然阻止将 Gateway 安装进状态，但不能关闭仍在 held bind 阶段已开放的入口。

独立实际反例使用公开 `Host::enable()` / `Host::disable()`。仅临时持有现有短 `state` mutex 以确定两者排队顺序；没有改 action/cancel/closing 或实现逻辑。将已经验证的账号作为明确 fixture 种入（不冒充真实登录），使用受控 factory/held bind、实际 loopback TCP echo 与独立 upstream HTTP。释放状态锁后观察到：

```text
localClosed=true
phase=disabling
guardedIngressAlive=true
upstreamBearerAlive=true
localHTTPAlive=true
disableStillOwned=true
```

guarded ingress 在关闭已开始后仍真实收发 `afteroff` 字节，内部专用 bearer 的 HTTP 请求得到 200；不是仅检查旧 UI 状态，也没有使用裸业务 adapter 绕过受控入口。bind 阶段尚未进入 SDK enable，因此该反例没有访问生产或执行云授权。

测试先再次 seal、释放 bind、等待 enable/disable 收尾、确认 lease.close 恰一次及目标 EOF，再执行失败断言。`host-queued-enable-counterexample.log` 的实际结果是 **0 pass / 1 fail，exit 101**。私有 harness 位于 `fixture/examples/ram-only-workspace/desktop/src-tauri/src/host/tests.rs` 末尾 `independent_accepted_enable_waiting_state_lock_cannot_outlive_later_disable`。

必要修订：让公开 enable 的受理 reservation 与后续 disable 的优先 fence 在等待状态锁前建立一致顺序，且不能以新 token 覆盖已接受的关闭。修复必须同时证明实际 guarded ingress/upstream 关闭、迟到 factory/bind 仍被保留驱动并 close、本地业务继续。仅修改 public `localClosed` 或最终 commit 条件不足以关闭此反例。

## 其他审查与验证边界

- 新写的 3 个独立官方 owner 测试全部通过，exit 0：使用真实 `@logto/client@3.2.2`、受控 OAuth/JWK HTTP 与签名 ES384 token，验证退出等待已派发轮换且拒绝新请求、畸形已铸造回复保留并 poison 不重放、不可变绑定/过时代际/minValidity 边界不触发新 token HTTP。没有把自制 token owner 当官方实现。
- 原生 transport 的任务归属、未知 POST poison、回执相关性与取消 barrier，以及 LocalWorkspace/Gateway upstream 的 Host/Origin/cookie/CSRF/专用 bearer/WS 首帧/大小限制，已完整源码审查。开发现有 15 Rust/5 JS 测试与构建日志作为开发证据审计，**不计入独立通过数量**；因 blocking 已确认，未扩大到无关全套重跑。
- 独立重新计算官方 client 的 41 个安装文件 hash，与开发 registry/lock 证明一致；没有重新下载并声称独立验证 tarball。两个 lock 与全部新增 manifest 已审查，未新增 AGPL 依赖或复制协议源码。
- 外部 Gateway 的 `GatewayConfig.local_port/local_token`、start/port/close 与样例注入接口具有可装配映射；实际受控 `bind_guarded_ingress` wrapper、Noise、业务授权和 descriptor 发布仍由 010 实现验收。开发引用的外部 HEAD 此后已变化，但引用的 4 个相关 API 文件 hash 仍完全一致，私有证据准确记录；本 review 没有修改、链接或运行外部实现。
- 默认 factory unavailable 与默认 bind unavailable 都是明确未安装路径，不能作为样例远程完成证据。README 已明确桥接 unpublished、真实登录和完整样例边界，本 review 也不把 stub/生命周期 fixture 当 010 通过。

独立结果：**3 个官方 owner 正向/负向用例 PASS；1 个实际 Host 关闭反例 FAIL。** 受控 OAuth 不等于真实 Logto Cloud 登录；没有启动 App、手机、生产、账号或云资源动作，没有 push/deploy。未完成本次独立真实 Tauri UI/IPC、外部 Noise/public tunnel、远程 SPA、Wi-Fi/4G、Android、长跑或 24h 验收。修复 freeze 到位前保持 BLOCKED。
