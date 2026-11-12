# RAM-only 共同样例与验收合同（实施目标）

本页定义 integration-010 的模块与创建链。交付状态见 [覆盖表](plan/analysis/integration-progress.md)，本合同不代表样例、真实登录或浏览器长跑已经完成。样例验证 SDK 与外部安全桥接的实际组合，不实现 AnyWork 的工作区产品，也不复制其 AGPL 传输实现。

## 模块和所有权

公开样例放在 `examples/ram-only-workspace/`；验证用的外部桥接装配、构建与私有证据位于源码树外，不进入源码导出或下载包。

| 模块 | 输入 → 输出 | 创建者与状态所有者 |
| --- | --- | --- |
| desktop | 预置公开配置、官方 Logto 登录 → Rust Client、发布意图与本地控制 | Tauri 2 宿主；进程内 token owner、原生私钥和业务服务独立存活 |
| local-workspace | 本机一次性启动授权 → 受限本机会话、合成业务与账号/远程摘要 | desktop 创建 loopback listener；远程开关不停止它 |
| gateway-upstream | 外部Gateway专用RAM bearer → 受限合成业务HTTP/WS | desktop创建独立loopback listener，与local-workspace共享业务状态，账号/控制接口不在此入口 |
| web | 预置 SPA 配置、官方 Logto 登录 → JS 目录、设备选择与受控业务连接 | 浏览器页面；SDK/授权状态按用户、产品及页面代际隔离 |
| bridge-adapter | 已验证 descriptor、宿主私有业务授权 → 已认证业务接口 | 显式选择的外部宿主实现；CyberGrid wrapper 控制取消/失效，不交出底层句柄 |
| acceptance | 实际上述模块及合成请求 → 时长、计数、原因码与关闭证据 | 独立验证进程；不收集 token、私钥、业务正文或本机文件路径 |

desktop 与 web 分别固定可信 server、issuer、真实 Native/SPA client、resource 和产品；配置由部署/构建阶段注入并经过 SDK 完整校验。正常用户界面不提供 token、隧道地址或端口输入框。它们使用独立官方 Logto 客户端登录同一账号，不相互转交 refresh grant。

## 创建与调用链

```text
desktop 进程启动
  ├─ LocalWorkspace ──受限本机会话──> 本地 Web（离线可用）
  └─ 用户登录 ──> 官方 Logto RAM owner
                    └─ AuthProvider ──> Rust Client（remote gate 关闭）
                         └─ 用户开启 ──> finite-v2 准入/发布确认
                              └─ 外部业务桥接 + 受信入口
                                   └─ bridge descriptor 定期确认/续期

远程 Web / Android Chrome
  └─ 官方 Logto SPA owner ──> JS SDK directory/watch
       └─ 用户选择设备/确认身份 ──> 受控 adapter ──> 实际加密握手
            └─ 宿主业务授权 ──> 同一本地合成工作区
```

桥接业务授权与协调 session 分开。外部桥接必须验证当前账号与允许 SPA client；安装标识不构成授权。业务加密钥匙与 Native mTLS 钥匙分开，目录 descriptor 必须准确发布实际握手公钥和 installationId。新进程产生新公钥后，远端不能仅凭相同安装标识自动接受；首次确认/换钥提示由样例明确实现。

desktop同时拥有原生SDK与外部Gateway两条远程入口的开关/取消权；Rust gate不控制独立Noise WebSocket。外部Gateway仅在明确开启后创建，受本次host generation约束。关闭、退出或开始重新登录时，先同步封闭宿主复合gate，驱动SDK关闭与外部Gateway取消/close，终止其已有/在途握手、业务流及续期，再等待慢的云撤销。不能暂停持锁关闭future后先等待同锁的另一清理操作。已关闭generation的迟到Gateway启动必须立即关闭、不得发布。

样例的开启操作同样是复合动作：没有注入可用Gateway factory时，在开启业务gate/发布前返回不可用并保持关闭。存在factory但bridge尚未确认发布时只显示准备中，不能只开放原生路径就声明浏览器可用；任一阶段失败先关闭两条入口并保留清理owner，本地服务继续。

factory返回的lease先声明实际loopback ingress，宿主校验后创建受控TCP gate，再调用lease的`bind_guarded_ingress(address, cancel)`，让受信私有wrapper把公开入口仅绑定到该gate。默认实现明确返回`GATEWAY_BIND_UNAVAILABLE`，不以未配置的绑定冒充成功。宿主持续持有并驱动bind future；关闭或代际改变时先封gate和内部upstream，bind迟到完成也不得commit或发布，待其收尾后消费lease执行close。具体Rust签名及实现证据由样例README维护，010仍须验证实际外部provider通过此创建链。

### 010装配接口与发布owner

完整Tauri命令、主窗口校验、IPC、退出和owner创建链只保留一份；提供`run_with_factory(config: BuildConfig, factory: Option<Arc<dyn GatewayFactory>>)`供树外受信main注入实际factory，原`run()`仅以默认配置和None调用它。不能在私有测试程序复制一套不同宿主来代替实际样例。

lease的`GatewayIdentity`报告真实握手公钥和安装标识，另以不可变capabilities报告精确protocol和实际limits（不能大于实际外部实现）。必需的`endpoint_updates()`返回`watch::Receiver<GatewayEndpointState>`；状态为`Pending`、`Available(GatewayEndpoint { revision, provider_id, url })`、`Unavailable { revision }`或`Closed { revision }`。元数据不含私钥、bearer或账号token。首次Available的revision至少1，所有有效revision为安全整数；地址或可用性变化严格递增，重复revision不能改变内容。Host在bind前订阅，允许watch合并中间值，但只处理本generation最新有效快照；缺少Available不能用虚构URL补齐。watch关闭或不可用立即封闭复合入口并停止发布续期。

endpoint revision只表示wrapper事件序列，不能直接替代descriptor的addressRevision。Host按真实协调incarnation和bridge id维护地址版本；公钥、installation、provider、URL、protocol或limits任一变化均增加addressRevision，同身份/地址仅增加expiry和publishRevision。新lease不能在同incarnation内把历史清零；可以在确实新建并确认SDK运行身份后进入新incarnation，不能以本地计数器假装服务端已换代。

发布owner位于实际Host：核固定产品/可信endpoint policy与当前lease身份后，经Rust SDK保存意图、开启并确认实际发布；核对当前device/incarnation、完整descriptor及确认revision后，最多显示“可尝试连接”，不显示业务已连接。默认每60秒更新约180秒期限，仍受服务端五分钟上限；确认ACK不延长原地址版本以外的内容。owner从请求起点及可信serverNow保守计算本机单调hard deadline，迟到/旧代确认不能延长期限或恢复ready。endpoint变化、publisher停止、watch结束或到deadline仍未确认续期时，先同步封Native/TCP/upstream并取消更新，再驱动撤回、disable及owned cleanup；不以目录自然过期或JS轮询代替本机关闭。本地工作区始终继续。

发布确认按同一意图已被 SDK 确认的 revision 建立下界：发起目录读取前固定该意图与已确认 revision，目录中的同一 device/incarnation 必须 online、revision 不小于该下界，且完整 descriptor 精确相等。独立 heartbeat 在目录采样之后推进 revision，不应单凭其与后续最新 revision 不相等否决有效快照。读取后仍须验证当前意图及其确认未改变、发布 ready、session/epoch/product/generation 一致、未取消且期限有效；此规则不接受较旧意图、旧身份、不同 descriptor 或过期确认，也不改变失败关闭策略。

实际私有wrapper必须持有Gateway、真实cloudflared子进程和endpoint producer。bind只启动指向宿主gate的真实隧道，子进程退出发Unavailable；close取消、等待并回收子进程/Gateway，迟到bind仍收尾。把Quick域名改写到loopback的fixture只能证明本地协议互通，不能计为实际Cloudflare公网验收。

本机关闭结论覆盖两条入口，不能仅凭目录撤回或远端JS检测失效显示完成。外部公开close接口若不能拒绝新握手并终止活跃连接，验收装配必须增加本机受控传输外壳及取消句柄，或列为阻塞；不通过停止LocalWorkspace来达到关闭远程。实际测试须在远端停止目录轮询、云撤销响应被扣住时仍观察到Gateway流终止、新连接拒绝和本地业务继续。

## Native 官方授权 owner

desktop 使用官方 `@logto/client` 及自定义内存 adapter，复用已核对的系统浏览器/loopback callback 与固定 JWT 验证边界。与当前 Lab 的文件 grant 交接不同，本样例不调用 CredentialStore 或把 refresh grant 导入 Rust。原生侧只通过 `AuthProvider.access_token_for` 获取所需资源 access token；官方客户端和刷新权一直由同一个 JS owner 持有。

所有 token 调用由该 owner 单飞串行执行，核对请求固定 issuer/client/resource/scopes，满足 minValidity；需要强制刷新时调用官方 `clearAccessToken()` 后再获取指定 resource，不把 organization 参数当刷新参数。旧 owner 的派发操作必须收尾后才允许新 Client 使用同一 owner，不同时消费刷新 grant。取消用户等待不会丢弃已发出的轮换操作。

自定义 transport 在已派发 token 请求但结果未知时将 owner 标为不可继续轮换，不能下一次自动重放旧 refresh grant。已收到的新 grant 在进一步验证失败时仍由唯一 RAM owner 保留，供安全收尾；错误和诊断不含响应原文。需要重新登录时先关闭远程访问并明确结束旧 owner。重新开启远程新建 Rust Client、沿同 owner 授权；本地服务与本地已建立连接保持原样。

Tauri token 响应 IPC 只允许应用自身主 WebView，按 requestId/owner generation/SDK generation 关联；远程页面、本地浏览器与任意外部窗口均不能调用该接口。退出进程清除 RAM，下一次启动重新授权、重新开启；不以隐藏文件、Web Storage 或钥匙串补自动登录。安装标识可持久化，但不保存账号 token、注册私钥或业务加密私钥。

样例“本机退出”先关闭远程并等待已有token派发/transport收尾，再清除官方owner内存和账号摘要；它不代表Logto全局SSO退出或云端grant撤销。没有实际执行/确认这些云动作时不显示已完成，后续登录仍可能使用系统浏览器SSO。

## 本地 Web 边界

本地业务在登录前启动，绑定 `127.0.0.1` 的随机端口；不监听全网卡。公开静态页面不授予工作区访问。用户从 desktop 点击“在浏览器打开”时，宿主生成 RAM 中的256位一次性启动凭据，最长30秒、仅兑换一次，以 URL fragment 交给本地页面；页面先清除 fragment，再向同源兑换接口提交，不把秘密放 query 或日志。

默认入口继续使用系统浏览器。树外受信装配可在进程创建时注入独立的本地浏览器 opener，用于将这一次初始打开直接路由到指定的测试浏览器；现有 `run()` 和 `run_with_factory` 的默认行为保持一致。opener 只接收同一公开 `local_open` 命令现场生成的 URL，不新增返回 URL 的 IPC、不经网页或网络中转、不读取或复制既有会话。它不能替换 OAuth 浏览器入口，也不能从页面参数选择执行程序或 profile。失败返回固定错误，不把含 fragment 的 URL 写入日志、命令行或文件；测试装配须通过私有进程内调用或受控 pipe 直接交给自有浏览器，并保持原主窗口调用者校验、一次性兑换和本地会话约束。

兑换与后续 API 严格检查实际 loopback Host/端口、精确 Origin；不信任转发头，不开放 CORS，不接受任意localhost别名。兑换生成进程内受限会话与 HttpOnly、SameSite=Strict、无Domain 的独立 cookie；会话还需匹配本次服务实例，普通跨端口 cookie 不构成授权。修改操作校验独立 CSRF 值和Origin；拒绝跨源请求及 DNS rebinding。HTTP仅用于loopback，cookie不声明跨公网传输安全性；本机其他进程与同源脚本仍属于本机信任边界。

浏览器同源GET可能不携带Origin，因此受保护本地HTTP接口统一使用POST（含状态读取），不为缺Origin的GET开通豁免；静态GET不返回私有状态。兑换后bootstrap POST可凭会话cookie和精确Origin取得CSRF值，其余受保护调用还需该值；本地WS使用其实际Origin、会话及首帧CSRF验证。HTML禁止被其他页面frame嵌入，私有响应no-store/no-referrer；这些边界不靠隐藏随机端口代替。

本地 API 只返回当前账号的必要摘要、当前电脑和远程开关状态；不返回 Native refresh/access token、注册钥匙、业务私钥或本机长期 bearer。退出 Logto 后清除摘要但保持本地业务，关闭远程后继续本地业务。浏览器刷新可复用仍有效的本机受限会话；进程退出后原会话失效，不自动恢复云端授权。

本地摘要的 `remote` 直接读取 Host 当前同步 gate，固定返回 `on` 或 `off`；同步封闭 gate 后立即为 `off`，不等待云端撤销或后台清理。该值只表示当前远程开关，不代表其他连接、发布或可达阶段。

外部Gateway转发采用独立内部loopback listener，不能复用上述浏览器认证入口。desktop为其生成专用随机RAM bearer并提供给GatewayConfig.local_token，限定本次host generation与合成业务路由；HTTP按实际Gateway的Authorization bearer验证，内部WS按实际Gateway的首帧token验证（短握手超时和帧上限），验证前不处理业务。此入口仍检查精确loopback Host，并拒绝浏览器Origin；不能以缺少Origin作为放行条件。它不提供账号摘要、本地会话兑换、远程开关或token owner接口，不开放CORS。

内部Gateway bearer不是Native/Logto/协调凭据，不经浏览器、query、日志、命令行或落盘文件传递；外部测试装配通过受控进程内参数或私有pipe传入。内部与浏览器入口共享同一业务状态，关闭远程撤销内部bearer/取消其活动业务，但浏览器listener和已授权本地连接继续。合成业务仅实现外部Gateway现有路由白名单允许的子集，不通过扩大外部协议白名单解决样例适配。

## 远程 Web 授权与页面

SPA 使用官方 Logto 客户端的内存 adapter。登录打开独立授权窗口，使持有 PKCE/state 的页面继续存在；从用户点击同步创建窗口，随后才执行异步发现/导航，避免异步操作后被浏览器拦截。未能创建窗口时给出明确原因，不无限等待或退回手工 token 登录。

同源回调页面在清除地址中的code/state前保留仅内存副本，通过原窗口或同源 BroadcastChannel 将回调交回原owner；SDK验证精确redirect/state/PKCE。信道绑定本次随机state，回调页不加载第三方脚本/统计，不把回调URL写日志或持久化。不能只因收到message就认为登录成功。若平台隔离策略阻止回调或父页面被销毁，安全失败并重新授权，不把此路径描述为已支持。

刷新页面后重新授权；不将 refresh grant、access token 或业务私钥保存到 Web Storage。显示“已登录未开启、发布中、可尝试连接、业务已连接、正在恢复、状态未知、需重新登录”等准确层级。目录错误先清可用结论并关闭旧业务 wrapper；恢复后显式重连，不自动重放写入。目录 changed=false 也处理完整快照中的确认revision和续期，保守到期使用单调时钟。

页面布局与交互：

```text
+--------------------------------------------------------+
| CyberGrid Workspace 示例    账号摘要 / 登录 / 退出       |
+--------------------------------------------------------+
| 本机（desktop）                                        |
| [在浏览器打开]    远程：关闭 [开启远程访问]               |
| 本地业务可用 | 身份/协调/发布/入口/业务授权分别显示       |
+--------------------------------------------------------+
| 我的设备：状态、发布能力、身份待确认/换钥提示             |
| [连接选中设备] [取消]     恢复阶段 / 截止预算             |
+--------------------------------------------------------+
| 合成工作区：只读信息 / 请求校验 / 双向流                 |
| 每个写操作显式点击；失败结果未知时提示，不自动再执行      |
+--------------------------------------------------------+
```

手机竖屏按上述顺序堆叠；恢复前台后先核目录和业务授权，不假称后台常驻。连接等待时取消始终可用，关闭远程不被旧enable覆盖。页面不展示线路选择作为正常用户步骤。

公开Web入口接受固定`WebConfig`及显式受信的`createAdapter(authorization)`依赖，返回可等待的dispose；不接受外部脚本字符串、动态import路径或用户输入的token/地址。内部唯一官方RAM owner创建最小`BusinessAuthorization.fresh({signal})`闭包，只向受信adapter提供同SPA/issuer/resource/subject/generation的新鲜资源JWT；中止等待不能丢失已经派发的轮换。协调SDK通过同owner的独立上下文回调取得其所需access token，不能宣称OAuth资源token从不经过需要验签的协调exchange。业务正文、加密私钥及业务授权闭包不进入协调目录、页面view/log、Native IPC或本地API。

adapter直接实现007的受控泛型合同，不返回raw peer、terminal或transport。合成业务DTO仅允许实际Gateway白名单内的info、list/create session及poll，禁止任意path/RequestInit；流只承诺有界UTF-8 echo，分块正确处理多字节边界，输出队列溢出关闭通道。未注入adapter明确不可用。所pin外部connector仅支持Quick根及精确`/remote/v1/ws`，须核descriptor后才派生该根；通用custom provider支持与此connector限制分别记录。

Gateway-only合成本机上游的`/ws/terminal/{id}`与浏览器本地WebSocket是不同接口。前者先执行已有本机bearer首帧认证，随后仅接受精确文本JSON对象`{"type":"input","data":"合成文本"}`，`data`为UTF-8字符串，不允许额外字段、未知类型、非JSON或二进制业务输入；非法输入关闭该远程流。合成上游应把`data`的UTF-8字节作为binary输出，不回传整个JSON封套。既有4096字节WebSocket帧/消息上限继续生效，解码后data亦不得超过4096字节；Ping/Pong、取消和有界发送/关闭保留。此处不是完整PTY，不支持resize或任意命令。adapter维持已认证终端的原始输出字节语义，不猜测输出是否JSON，不以解包正常业务输出掩盖本机上游的协议错误。修复须验证多字节/转义文本、非法输入、远程关闭及独立本地连接继续可用。

## 外部桥接与许可

通用样例只依赖 CyberGrid 的正式 adapter 合同。真实互通验收通过用户已有 AnyWork 的公开 `Gateway/AccountVerifier/RemoteTransport` 接口装配外部宿主，固定其提交和实际文件hash，不复制其协议实现，也不把外部代码打入CyberGrid下载包。验收装配保存在源码树外；公开说明须明确外部依赖及许可，不以一个未实现的默认adapter声称样例独立可运行。

AnyWork已有interop fixture只有有限的短期假token，不能直接承担30分钟六轮授权或真实Logto验收。隔离长跑使用其实际Gateway及私有的轮换验证器，另行用实际JwtVerifier完成云账号授权/隔离检查。合成轮换验证器的通过不替代真实Logto登录；两类证据分别记录。实际桥接必须核对descriptor恰为该实现支持的显式WSS endpoint；不忽略path拼接其他URL。

项目自身许可仍按当前 LICENSE；此合同不授予外部SDK嵌入或再分发许可。技术上完成互通与允许将依赖发布到第三方产品是独立事项。

## 验收证据

先完成实际创建链和拒绝路径，再启动长跑；每项记录版本、网络/平台、起止时间、计数与安全原因，不记录凭据或原始业务数据。

持续授权的直接证据须包括客户端接受`auth.renewed`，而非仅token回调或服务端verifier返回。外部transport公开API没有对应事件时，可另行采用独立审过、绑定实际bundle字节与精确源码位置的诊断observer：仅在续期确认全部校验及lease更新成功之后记录固定事件与单调时间，不读取局部变量、私有字段、token或消息正文，不改源、协议或授权期限。调试断点须核实际解析位置、限制暂停/总预算并立即恢复，未知断点不计ACK，observer有明确detach清理；未审observer不能挂入既有运行。此类证据明确标为源码位置观测，不能描述成公开API事件；暂停时间与观测开销如实记录，不据此单独声称正常时延或完全无干扰。无论采用何种observer，都仍须另证同一连接在旧授权截止后业务成功，拒绝续期后按既定期限关闭；只有间接证据时保持该验收项未完成。

长跑的请求数只累计实际adapter完成、状态与合成DTO校验均成功的业务HTTP操作；目录轮询、UI点击、授权ACK和echo帧分别计数，不能充当业务HTTP请求。每五秒仅完成一次HTTP时，八小时最多约5,760次，不满足超过10,000次的门槛。runner按单调时钟的计划截止点调度足够的混合只读请求，不采用“完成工作后再睡五秒”的累计漂移；失败、超时或漏过计划点如实记录，不用突发补发或自动重放未知写入补计数。正式运行只计算同一页面、连接及代际从业务ready开始的实际连续时长，重新登录或重连须另起记录。受控时钟验证调度和判定逻辑时必须标为`CONTROLLED_ONLY`，不能形成真实30分钟或8小时的通过结果。

资源结论分开记录三种证据：绑定源码的集合/队列不变量、按平台可观察的进程或浏览器内存采样、实际运行计数与异常。每个有界结论注明作用域、容量、满额行为和清理路径；并发数上限不自动等于输入物化或整进程字节上限。静态源码审查不能冒充实时队列峰值，也不读取运行中对象的私有字段制造公开指标。采样本身须有容量和清理边界；无法观察的指标明确列为未测，不用短跑或无增长样本宣称长期硬内存上限。外部adapter的限制由其实现和绑定版本负责，不能仅凭CyberGrid wrapper的并发限制替其承诺。

进程RSS采样分别标明runner、Native、Gateway和浏览器的覆盖范围；runner自身RSS不能替代其他进程。仅采样已核定归属、可执行文件与启动身份的测试进程，绑定必须在开始前核对，PID消失、被复用或执行身份改变时立即使该段证据失效。采样器只读OS公开指标，不改变优先级、不暂停或结束被采样进程；进程路径等本机信息留在私有绑定中，对外只报告固定角色、相对单调时间、字节数和固定错误原因。OS采样的观测峰值也不是所有瞬时分配的数学上界。

正式runner在连接前核定目标产品、设备/运行身份、bridge、公钥与端点，并将其关联到本次独占Gateway的进程/源码绑定及新verifier日志。不能以列表第一项、任意身份确认对话框或一个空日志文件代替这些核对。使用源码位置ACK时，独占关联还须满足单页面/单连接、初始seq1和后续顺序检查；不满足前提则该证据不可用。启动门槛核对独立报告的实际字节、结论范围及其源码冻结，并逐项核对冻结清单与实际加载文件，而不只读取配置里的`PASS`文字或清单自身；报告未通过、缺失或绑定不符时不能开始正式运行。

| 门槛 | 必须观察到的结果 |
| --- | --- |
| 本地独立使用 | 无账号/断云仍能授权打开本地业务；关闭远程后同一本地TCP继续 |
| 官方双端登录 | Native/SPA真实同账号，无token/地址输入；跨账号与不允许client拒绝 |
| 发布与身份 | 未开启无业务发布；确认后目录变化目标5s，首次/换钥按明确策略确认 |
| 连接与取消 | 全链默认60s预算，取消1s内结束UI等待；底层清理有界，迟到成功不能交付 |
| 关闭与恢复 | 活跃加密请求/流终止、待确认云撤销与本机关闭分开；旧epoch/代际不复活 |
| 连续业务授权 | 实际30min跨至少六个短业务授权周期；拒绝续期后及时关闭 |
| 长跑与资源 | 实际8h且超过10,000个混合请求、活跃双向流，内存和去重有界；未知写入不重放 |
| 协调/准入到期 | 单独覆盖24h边界，真实存储/SDK受控时间或独立长跑；不把短模拟叫8h实测 |
| 手机浏览器 | Android Chrome Wi-Fi/蜂窝/切网/锁屏返回；iOS缺设备时明确未验证 |
| 最终交付 | 网站/下载/版本矩阵与实际能力一致；最终源码导出扫描、独立评审与生产回滚记录齐全 |
