# Lab 远程访问开关（实施目标）

本页是 Tauri Lab 接入新 Rust 生命周期的界面和宿主合同，不代表当前发布客户端已经具备。网页登录授权后默认保持远程关闭，登录与开启是两个明确动作；本地测试业务服务继续在 loopback 工作。当前 Lab 使用的 CredentialStore 行为保持明确，不称其为 RAM-only 样例。

## 页面结构

```text
+------------------------------------------------------+
| CyberGrid Lab                  已授权 / 协调恢复中   |
+------------------------------------------------------+
| 本机 Mac    远程访问：关闭                            |
| [开启远程访问]                                       |
| 本地服务可用；远程端口尚未发布                        |
| 身份 已授权 | 协调 已连接 | 发布 本地意图             |
+---------------------------+--------------------------+
| 同账号设备                | 消息 / 校验              |
| 在线 / 离线 / 未知         | 远程关闭时禁用           |
| [刷新]                    |                          |
+---------------------------+--------------------------+
| [退出并撤销授权]          云端撤回：已确认 / 待确认    |
+------------------------------------------------------+
```

开启后主按钮换成“关闭远程访问”。不要用“已登录”或设备租约 online 表示端口已确认或业务已授权。SDK 快照中的 identity/coordination/publication/reachability 与本机 gate 分开呈现；端口显示“仅本地意图”“服务已确认”或“状态未知”。底部云端撤回待确认不会使本机 gate 再打开。

## 交互和生命周期

| 操作 | 宿主行为 | 完成条件 |
| --- | --- | --- |
| 首次网页登录/恢复授权 | 沿用官方 Logto 流程，建立 SDK session，启动本地服务并记录 expose intents；不调用 enable_remote | 已授权、远程关闭；端口未对外发布 |
| 开启远程访问 | 明确调用 enable_remote，串行化/代际隔离；需要新 Client 时沿同授权 owner 恢复 session 和 intents | gate 开放与 publish 确认分别显示；失败保持真实状态 |
| 关闭远程访问 | 先 SDK disable_remote 让本机 gate/远程流关闭，再处理协调撤销；不停止 LocalServices，不退出 Logto | local_closed=true 即显示关闭；revocation Pending 独立显示 |
| 再开启 | 新建 Client，沿已有授权 owner（或既有 CredentialStore 唯一 owner）授权并重新记录 intents，再显式 enable | 不复活旧 Client/peer，不凭旧缓存显示 ready |
| 退出 | 分别完成本机关闭、协调撤销与原授权退出流程，保留 pending grant owner | 继续保留原旋转/checkpoint 的安全结束边界 |

远程关闭后 map/connect/send/probe 等入口均明确拒绝或禁用；“重新连接设备”不能隐式开启远程。LocalServices 的 listener 与本地业务状态不能被远程 disable 的 runtime cancellation 顺带终止。若测试消息 ACK 需要当前 deviceId，显式更新其运行标识，不能为了更换 SDK Client 关闭重启本地业务连接。只有真正退出 App/测试业务时关闭这些本地服务。

关闭和重新开启采用 host generation 与单飞控制；旧 watch/poll/握手/enable 的迟到结果不能覆盖最新关闭状态。SDK watch 先读取初始快照，旧 broadcast Event 仅作辅助，不靠轮询字符串推断服务已确认。关闭后不再可用的协调读取标 unknown；允许展示账户摘要与本地服务状态，不能沿用缓存声称远端可达。远程关闭与完整退出按钮分开。

## Android ADB 控制

现有登录连接配置的策略下拉框增加 WebRTC 候选与 iroh 候选测试项，不另建页面或控制通道。可选 `directTransport` 为 `auto`（默认）、`webrtc-only` 或 `iroh-candidate`；旧配置缺少此字段仍使用默认值。WebRTC 候选仅关闭 SDK 的 iroh；iroh 候选将 WebRTC 等待预算压到接近零并保留 iroh，不声称协议层禁用 WebRTC。Cloudflare 回退仍可发生，必须按实际消息 ACK／校验结果的 transport 判定直连，不能按选项名称判成功。原 `directStrategy` 和 `forceFallback` 保留；强制回退优先于候选项。GUI 与 ADB 使用同一 `LabConfig`，不改默认公开服务／应用或凭据存储，也不接受 OAuth 材料。

沿用现有 debug-only、run-as 私有目录、controlEpoch/sequence/requestId 幂等控制；新增 `enable-remote` 与 `disable-remote`（空 payload），返回当前状态及 localClosed/coordinatorRevocation。不得新增可在正常 release build 使用的控制通道或 token 输入接口。重复 enable/disable 幂等；旧 control epoch、乱序及改内容重放继续拒绝。状态 JSON 仅包含公开状态，不含 Logto token、注册 key、带凭据 URL 或内部错误。

控制循环不能串行等待云操作而占住邮箱：durable claim完成后，先发布新highwater，再移走已claim的mailbox，由保留所有权的任务继续执行。每200ms输出当前safe_view；云撤回未结束时也必须反映localClosed和Pending，不把文件旧状态当操作现状。最多一个普通操作加一个优先disable操作在途；普通槽忙或已有优先关闭时，合法next-sequence请求仍claim/finish为固定BUSY并消费该序号，不执行业务、不保留无界队列。优先disable不得等待普通refresh等网络操作；不得覆盖未claim请求、丢弃授权轮换/清理future或重复派发未知结果操作。

结果按原controlEpoch/sequence/requestId归属，完成先后不决定高水位。status额外提供最多16条已完成结果摘要，仅含版本、epoch、sequence、requestId、ok与固定code；不保存历史state、token或内部错误。result.json只推进最高已完成sequence，较低sequence晚完成不能覆盖它；CLI按原请求从result或摘要匹配结果，并单独使用当前live state。能证明同claim/digest仍由本进程future拥有时，pending相同内容重放只等待原结果，不重派、不发布虚假终态；无法证明owner或完成记录已回收时返回OUTCOME_UNKNOWN。active claim改内容返回SEQUENCE_CONFLICT，已缓存完成结果保留原幂等/改内容拒绝。原epoch、gap、文件0700/0600及大小限制继续保持。持久写失败停止新受理，但已派发future继续安全收尾；不能因输出失败abort其唯一owner。

## 状态和布局

开启等待时禁用重复开启，关闭按钮仍可用；关闭优先，不被正在等待的云操作阻塞。错误分成固定提示与本机关闭/云端待确认状态，原始 OAuth/provider 错误不进入产品 UI。键盘 Enter 只触发当前聚焦按钮，不因为表单默认 submit 开启；Tab 可访问两个独立控制，按钮给出完整 accessible name。窗口较窄或 Android 竖屏时本机控制置顶、设备和消息分上下排列，不出现仅悬停可用操作，不新增 modal。

## 验收

用真实 LabState→Client→本地 HTTP 协调服务→mTLS/stream 的路径验证登录后未发布、明确开启、关闭在线/离线均停止远程流而本地 listener 继续、再开启与旧结果隔离。前端类型构建、Rust/ADB 控制测试及 Python CLI 测试通过；运行环境缺少 Android 时明确记录仅主机测试，不能声称完成手机验收。
