# Rust 多端口会话、调用预算与诊断

本页描述项目 v0.9.0 / Rust SDK 0.4.0 为 issue #1 新增的 API。它不证明 CyberNote 真机同步故障已经修复。业务处理时间、请求确认、事务预算和业务重试仍由宿主负责。

## 同一 Peer 复用多个端口

`Client::connect()` 返回的 `Arc<Peer>` 已是通过端到端 mTLS、当前目录身份核对的多路复用连接。新增 `Peer::map()` 可在同一连接上创建多个独立的本地 loopback listener；`Peer::dial_with_options()` 则直接返回实现 `AsyncRead + AsyncWrite` 的 `PeerStream`。

```rust,no_run
use cybergrid_sdk::{Client, ConnectOptions, DirectStrategy, OpenOptions, OperationOptions};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

async fn prepare(client: Arc<Client>, device: &str) -> cybergrid_sdk::Result<()> {
    // client 已完成登录，目标设备已由其宿主明确开放远程访问。
    let deadline = Instant::now() + Duration::from_secs(45);
    let cancel = cybergrid_sdk::CancellationToken::new();
    let operation = OperationOptions {
        deadline: Some(deadline),
        cancel: Some(cancel.clone()),
    };
    let peer = client.connect_with_options(device, ConnectOptions {
        operation: operation.clone(),
        direct_strategy: Some(DirectStrategy::Parallel),
        ..ConnectOptions::default()
    }).await?;
    let open = OpenOptions { operation, ..OpenOptions::default() };
    let enrollment = peer.map_with_options(
        "enrollment", "127.0.0.1:0".parse().unwrap(), open.clone(),
    ).await?;
    let sync = peer.map_with_options(
        "sync", "127.0.0.1:0".parse().unwrap(), open,
    ).await?;

    // 宿主通过 enrollment.addr / sync.addr 访问本机映射。
    // 全部准备操作消耗同一个 deadline，成功连接不会在 deadline 到时过期。
    enrollment.close().await; // sync 和 peer 继续可用。
    sync.close().await;
    peer.close().await;
    Ok(())
}
```

共享会话的规则：

- 每个端口独立完成端口选择与可用性探测，不重复选择传输或执行 mTLS。探测确实会打开目标本机 TCP 服务；目标需接受随后无业务内容的正常关闭。
- 映射关闭或 Drop 仅取消自己的 listener、端口打开和转发流。关闭会话则取消全部共享映射和流，并等待本地任务退出；多次或并发 `close()` 等待同一清理完成。调用者丢弃 `close()` future 不会取消会话的后台清理。
- `PeerStream::close().await` 关闭当前流并等待其本地任务退出；Drop 发起关闭。`shutdown()` 仅关闭写方向，保留读取回复的半关闭语义。
- `Peer` 在断开、runtime generation 或协调租约失效后不自动复活。宿主为新请求显式建立新会话、重新创建映射；已经发送的业务字节不会重放。旧 listener 不跨代际保留。
- 关闭共享映射不代替关闭会话。运行时保留 Peer 的所有权，单独 Drop 最后一个宿主 `Arc<Peer>` 不代表已关闭，仍应显式 `peer.close().await`。
- 每个 Peer 最多 64 个共享映射；每个映射最多 64 个在途本地连接；mux 仍最多 64 个同时打开的流。达到映射限制明确失败，达到本地连接并发限制暂停 accept，避免创建无界任务。

旧的 `Client::map(device, port, local)` 保留独占 Peer、关闭映射即关闭 Peer、协调恢复后仅为新 TCP 重建连接的语义。旧 `Peer::dial()` 继续返回 `DuplexStream`；使用新 `PeerStream` 可获得每条流的本地相关性和状态。

## 调用级选项

| 类型 | 字段 | 语义 |
| --- | --- | --- |
| `OperationOptions` | `deadline` | `tokio::time::Instant` 绝对单调截止时间；可跨多次准备调用复用 |
| `OperationOptions` | `cancel` | 可选 `CancellationToken`，取消当前准备操作 |
| `ConnectOptions` | `direct_strategy` | 本次调用覆盖 Config 的 Sequential / Parallel |
| `ConnectOptions` | `direct_timeout` / `iroh_timeout` / `fallback_timeout` | 本次路径阶段预算，含该路径内的握手与身份核对；必须为正 |
| `ConnectOptions` | `only_transport` | 诊断用强制 WebRtc / Iroh / Cloudflare；失败不隐式切换其他路径 |
| `OpenOptions` | `port_open_timeout` | 本次端口打开阶段预算，默认 10 秒；必须为正 |

连接 deadline 涵盖初次目录查询、信令、路径选择、握手、认证后的目录核对。各路径阶段预算与剩余总预算同时生效；切换路径不会刷新总预算。没有显式 deadline 时，连接沿用 Config 推导出的总预算（默认顺序 57 秒、并行 49 秒，推导值最大 90 秒）。Parallel 仍只竞速 WebRTC 与 iroh；两者失败后进入 Cloudflare，没有增加延迟中继竞速策略。

取消、超时和迟到成功均关闭所拥有的连接／流；操作返回前等待已拥有的清理，因此实际返回时间可能包含截止时间之后的清理耗时。直接丢弃建连 future 会发起取消，无法通过 Drop 等待异步清理；对应诊断保留 `cleanupPending`，需要确定性退出时取消 token 并继续 await 原调用，或 await Client 关闭。不要把随意丢弃 future 当作已确认完成清理。

端口打开预算覆盖 mux 流分配、端口请求发送和响应等待。映射的 operation 选项约束初次探测与 listener 建立；成功后取消该 token 或经过其 deadline 不关闭已建立的映射。后续每个本地 TCP 的端口打开使用该映射的 `port_open_timeout` 和独立的映射存活取消令牌。转发中的业务 IO、首字节等待及整次同步预算由宿主另行控制。

调用选项不能覆盖 mTLS、证书指纹、当前目录核对、远程访问 gate、接口限制或 iroh 禁用 relay 的规则。

## 固定诊断与快照

`client.subscribe_diagnostics()` 返回独立于原字符串 Event 的结构化广播；`client.diagnostics_snapshot()` 返回最近 256 个不同诊断对象的最新状态，可在 broadcast lag 后恢复保留的状态。该缓存有界，可能淘汰较早对象，包括仍活跃的对象；它不是完整事件历史或所有活跃资源清单。长期观察特定资源应保留其 `watch_state()` receiver。

Peer、Mapping 和 PeerStream 均提供 `snapshot()` / `watch_state()`。watch 初值立即可读，后续 revision 单调增加，合并中间通知。新的 options API 操作失败返回 `Error::Operation`，可通过 `error.diagnostic()` 获取失败的本地快照；参数验证错误仍为 `Error::Config` 或 `Error::Protocol`。

快照包含：

- 随机本地 `id`，以及 `attemptId`、`sessionId`、`mappingId`、`streamId` 关联；跨端标识独立，不能直接用来对应另一台设备的日志。
- generation、固定 transport、stage、phase、elapsedMs 和 close reason。
- `retryable` 只表示是否可尝试建立新连接，不授予业务重放权限。
- `cleanupPending` 区分已关闭本地 gate 与本地清理完成。成功完成的连接尝试可为 Closed 且没有错误原因；活跃会话另有自己的 session 快照。

固定原因区分主动关闭、取消、总 deadline、阶段超时、keepalive 超时、远端 EOF／FIN、IO、协议、认证、端口不可用、路径不可用、协调失效、代际变化、远程关闭和接收背压。最先观测到的终止原因不会被后续通用 close 覆盖。原因描述本地观察，不证明跨端因果顺序；EOF 也不推测远端为什么关闭。普通端口探测或一个流关闭，不会被计为整个会话失败。

诊断不包含设备／用户身份、端口名、端点、令牌、私钥、任意 provider 错误文本或业务数据。原 `Event` 保持兼容，其 `peer_id/message` 不属于这个新诊断接口的隐私保证。Client 级 `StateSnapshot` 继续表达身份、协调、发布与可达性；诊断不替代宿主业务授权或业务 ACK。

## 背压与验证边界

smux 的接收循环不再等待某条流的应用队列空位。单流队列或总接收内存耗尽时，明确关闭该慢流、记录 `ReceiveBackpressure` 并通知对端 FIN，其他流和心跳继续读取。接收预算为 3 MiB，加上有限的单帧／duplex 缓冲；每帧最多 32768 字节。若控制 FIN 都无法入队，则关闭会话并记录背压，不能静默丢弃业务字节后继续使用该流。此版本未增加 smux 线协议级的逐流窗口协商，持续过载的流会被终止，宿主应持续读取并控制业务并发。

NOP 仍为 10 秒，入站活动检测仍为 30 秒；业务长时间计算但传输运行时正常调度时，心跳可以独立维持连接。阻塞整个运行时线程不由修改网络超时解决。

回归测试包含真实 WebRTC、iroh QUIC、内层 mTLS、smux、64 KiB／半关闭、本地 WebSocket 回退、Redis／Node 协调、多端口复用、deadline／取消、关闭／Drop、epoch 失效和慢流背压。可运行：

```sh
cargo test --locked --manifest-path sdk-rust/Cargo.toml -- --include-ignored
cargo clippy --locked --manifest-path sdk-rust/Cargo.toml --all-targets -- -D warnings
```

包含 ignored 的协调测试需要仓库已有 Node 依赖和真实 Redis；可通过 `REDIS_TEST_SERVER` 指向本地 Redis binary。测试不证明公网 Cloudflare 可达、Wi-Fi／蜂窝切换、两台真机持续同步或 CyberNote 的应用预算问题已修复；这些仍需使用新诊断做独立验收。
