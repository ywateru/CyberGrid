# CyberGrid 协调服务

Node 服务元数据 0.2.0 属于待发布的项目 v0.8.0；静态产品、finite-v2、浏览器目录与新 SDK 应按匹配策略部署。此版本准备不表示正式站点已升级，旧发布保持不变。

Node.js 22 / Docker 与 Vercel Functions 共用网站、Logto 控制台、只读管理后台和 `/api/v1/`。Redis 保存跨实例共享的短会话、设备、端口和信令；中央不接收或转发端到端业务内容。

完整配置见 [部署说明](../docs/deployment.md)，数据与原子性边界见 [架构](../docs/architecture.md) 和 [Redis 协调契约](../docs/redis-coordination.md)。Vercel 从仓库根目录导入，采用根 `vercel.json`；不要把项目根目录改为本目录。

```sh
cp .env.example .env
chmod 600 .env
# 填写 Logto 配置，Compose 使用 REDIS_URL=redis://redis:6379
docker compose up --build -d
```

Compose 包含带 AOF、健康检查和持久化卷的 Redis，Node 容器以非 root 用户运行。默认向宿主 loopback 发布 7400，由 HTTPS 入口提供外部访问。真实 `.env` 仅在运行时注入，不进入镜像。

直接运行需要 Node.js 22+ 和 Redis，无需 Python：

```sh
npm ci --include=dev
npm run build
npm start
```

`start`、`dev`、`owner` 都使用同一 Node 入口。生产构建后可 `npm prune --omit=dev`；`tsx` 是运行依赖。普通 Node 可使用 `REDIS_URL` 或直接 Upstash REST。Vercel 使用 `CYBERGRID_REDIS_TRANSPORT=vercel-kv`，直接读取 Marketplace 注入的 `KV_REST_API_URL` / `KV_REST_API_TOKEN`。旧 Vercel KV 已迁移为 Upstash Redis，该选择器提供环境变量兼容接入，不需要 `@vercel/kv`。配置分组、部署按钮和迁移步骤见 [Vercel 配置](../docs/deployment.md#vercel-functions)。

未设置选择器时只允许一组完整连接参数；显式选择后只采用所选组。所选参数缺失、选择器无效或存储不可用时拒绝请求，不改用其他存储，也不退回 RAM。`CYBERGRID_REDIS_PREFIX` 隔离环境，同 namespace 使用一致的 `CYBERGRID_LIMITS`。默认 Compose 使用内部 TCP Redis，不应设置 `vercel-kv`。

`CYBERGRID_PUBLIC_ORIGIN` 可留空；设置时固定使用该 HTTPS origin，留空时从本次合法访问地址派生回跳。可信代理必须覆盖单值 X-Forwarded-Host / Proto。`LOGTO_RESOURCE` 是固定 audience，不随请求 Host 改变。已有部署移除公开 origin 前须显式保存原 resource。

默认 SPA 回跳为 `/console/callback/`，须在 Logto 登记完整 URI。动态目录模式仅接受关闭 Device Flow 的普通 Native 应用，Lab 使用官方 SDK 网页登录。中央不管理应用，也不向 Logto 写入协调数据。

`GET/HEAD /healthz` 检查实例与 Redis，不调用 Logto；Redis 不可用时返回 503。长轮询最多 15 秒、全请求预算 25 秒，body 上限 100000 字节、响应上限 2 MiB。多个实例通过 Redis 原子校验协调；停止单个实例不会清空 Redis，也不更换 epoch。`memory-v1` 保留为客户端线协议标识。

管理 API 使用独立 `CYBERGRID_ADMIN_KEY`，未设置时返回 503。它只返回白名单 DTO，不展示会话密钥、信令正文或设备地址材料。前端不内置该密钥。

检查包括类型与站点构建、服务安全边界、Functions 适配，以及真实 Redis 的多实例注册、发现、信令、配额、过期与重启恢复。MemoryStore 仅作为测试夹具，不由生产配置创建。GitHub Actions 的服务检查与原生 SDK / Lab Release workflow 独立运行。


可选 `CYBERGRID_PRODUCTS` 把多种真实 Logto Native / SPA 客户端绑定到同一产品，不需要 Management M2M：

```json
{"version":1,"products":[{"id":"nativeDesktop","name":"Workspace","nativeClientIds":["nativeDesktop","nativeMobile"],"browserClientIds":["workspaceWeb"]}]}
```

产品 ID 必须是该产品的一个 Native client ID。同账号、同产品的 Native 客户端可发现设备和交换信令；SPA 仅能读取显式允许的产品。静态角色是管理员声明，须在 Logto 核实应用类型和关闭 Device Flow；JWT 仍验证固定 issuer/resource 及真实 client_id，不会凭静态配置证明 Logto 应用类型。配置存在即采用静态模式；错误、空值不会退回动态目录。JSON 最多 128 KiB、产品 1..32 个、每产品每类客户端 1..16 个，ID 限 `^[A-Za-z0-9_-]{1,21}$`、名称 1..256 字符；未知字段、重复项、跨产品重复 Native 和 Native/SPA 角色重叠均拒绝。

`GET /api/v1/auth/config?appId=<actual-native-id>` 保留实际 `clientId`，静态模式另给 `productId`、排序规范化策略的 SHA256 `policyVersion` 和当前允许的 `products:[{id,name}]`。SPA 配置可用 `/api/v1/console/auth/config?clientId=<actual-spa-id>`，不传时采用 `LOGTO_BROWSER_CLIENT_ID`。动态配置省略 `policyVersion` / `products`，保留原来的目录行为。

交换的 `appId` 必须与 token 的真实 Native client_id 精确一致；响应 `appId` 仍是实际 ID，`productId` 是设备/信令内部隔离键。会话记录真实 `authClientId` 和 `policyMode`，注册回执也绑定这两项。静态 `/apps` / `/console/apps` 返回 `source: "static"` 及允许的规范产品，应用行保持 `{id,name,type:"Native",isDeviceFlow:false}`；动态模式返回 `source: "logto"`。控制台 `/console/devices?appId=` 只接受规范产品 ID，不接受该产品其他 Native ID 别名。

每次请求检查当前策略；移除 Native、移动产品绑定或撤销 SPA 对产品的允许，均阻止旧会话使用该权限。切换 static/dynamic 模式需要重新交换授权；静态模式拒绝没有绑定信息的旧动态会话。动态模式仅在原始固定 SPA / Native 一对一范围兼容旧会话，设备/信令隔离不扩大。注销仍可撤销已不再获策略允许的会话。动态升级时，精确匹配且仍有效的旧未绑定注册回执可原子返回原会话密钥，不改写元数据、不新建会话；仅限 Native 一对一或原固定 SPA 信任范围。旧 SPA 回执本身未保存历史 client ID，部署者须保留原固定 SPA 信任关系。静态策略和新的已绑定回执不采用此恢复路径；已撤销、到期或被更高 revision 接管的旧会话不能恢复。

允许策略在各进程启动时固定。新配置只约束使用它的实例；部署时需切换并停用旧实例及旧公开端点，不能把单实例的撤销测试视为跨所有旧部署即时撤销。Logto 用户撤销也不因此变成即时同步。

未配置 `CYBERGRID_PRODUCTS` 时继续使用动态 LogtoDirectory，并要求专用 M2M。`LOGTO_MANAGEMENT_ORIGIN` 可固定 Management HTTPS origin，`LOGTO_MANAGEMENT_RESOURCE` 可固定其 HTTPS audience，默认分别为 issuer origin 和 Management origin + `/api`；获取 M2M token 始终使用 issuer + `/token`，不从请求 Host 或客户端输入派生。

`CYBERGRID_REGISTRATION_MODE` 默认 `legacy-v1`，保持旧客户端和永久注册 fence 容量限制。可显式设置 `finite-v2`，但必须采用新的 `CYBERGRID_REDIS_PREFIX` 并退役旧模式公开服务；同一 namespace 的不同模式实例启动会拒绝，不能靠新配置约束仍执行旧 Lua 的服务。模式非法或存储失败不会降级。

有限准入协议见 [注册准入契约](../docs/registration-admission.md)。Native auth config 广告 `registrationAdmission:{version:2,challengeSeconds:120,ticketSeconds:86400}`；控制台不需要准入。Native 先用资源 access token 和实际 `appId`、规范 `registrationKey` 调用 `POST /api/v1/auth/registration/challenge`，再携带原 `challenge` 和独立 UUID `requestId` 调用 `/api/v1/auth/registration/admit`。交换时携带返回的 `admissionTicket` 及原来的注册 revision/requestId。当前未接入该协议的 SDK 在有限模式下会收到 `REGISTRATION_ADMISSION_REQUIRED`，不会退回旧注册。

准入 challenge/ticket 按凭据处理，不记录日志；签名及身份绑定由 Node 验证，签发顺序、120 秒 challenge、24 小时 ticket、域 CAS 和提交有效性由 Redis 时间与原子脚本决定。有效 mint 回执重放可以跨过 challenge 的 120 秒，仍返回原 ticket，直至 ticket 到期。同 requestId 改内容、旧域高 revision、跨账号/产品/真实客户端/key 或改签名均拒绝。新域 mint 不终止旧 active session，只有注册成功才接管；同 key 续签维持原 logical device ID，旧 close 不能删除新代际。

有限模式的注册回执与 mint 回执保留到对应绝对期限，共享现有总幂等容量，接管不提前删除注册回执。session/设备期限不超过 ticket 到期，心跳不能延长绝对准入。只清理 `admissionVersion=2` 且所有已签票据到期、所有相关有效 session 已结束的 fence；全局域序列不回收或回绕，旧非零 challenge 在 fence 回收后仍因 CAS 失败而不能重建。旧永久 fence 不采用此 GC。

窗口中每用户/全局注册键、session、设备、签发限流及总幂等配额仍生效。有限准入避免历史永久累积，并不承诺任意频率无限重启；`CAPACITY_EXCEEDED` 的 retryAfter 也不保证届时已有空位。固定准入错误为 `REGISTRATION_ADMISSION_REQUIRED` / `EXPIRED` / `CONFLICT` / `INVALID`（后三项同 `REGISTRATION_ADMISSION_` 前缀），不会回显票据或身份。到期不证明此前没有成功，未知结果必须按原 requestId/challenge/ticket 重放并检查或清理原 session。

服务端支持可选 `CYBERGRID_ENDPOINT_PROVIDERS`，为自有入口配置精确 HTTPS origin（包含非默认端口）与精确 path；未配置时仅允许内置 `quick-tunnel`。错误、空值、未知字段、重复或空集合会拒绝启动，服务不会探测、fetch 或重定向到这些地址。Native 与 SPA auth config 都返回 `endpointPolicy:{version:1,policyVersion,providers}`，`policyVersion` 是排序后的规范策略 SHA256。内置 Quick Tunnel 规则固定，不进入 providers 列表，也不能被覆盖。

```json
{"version":1,"providers":[{"id":"workspace","origins":["https://remote.example.com:8443"],"paths":["/","/bridge"]}]}
```

原设备级 Quick Tunnel HTTPS 根地址保留，端口可另外发布受信 HTTPS 精确 path；设备级自有入口仍限根地址，并保留 authority 中的显式端口。新 `bridges` 是独立的浏览器能力，不能由旧 tunnelUrl 推断。每台设备最多四项，必须包含 id、protocol、providerId、显式规范 WSS url、installationId、32 字节规范 base64url publicKey、正安全整数 addressRevision、规范 ISO expiresAt，以及必填 limits。limits 上限分别为 maxFrameBytes=65535、maxStreams=32、maxPendingRequests=128。Quick bridge 可带显式安全 path；自有 bridge 的 authority 对应已配置的 HTTPS origin，path 必须精确获准。百分号、反斜线、dot 路径段、重复斜线、query/fragment、凭据或 URL parser 的隐式规范化均拒绝。

发布仍为完整快照，总公开 metadata 不超过 24 KiB。省略 bridges 表示撤回。URL、provider、protocol、installationId、公钥或 limits 变化必须增加 addressRevision；同一版本只允许保持内容并延长 expiresAt，且需要更高 publishRevision。新发布在 Redis 原子提交时检查 `now < expiresAt <= now+300000`。同 publishRevision 的完全相同重放可在 bridge 到期后确认，但不会延长地址期限；心跳租约照常更新，目录继续过滤已过期入口。同 incarnation 内，即使撤回也保留最多四个历史 id 的有界修订记录，只有新的注册 incarnation 重置；历史和内容摘要不会公开。每个入口的业务授权和加密握手仍由宿主 adapter 实现，中央不转发业务流量。

新增完整目录 `GET /api/v1/directory?cursor=<opaque>` 和 `GET /api/v1/console/directory?appId=<canonical-product>&cursor=<opaque>`。Native 固定读取自身产品，SPA 仍只读当前策略允许的产品。响应为 `{version:1,productId,epoch,endpointPolicyVersion,revision,sampledAt,reset,changed,devices}`，每次返回完整设备列表，设备及 bridge 按 id 排序。Console 新旧目录继续隐藏 iroh 地址；所有目录隐藏内部历史、摘要与会话标识。设备、端口或 bridge URL 不再符合当前实例入口策略时，新旧目录均过滤该入口；过期 bridge 也会移除。入口策略收紧需要切换并停用旧配置实例，不能约束仍运行旧策略的公开服务。

Store 在单次事务内校验当前 session，按已存储的 owner/product 选取设备，使用同一次 Redis TIME 判定 online 并返回 epoch、sampledAt。revision 不使用全局 Redis 变化序号，而是 `v1.<epoch>.<scopeTag>.<contentHash>`；scopeTag 使用协调 seed 分域 HMAC，绑定 owner、产品、入口策略与 Native/Console 投影，跨账号、产品、epoch、策略或投影的 cursor 会 reset。无 cursor 或有界格式无效也 reset；最大 512 字节。内容摘要基于最终安全投影，包含地址、key、limits、expiry、代际、online 等实际状态；仅 lastSeen 与 publishRevision 确认计数不参与。因此 `changed=false` 仍须处理返回的完整快照及最新发布确认。bridge 到期、lease 离线、元数据变化都会改变内容标识。第一版仅支持有界轮询，不为目录提供长轮询或增量事件；完整响应上限仍为 2 MiB。

Redis 设备记录内部以规范 JSON 字符串保留 bridge 描述，以十进制字符串保留桥接历史 addressRevision 与精确 publishRevision，避免 Lua cjson 对大安全整数重编码时舍入。公开 DTO 仍是 number，私有精度字段不会返回。没有精确字段的旧记录沿原值处理；此实现不能恢复升级前已经舍入的历史数字，也不会据此放宽修订检查。
