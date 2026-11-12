# 浏览器安全桥接与目录扩展（实施目标）

本文件承接 CG-01/06/07/08；是否已实现以任务评审为准。CyberGrid 仍只协调身份和设备元数据。浏览器业务加密、工作区权限、请求与 PTY 语义由显式受信宿主适配器提供，不新增中央业务代理。

## 能力与入口

保留现有设备级 `tunnelUrl` 与端口级 `ports[].tunnelUrl`，它们不自动代表浏览器安全桥接。新增设备发布字段：

```typescript
interface BrowserBridge {
  id: string;                    // 1..64 ASCII identifier
  protocol: string;              // 1..128 ASCII identifier, exact adapter match
  providerId: string;
  url: string;                   // wss, trusted origin and exact allowed path
  installationId: string;        // 1..128 ASCII identifier, display association only
  publicKey: string;             // canonical base64url, exactly 32 decoded bytes
  addressRevision: number;       // positive safe integer
  expiresAt: string;             // canonical ISO timestamp, max 5 minutes at publication
  limits: {maxFrameBytes: number; maxStreams: number; maxPendingRequests: number};
}
```

标识符限 `^[A-Za-z0-9_-]+$`；protocol 另允许 `.`。每台设备最多四个 bridge，id 不重复，总 metadata 仍受 24 KiB 限制。limits 全部为正安全整数，maxFrameBytes≤65535、maxStreams≤32、maxPendingRequests≤128；宿主声明实际更小限制，adapter 需提供相同或更严格的执行边界。发布意图/服务确认 revision 使用既有机制；同一设备同 id 的 URL、provider、protocol、installationId、publicKey 或 limits 变化必须增加 addressRevision，较小值拒绝，同值改变这些身份/地址/能力字段拒绝。同地址版本允许仅延长 expiresAt，须带更高 publishRevision 并受五分钟窗口限制；重复旧快照不延长既定期限。相同版本缩短到期时间拒绝，显式缩短需新地址版本并关闭旧 channel。正常 expiry-only 续期不强迫活跃连接周期重连。撤回再发布同 id 不能绕过当前 incarnation 内的修订检查，其历史最多四个不同 id；需要更多 id 的宿主应重新注册，不引入无界表。

新 publishRevision 在原子提交时按 Redis TIME 校验每项 `now < expiresAt <= now+300000`，MemoryStore 使用其注入时钟；不能只在 Node 解析时检查。相同 publishRevision/相同完整内容的幂等确认允许原 bridge 已到期，但不延长其地址期限；原有心跳租约语义保留，读取仍过滤过期入口。为保留无bridge旧回执，缺省与空bridges使用原无bridge摘要形状，非空才扩展摘要。修订历史为设备记录的私有有界索引，最多四项，保存比较所需的id/revision/身份内容hash/期限，不复制完整URL和描述；公开24KiB限制针对当前完整发布，历史不对外返回。幂等重放不能清空历史，仅新incarnation重置。

桥接身份与 native mTLS certificateFingerprint 分开。installationId 只允许在已验证 owner/product 内用于展示关联；相同 installationId 加不同公钥仍是不同安全身份，SDK 不合并、自动信任或替换目标。首次公钥发现信任已固定的协调服务及其授权目录；若宿主有独立 pin，应同时验证，不能认为首次目录发现已具备独立带外信任。

## 受信 provider 策略

新增可选 `CYBERGRID_ENDPOINT_PROVIDERS`：

```json
{"version":1,"providers":[{"id":"workspace","origins":["https://remote.example.com"],"paths":["/","/bridge"]}]}
```

策略最多 32 KiB、16 个 provider，每项最多 16 个 origin 与 16 个精确 path。id 1..64；禁止重复、未知字段/版本、空列表。origin 必须是规范 HTTPS origin，无凭据/query/fragment/path；path 以 `/` 开始，最多 256 字节，ASCII 字符仅允许 `A-Z a-z 0-9 _ ~ . ! $ & ' ( ) * + , ; = : @ / -`，不含 `%`、反斜线、控制字符、重复斜线、`.`/`..` 路径段或 query/fragment。显式非默认端口属于 origin，不能隐式放宽；写出的默认端口不属于规范 origin。配置错误不回退。

内置 `quick-tunnel` provider 不能被覆盖；设备和端口的旧 HTTPS URL 仍只接受 trycloudflare.com 合法子域根地址。新增 bridge 的 WSS URL 可包含显式规范 path，使用与自有 provider 相同的 path 安全语法/256 字节上限，不接受 query/fragment/credentials/显式端口；整个 endpoint 必须固定给 adapter，不能由它任意改写。端口 tunnelUrl 也可使用被自有 provider 明确允许的 HTTPS origin/path；bridge 使用同策略下的 WSS URL。设备级 tunnelUrl 首版仍仅为 HTTPS 根地址，显式 origin/port 必须完整保留；原生 `cybergrid-v1` 的既定派生规则为同 authority 的 `wss://authority/connect`，provider 须将该固定路由转交 SDK listener，不支持任意设备级 path 前缀。Rust dial_tunnel 必须按 URL authority 构造，不能只取 host_str 而丢失端口；自定义 path 只用于端口 HTTP 或 bridge 显式地址。绝不跟随重定向、猜测业务路径或将 device URL 当成 port URL。服务端不为验证地址而 fetch 用户 URL。

公开 Native/console 配置增加 `endpointPolicy: {version:1,policyVersion,providers:[...]}`。policyVersion 为 SHA256（64位小写hex），规范字节为 UTF-8 的 `JSON.stringify(["cybergrid-endpoint-policy-v1", entries])`，entries 按 id ASCII 排序，每项为 `[id, originsSorted, pathsSorted]`，两个集合均按 ASCII 排序。该固定规则标签包含内置 Quick Tunnel 语义。未配置自有 provider 时公开 providers=[]，内置 Quick Tunnel 仍可用。Quick Tunnel 是固定内置项，不由公网响应改写其规则。Rust 和 JS 在固定可信服务配置链路获得相同策略并再次校验；宿主可增加更严格 allowlist，不接受客户端自行扩大服务策略。正式自有 URL intent 需要已加载并验证的策略，策略未取得时明确 unavailable，不为离线意图隐式放行；不带地址的 intent 及内置 Quick Tunnel 保留原行为。

地址须是规范序列化形式，不能在 URL parser 消除 dot segments、默认端口、编码或反斜线后绕过原始文本限制；旧 HTTPS 根地址允许既有的可选末尾 `/`，返回规范 origin。发布后的地址若不再受当前策略允许，目录读取标为不可用/移除该入口；新 directory 与原有 devices/console/devices 均过滤，不能依赖仅在首次 publish 时校验。

## 版本化完整目录

新增 `GET /api/v1/console/directory?appId=<canonical product>&cursor=<opaque>`；console 仍无设备信令权限。Native 增加自身产品的 `GET /api/v1/directory`。返回：

设备投影沿用既有权限边界：Native目录可含同owner/product的iroh地址，console新旧目录均剥离iroh字段，不因使用共同快照结构扩大权限。所有公开接口均剥离session hash、snapshot digest与私有bridge修订历史；admin保持原显式白名单，不新增原始地址/密钥输出。下列devices表示当前角色允许的完整设备投影；console类型为不含iroh的设备列表。

```typescript
interface DirectorySnapshot {
  version: 1;
  productId: string;
  epoch: string;
  endpointPolicyVersion: string;
  revision: string;       // opaque scope/content-bound cursor, not a signal cursor
  sampledAt: string;
  reset: boolean;
  changed: boolean;
  devices: Device[];      // complete snapshot, including incarnation/publishRevision/bridges
}
```

第一版每次返回完整快照，提供有界轮询，不实现增量或占用 Functions 长轮询。revision 是有界不透明内容标识，绑定 epoch、已验 owner、product 及经过策略过滤的完整有效快照；不向客户端公开全局 Redis revision 或其他账号的变化量。设备按 id 排序，桥接按 id 排序；lease online/offline、入口过期/移除、地址/公钥/代际及实际发布内容变更均影响内容标识。重复心跳的 lastSeen 与 publishRevision 确认计数不是目录拓扑变化；内容 hash 排除这两个字段，但完整 devices 仍返回实际采样值。端口、bridge（含 expiresAt/limits）、身份、地址和在线结论等实际变化不能排除。changed=false 也返回完整快照，SDK仍处理其中较新发布确认与采样期限，不能跳过它；客户端不能用 revision 推断两个快照时间先后。

新旧Native/console目录均保留2MiB完整响应上限，新增快照包装字段计入总大小。超限明确返回 `DIRECTORY_TOO_LARGE`；不截断设备、端口或桥接后宣称完整，也不在本版隐式引入分页游标。

cursor 为 `v1.<epoch>.<scopeTag>.<contentHash>`：scopeTag 是协调 seed 分域 HMAC-SHA256；先以32字节原始seed为key、UTF-8标签 `cybergrid-directory-scope-v1` 为message派生32字节key，再对 UTF-8 的 `JSON.stringify([epoch,userId,productId,endpointPolicyVersion,projection])` 计算HMAC；projection固定为 `native-v1` 或 `console-v1`，跨角色投影的cursor必须reset。contentHash 是当前角色最终安全投影的规范有效快照SHA256，两个最终摘要均以小写 hex 编码；不泄露 owner 明文或seed。无 cursor 或 epoch/scope/结构不匹配时 reset=true，完整替换；同 scope 的有效旧 cursor 可返回 reset=false、changed=true，仍完整替换；同内容 changed=false。内容 hash 不充当授权证明，cursor 不能扩展目录权限，最大 512 字节。Store 的原子采样操作同时检查 current session 并返回本产品设备、epoch 与 Redis TIME；不能将 devices()+getRevision()+Date.now 拼成一次快照。失效/不可达时 SDK 输出 unknown，不使用缓存继续声明 ready。

JS `watchDirectory(productId,listener,options)` 默认 2 秒、范围 1..30 秒，串行读取、立即首个快照，失败退避最多 30 秒；取消/注销立即停止等待，迟到结果不能更新。事件分别为 snapshot/reset/unknown，包含 SDK generation 与本地递增事件 revision；与目录不透明 revision 区别命名。切用户/产品/epoch 必须 reset，不共享缓存。订阅回调异常隔离。目录 endpointPolicyVersion 改变时，先将入口可用性置 unknown，沿完整可信配置校验链有界重取对应策略后再接受新地址；失败不继续使用旧 ready。不能直接把目录中未验证的 origin 添加到 allowlist。

## 安全桥接扩展

JS 新增独立泛型适配入口。SDK 用当前可信完整目录定位 product/device/bridge，要求在线、当前 incarnation、未过期、受信地址与已支持 protocol；由宿主注入显式受信 adapter。adapter 接收不可变目标描述、取消信号和剩余连接预算，不接收 CyberGrid bearer 或 Logto refresh grant；业务授权通过宿主私有闭包提供。

adapter 在实际握手验证预期 publicKey/installationId，返回认证 peer 身份、宿主业务操作接口、关闭句柄和关闭通知。SDK 再比较返回的身份及 descriptor 绑定，拒绝明文、未知协议和不匹配结果；错误不得回显 token/原始 URL。wrapper 只暴露有 gate 的 request/stream/close 接口，业务参数与结果可泛型透传，不向调用者交出可绕过关闭的底层 channel/adapter；流的读写和在途请求也受关闭信号约束。仅声明 `encrypted=true` 的假实现不能通过真实集成验收。SDK 信任宿主选择的 adapter 实现，其类型声明不能替代密码学握手审查。

wrapper 按 limits 限制同时活跃的请求和流，不排无界等待队列；adapter 负责其已认证帧协议的最大帧、分块、背压和业务总大小边界，不能让 generic payload 绕过它。SDK 透传字节流时自身检查帧上限及最多 1MiB 的单流缓冲，使用背压而非静默丢帧。取消的请求在 adapter 未确认停止/结果尚未结束时仍计入未完成操作上限；满额明确失败，不能通过重复取消无限累积。协议特有的业务限制由宿主声明并执行，SDK 不解析工作区语义。

一次连接总预算默认 60 秒，1..120 秒可配置，包含目录/恢复和 adapter 握手；取消让调用者立即结束，wrapper 同步封闭 request/stream gate 再异步 adapter.close，迟到成功必须异步关闭，清理最多 5 秒并保留未确认状态。channel 不重放业务请求。每次目录采样用本地单调请求起点加 `max(0,expiresAt-sampledAt)` 计算保守到期 deadline，另设独立定时器，不能依赖下一次轮询或本地可回拨的 wall clock；只有有序、同绑定、较新 publishRevision 的确认续期可延长它。握手结束再核 generation/incarnation/key/addressRevision 与当前 deadline，过期/取消结果不得交付。保持目录监测，目标失去在线状态、epoch/代际/公钥/地址修订变化、入口过期或目录不可达时先关闭该 wrapper 的业务 channel；恢复需宿主显式发起新连接。宿主按业务授权 ACK 到期/拒绝关闭 channel，SDK 不把 Logto/协调在线等同于业务允许。

AnyWork 已有 Noise IK WebSocket 实现可通过其公开接口作为外部 host adapter 进行真实互通验证；其 AGPL 源码不复制到 CyberGrid，也不作为隐式可再分发依赖。已静态核对的宿主 API `RemoteTransport.connect` 接受 HTTPS root 并按其既定协议连接同 authority 的 `/remote/v1/ws`；adapter 必须先确认显式 descriptor URL 精确为该 WSS endpoint，才可将同源 root 传给此 API，不能把任意 descriptor path 忽略或猜测。验收需 pin 宿主 commit 及实际文件 hash（本地 WIP 不能只凭 HEAD 识别）。通用扩展本身不引入新密码协议。验收需记录真实握手、公钥/安装绑定错误、授权续期/拒绝、地址变更/到期、取消迟到关闭及业务密文；手机浏览器另行实测。
