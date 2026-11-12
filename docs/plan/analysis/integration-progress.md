# 产品集成改进覆盖表

目标：按 AnyWork 集成反馈的评估建议完成改进。不能以某两个 SDK 或某一轮测试通过代替整体目标。当前状态以代码提交、任务评审和真实集成记录为证据。

2026-11-11 复核：当前源码核对基线为`9c5c7f0`，包含公共样例`652634a`、发布准备`69cbfa3`及终端echo修复`bdaefb3`/`4f7ab46`与评审记录。012模块已合入`2ce0dc1`/`e97ad1e`并完成根组合验证；011为`f3b3a05`，006为`cd3e891`，007为`0465e75`，Lab为`1da15cb`。反馈调研的 `50a12e0` 是历史基线；不能把其中描述的所有缺口继续写成当前源码缺陷，也不能把本地新源码当作线上服务或已下载 SDK。真实云登录已有下述分段证据，完整业务、手机和本轮部署仍未验收。

反馈中的改动分成三类：产品绑定、授权/状态、关闭、目录和桥接接口已有本地模块交付；共同样例公共模块已通过独立复审，真实装配仍在验收；正式入口运营、手机切网、持续授权长跑及最终发布仍需实际证据。AnyWork自身的导航统一、工作区权限与旧入口退役继续由接入方负责，不纳入CyberGrid已完成清单。

## 反馈处理摘要

本次按当前源码重新判断修改范围，原反馈的 CG-01～CG-12 编号继续保留。以下是阅读入口，具体证据与逐项状态仍以下文覆盖表为准；“模块已实现”不表示已发布或完整体验已验收。

| 主题与对应反馈 | 当前判断 | 下一步及交付标准 |
| --- | --- | --- |
| 浏览器安全连接（CG-01） | 已有受信桥接扩展；不是内置通用浏览器传输 | **P0 集成验收**：真实官方登录→目录→目标身份核验→加密业务请求/流。缺适配器时明确不可用；完整浏览器传输另立项 |
| 持续授权（CG-02） | 已有新鲜token请求及单一owner，业务端持续接受授权尚未完整验收 | **P0 验收并按失败定位修复**：正常续期、拒绝/到期关闭、30分钟跨至少六轮更新，再做8小时及超过10,000混合请求；不能以延长TTL代替 |
| 状态、错误与取消（CG-03/05/09） | 已有分层状态、固定错误及代际隔离；接入诊断仍可集中完善 | **P0 候选改进**：在创建/发布前校验provider、产品绑定、协议及能力；固定stage/reason区分配置不符、暂时故障和操作结果未知。先定位真实链中的缺口，不另建平行状态框架 |
| 产品绑定与最小权限（CG-04/11） | 已有静态SPA/Native允许集合和可配置Management参数 | **复用并验证部署**：静态模式无Management M2M可运行，真实允许集合互通、集合外及跨账号拒绝。应用仍由Logto管理，CyberGrid只声明允许策略 |
| 本机关闭与RAM-only身份（CG-07/10） | 已有默认关闭、本机优先封入口及有限准入实现；生产迁移未完成 | **P0 完整链验收**：断网关闭也终止远程活跃流、本地继续；重启重新授权、换钥确认、重复启停有界。安装ID不代替授权或公钥确认 |
| 生产入口（CG-06） | 已有provider策略，缺正式运营交付 | **P0 产品/运营决定**：明确实际provider、维护者、配额、费用和故障范围，再验收；Quick Tunnel保留测试定位，最终用户不配置线路 |
| 目录、平台及交付（CG-08/12） | 已有版本化目录/watch及发布准备；手机与最终发布未完成 | **P0 验收/发布，P1 扩平台**：ADB控制Android Chrome进行Wi-Fi/蜂窝/切网/锁屏返回；iOS及其他平台按实际证据声明。明确元数据保留、SDK分发授权及版本兼容，最终冻结后重新导出/构建/部署 |
| AnyWork产品改造（反馈第8节） | 属于接入方实现范围 | 统一账号/设备/工作区导航、业务权限/写入恢复、本地Web受限会话和旧入口退役；替代路径验收后再退出旧入口，不将这些列为CyberGrid缺陷 |

反馈中的配置偶发失败、业务票据未续期和图片路径问题按原证据分别归属，不能据此重写协调服务。继续保留Rust主SDK、JS默认只读目录与显式安全适配器、Logto身份和Redis/Upstash协调；不新增云端业务数据库或内容代理。

## 本次整理：接下来实际需要做什么

本节是后续工作建议与交付门槛，不把反馈中的建议接口名当成已存在API，也不把诊断假设当成已确认缺陷。下列状态区分已合入实现、私有验收工具、实际运行证据与尚未发布内容；后续工具准备不构成运行通过或部署。

修改判断分为四类，避免将所有未验收项都排成新功能开发：

- **已有实现，继续复用**：产品SPA/Native允许绑定、静态目录、新鲜token请求、分层状态、远程开关、版本化目录和安全桥接扩展；后续以完整集成验证为主。
- **已证实缺陷，修复后重新验收**：样例terminal输入封套回显错误已修复并独立复审，重新绑定后的实际公网echo短链也已通过；完整关闭、持续授权和长跑仍待验收。实际空闲连接提前关闭尚未确定原因，不先归咎于代理、descriptor到期或系统休眠。
- **候选代码改进，待证据明确范围**：接入前置校验与安全诊断，重点检查provider ID、产品绑定、协议和能力限制，区分配置不符与暂时不可用。现有固定错误码继续复用；私有装配错误不能直接证明公开SDK缺陷。
- **产品/运营决定**：正式provider、支持平台与可用性范围、SDK嵌入/再分发许可；这些不能靠接口测试或域名允许列表自动解决。完整浏览器传输另行评估，当前采用受信安全适配器。

| 顺序 | 需要处理的事 | 修改判断与完成标准 |
| --- | --- | --- |
| P0-1 | 完成真实安全业务链 | 公开terminal协议修复已独立r2 PASS并合入，既有根复测为样例Rust26/26及源码策略7/7。新的实际Quick/Noise/JwtVerifier短链已通过三次HTTP200/DTO及精确公网echo；其后空闲连接约139秒关闭，原因未定。另一独立连接持续390.45秒、完成35次HTTP及35次echo，实际验签续期推进且9轮在原截止后成功；源码位置ACK observer机制已独立r2 PASS，真实ACK仍待观测，不能标完整续期通过。独立实际loopback空闲190秒后同连接GET200，进一步缩小无条件协议超时假设，仍不定因Cloudflare。拒绝关闭及独立本地继续仍待验证，不混计新旧连接或重放未知写。 |
| P0-2 | 补接入诊断与前置校验 | 已确认私有装配误用了provider ID，正式内置值为`quick-tunnel`，修复已在私有装配完成。建议在创建/发布前集中校验provider、产品绑定、协议、地址和能力上限，返回固定stage/reason。首次读取失败的系统代理候选已被实际同构loopback正反例否证，两组均HTTP200且目标命中；继续核对adapter请求/响应契约，不据此修改全局代理或放宽URL/身份校验。 |
| P0-3 | 持续授权与恢复验收 | 30分钟且至少六轮被业务端接受的新授权，再做8小时、超过10,000混合请求及活跃流；同时验证拒绝续期、过期即关闭、断网/休眠恢复和取消。未知业务写入结果不得自动重放。MemoryStore逻辑24小时已受控验证，不能替代Redis自然到期、Rust自然恢复或真实长跑。 |
| P0-4 | 手机浏览器真实体验 | Android先验收Wi-Fi、蜂窝、切网和锁屏返回；使用ADB控制测试流程，不要求用户复制地址或token。回前台重新确认身份、目录和通道。iOS Safari缺实际设备证据时明确未验证；不得用桌面viewport或Android原生SDK测试代替。 |
| P0-5 | 明确生产入口 | 已有受信provider策略和扩展接口，仍需选定实际生产provider、维护者、域名/路径、配额、费用和故障边界，并做真实验收。Quick Tunnel继续标为测试用途；产品内部选择线路，普通用户不维护隧道。 |
| P0-6 | 收尾发布与对外说明 | 013已完成报告纳入、独立closeout、合入与根组合验证，按准备模块done；010后续echo修复等运行/文档改变仍需最终重新冻结/导出。运行验收通过后再完成GitHub/Actions/下载/部署映射。说明哪些能力已发布、哪些仅在本地源码，保留旧版本真实能力。项目嵌入、修改、再分发许可须由所有者明确，技术评审不改变LICENSE。 |
| P1 | 扩平台及完善使用体验 | 在上述链路通过后补更多平台、诊断展示和性能基线；完整浏览器端到端传输另立项，不混入当前桥接交付。逻辑设备身份不能替代公钥确认，RAM-only重启须重新授权。 |

资源独立静态核对已完成其有限范围：绑定版本的Gateway每实例4个连接permit、每连接HTTP准入最多8项及另外最多2项PTY、容量16的相关队列和SDK wrapper槽位的增删/清理路径已追踪；正常`rN`请求使用高水位去重，没有第1,025或10,001个成功请求必然耗尽的累计历史上限。legacy ID集合及取消后未收到ACK的异常历史仍各有1,024项限制，不能与正常成功请求计数混淆。本次只运行只读哈希核验，没有runtime峰值或新的生产漏洞复现。请求体预物化、响应分块metadata、任意adapter内部缓冲及未完成关闭的条件界单列，不能承诺无条件的整进程硬内存上限。静态报告冻结SHA为`699a14cafb4a3f1403e07f484b1bfce6fa1b4fb7b2431e38abdb63b51a1fbd4f`；它不能替代8小时与内存实测。

正式30分钟/8小时runner的首份私有离线候选已冻结，独立评审为**BLOCKED**，尚未授权或启动实际运行。两项P2阻塞为目标/新独占Gateway关联未建立、启动检查未绑定实际独立报告；3个独立离线断言实际失败exit1，原失败和15个冻结文件均保留。报告冻结SHA为`5ecfb80f883549937b24e228ee8cda6aac1c5827cfdc76f73baf52fe3042bbfc`，修订须在新目录完成并重新评审；工具问题不能当作SDK已复现缺陷，也不能用作者测试抵消。作者实际28项离线检查覆盖受控调度、文件/句柄边界和UI doubles，8个源码语法及默认安全入口检查退出码0；这些仅是工具准备证据，全部受控时间标为`CONTROLLED_ONLY`。

后续runner r2独立评审仍为**BLOCKED**，新增一项实际接口不兼容：目标observer将请求query误写为`productId`，而公共JS SDK与服务均使用`appId`，仅响应body使用`productId`。独立反例通过实际SDK→HTTP→MemoryStore/服务取得200和完整有效目录后，被observer错误拒绝为`TARGET_PRODUCT_CHANGED`，实际exit1；作者镜像target测试35/35与新增gate检查9/9通过均不抵消此结果。报告冻结SHA为`ff4d55e6320afbac15c6063a735804716cfb48c9d85313a0cf6fd05f2546dde3`，36个r2冻结文件与原反例保留。修订在新r3目录进行，必须加入真实SDK/服务组合验证；不修改公共SDK或服务去迁就工具，也未启动正式运行或签发实际gate。

runner r3已冻结30个文件，manifest SHA为`af9beeaf574752af56b430552f2ba73e37fae8b4a52b46c15c38582be6a94d63`，独立复审为**PASS / OFFLINE_RUNNER_MECHANISM_ONLY**，报告SHA为`ad8ce2e09290beae473df7d586ad242ab6f7f2779dc7c380fde033d38cc24893`。运行代码相对r2仅修正上述一处query，其余六个运行模块字节未变。作者相关验证65项通过：58项target/gate、6项实际SDK→HTTP/MemoryStore组合及原独立实际SDK反例1项（脚本字节不改，仅适配依赖位置）；独立本轮35项实际exit0，原锁定反例确认HTTP200→SDK解析→watch关联成功。首次新增换钥测试因未增加地址版本而被服务正确拒绝，原失败保留，修正测试数据后通过。离线机制范围内原三项阻塞闭环；此处仍为受控工具准备，不是实际登录、ACK、30分钟或8小时通过，也不代替真实目标/进程/日志attestation。

runner设计每五秒计划分别等待info/list两次HTTP及DTO校验，再验echo；漏点跳过并记录，受控slow case即使到达八小时而仅完成5,760次模拟HTTP仍判失败。真实模式不允许注入时钟，仍需实际30分钟/至少六轮接受续期、实际八小时/超过10,000次HTTP及同连接流。首候选仅含自身RSS与可选页面heap，Native/Gateway RSS、内部队列/去重实测峰值均未覆盖。独立macOS只读RSS采样助手已通过有限准备范围的独立审查：冻结测试副本14/14及新增7/7均exit0，其中六个新建OS短fixture全部自然exit0并回收，余项为受控反例；未读取现有owner或Native/Gateway，未集成正式runner，也不解决其两项阻塞。采样报告冻结SHA为`537658a2ed05b7fdbd911cef9b684fe713070d09390a0f04a3390db6a0e2b544`；同PID多角色RSS不可相加、同image exec与非原子采样等限制保留，实际资源与8小时仍未测。

私有拒绝策略已完成有限独立源码审查，报告冻结SHA为`4bb0ce9407c2775616c07f7b36951105e5b0c1bc9850492c49e4420aa558c775`；该审查本身未构建或加载，后续新构建单列如下。默认关闭，仅显式测试开关启用；真实JwtVerifier先验签，按已完成的verify调用计序号（失败也占），只在seq2且真实验证成功时拒绝一次，并非无条件“第二个成功验证”或永久撤权。实际用例须新独占Gateway、seq1成功initial且无先前失败/额外连接，才可把seq2关联到首轮续期；否则该用例无效。共享append日志也不自动证明新实例，须旧owner正常结束并保留原日志，再核新空日志inode与进程/源码/目标关联。本结论仅证明故障注入准备条件，不是Logto云撤权、实际拒绝/到期断流或独立本地继续使用的通过证据。

2026-11-11本机时间02:56，旧测试Native已通过公开界面关闭远程，观测到`localClosed=true`、操作off、协调撤销Confirmed；随后本机退出明确完成`owner_end:done`，正常退出应用且原PID消失。进程路径/启动身份及实际可执行文件SHA已先核定，无强杀、日志清理、手机操作或新实例。旧二进制另作逐字节保留。其后首次`--locked --offline`私有构建因Cargo.lock需要更新而实际exit101，该次失败后未自动重试或改锁、未下载或生成新产物；前后源码与dist哈希一致。原失败完整保留，后续仅按下述精确恢复方案另行构建；旧二进制不能代表当前拒绝策略源码。

依赖锁差异已定位为外部路径包`anywork-remote`继承工作区版本从0.1.21升至0.1.23，而私有锁仍是旧值；其清单、整个remote crate及关键传输源码无变化。隔离副本仅改该版本字段，单次`cargo metadata --locked --offline`实际exit0，解析仍使用原三个路径依赖；active文件当时未变。独立只读核对报告SHA为`96a79b628c3cbafee04c9c1d105272045cf3184244a391fadf107ef543cff5c7`，通过范围仅为可应用该精确一行并重新locked/offline构建。随后保存原lock并应用相同一行，新build实际exit0、26.812秒，新可执行文件SHA为`6c33ae7311786101d60d96a52a90b00c17665c4cd061afafbb0d0aa2a6933617`。构建前后公共运行源码、私有源码/锁、相关外部文件及desktop dist未变；旧bundle与保存件仍为原二进制。该构建步骤未替换bundle或启动程序；normal/deny实际加载及运行另记，不以构建通过替代。

随后新建独立normal测试bundle并启动新二进制，实际进程路径/启动身份/UID已绑定，前端三个静态资产与dist哈希一致。固定verifier日志先核无writer，再完整复制原字节及元信息并原子移动保留，以`O_EXCL`/0600建立原路径新空inode，启动后仍size0；原bundle/保存exe及lifecycle日志未动。唯一一次初始界面读取明确返回`MAC_LOCKED`，立即停止且未尝试解锁。当时只确认新Native进程活跃，未确认UI状态、未登录或开启远程，未发正式gate/启动runner。另有预检采集缺口：完整target字段不能从DOM名称/计数取得，而formal入口本身先要求target attestation；以独立directory-only官方SPA预检采集器补齐，冻结后须独立审查，不能用伪造gate、自造body或转移token自举。

后续重新核定同一进程及日志身份，目标内部AX已可读，明确未登录、远程关闭。公开登录仅点击一次后出现固定“操作未完成”；实际白名单阶段包含`owner_begin`/`oauth_fetch`完成及`oauth_open`完成后又失败，未进入callback。CUA启用面未观察到新授权tab/window，但不能据此断言系统绝对未打开页面；系统默认HTTPS handler与已启用Chrome面匹配。独立源码诊断报告SHA为`f53900990f6facc3079545b0ae8504a9beecfdbca0ddc644aa3ba859c49cf57c`，补充为`649563e00c668666569f85475d61d2f2c71ce1d806bdc64cf79ec43ddee763ae`。实际私有invoke壳在原生`oauth_open`成功后另await授权URL诊断relay，后者失败仍会抛错并阻断callback；公共登录方法、RAM owner及原生opener没有这项差异。最小修复仅删除私有后置relay依赖，已独立通过下述有限源码审查，不据此修改公共SDK或放宽鉴权。旧`native_token`标识不在本次实际bundle中，其未观察不能用于定位。

该invoke候选现为**PASS / CONTROLLED_ONLY / PRIVATE_INVOKE_SOURCE_ONLY**，独立报告SHA为`eed5d2e593d13e97bbf7ea5482bf98c3bad30e59024c8de41a40d29328e22797`。冻结14项输入前后哈希一致；函数外字节不变，底层native只调用一次，结果/错误及等待语义保留。独立6项与作者5项重跑均exit0；首次静态oracle误判及原exit1保留，修正的是审查断言。候选尚未安装到实际前端，真实浏览器打开/callback仍待新装配验收。诊断过程中曾误输出本地测试relay的访问能力值；不在公开源码或发布产物内，该事件也不能等同于已证实OAuth凭据泄露。旧relay经fresh归属核定后仅一次SIGTERM正常关闭，原PID消失且1430无listener；只证明该服务endpoint关闭，不推断其他转发副本均不存在。本轮整理未启动替代服务或重试登录。

该失败owner随后通过公开logout完成`remote_disable:done`和`owner_end:done`，再正常Quit；实际PID已消失，原源码/资产及日志完整保留，没有强退或清grant。此前新实例ACTIVE与锁屏记录保留为历史观测，不能继续作为当前进程状态。directory-only采集器初版已冻结，但独立评审发现两项P2：主页面导航/关闭后仍能导出旧快照，及newContext创建失败未回收自建browser；四个原断言实际exit1。报告SHA为`db31a0fe7cbcfb303f764039e4734e43176a0d4093447c9c0cfcf13796d923c3`，原冻结不动，新r2修复；初版不得用于真实登录，实际SDK/HTTP正例通过不抵消这两项阻塞。

后续实际应用exact reviewed私有invoke源码并构建到独立新dist，TypeScript与Vite均exit0，旧dist及其他私有源码未变；未重新编译Rust。新产物manifest SHA为`3b5827f212f8fbd1a07205b969965295d759a6731f1545c6c774606d0822259e`，源码/产物绑定SHA为`f01c3fe2ba9d493fea61cbc440f246dca333d34f491b03b88a9b16e0d6b8ffaa`。独立artifact核对确认源码/三资产及原运行源码/二进制绑定，报告SHA为`1ca629c1440a7e4e4b295f4dd72c44bee665115c6af8ae33255bbd27c00f3bf4`；旧relay特征值不在新产物中。新纯static1430服务的三次无query GET内容hash/类型匹配，空POST返回501，访问日志直送DEVNULL；其后fresh核发现执行文件path/hash与启动记录不一致，虽然持有handle、PID/start/UID/parent及唯一loopback listener仍匹配，仍标`ACTIVE_HANDLE_IDENTITY_MISMATCH`待诊断，不重签原绑定或自动重启。HTTP资产通过不证明Native实际加载；本轮CUA再次明确返回Mac锁屏，未尝试解锁或启动Native/登录。

上述static身份差异后续已按有限证据定位：最初读取的是Python framework启动器，随后同PID执行最终framework解释器；两个image的文件/签名及启动记录支持这一解释，没有exec事件追踪，不追溯声称精确转换时刻。诊断报告SHA为`5ad13bd07c11c92c59b848cd21041f5523705ab057da6de54831dca3f0021abb`。fresh核对该owned child身份和listener后仅一次SIGTERM，原句柄wait/reap退出-15，PID和listener消失；原MISMATCH完整保留。新服务直接使用已核最终解释器，首次未就绪样本保留，随后三次连续身份/listener一致才建立新绑定；三个GET hash/type及空POST501、再次fresh身份核对均通过。新报告SHA为`66a4d7d35e2baf49475811d879477f6a015794ea2b9337ce98a38ef44fe06a03`。该服务当前owned ACTIVE；默认静态缓存行为不变，仍不能据此推断Native已加载或登录通过。

directory-only r2冻结manifest SHA为`d9836420527e4550cc03dd22c496f54333321251abb899354b207bddff6a51ee`，已独立**PASS / PREPARED_ONLY / OFFLINE_DIRECTORY_COLLECTOR_MECHANISM_ONLY**，报告SHA为`f9e51075eab025ccfcbc51375224c81af67c63154d87d361613a20f1ae7e6440`。原反例4项、独立补充9项及实际entry退出顺序3项均exit0；作者受影响23项和实际SDK→MemoryStore/HTTP1项独立重跑exit0，冻结30输入前后匹配。原失败及审查自身修正记录保留。页面失效后拒绝写出且保留未知owner，不冒称已清理；真实headful登录/退出、目录采集和formal gate均未执行。

后续验收入口只读核定另确认两项缺口：正常runner不提供独立deny判定，不能把normal失败重命名为拒绝通过；复合关闭也缺公开Native动作与同一本地持续流、远端EOF/新请求拒绝、门控和撤销分层结果的完整组合入口。原有拒绝注入和公开关闭模块继续复用，不将入口缺失写成模块已复现缺陷。RSS库已具备pin/attest，但旧启动方式缺启动前pin及十秒内的实际launch receipt，需薄launcher连接；这些入口准备不改变30分钟/8小时和真实关闭仍待验收的状态。

薄Native RSS launcher现已冻结，manifest SHA为`54da53ee454aea0b216f7433804423f654f0c60370e3a8d916d05f7f9d9eedf3`，独立审查为**PASS / LIMITED_PREPARED_ONLY / OFFLINE_LAUNCHER_MECHANISM_ONLY**，报告SHA为`367563a5c8f9c785abdaba7d6e35c76c9a31264d91c54b6111d50c226fa51169`。作者15项独立复跑exit0（14受控、1新建短Mach-O实际身份/RSS/自然退出回收），独立新增9项exit0；launcher8项及sampler10项绑定匹配，原开发/审查断言失败保留。顺序为prepin→单次直接Popen→立即attest→私有回执→60秒间隔采样，启动后异常/取消继续保有child，不向Native发信号、不重绑。同PID的Native/Gateway仅计一次whole-process RSS；60秒不是owner清理总期限，取消后exit0仍须读取明确cancelled/failed状态。尚未启动真实Native、采集其资源或生成formal gate。

只读启动预检报告SHA为`8846e92ad3bae79b485ac56b715620916dc6b0db990b0af6c48774e1c24b2c95`：新static持有句柄、实际身份/listener/三资产和旧normal bundle二进制均匹配，approved config路径/字节hash/私有权限未变；verifier/lifecycle日志仅核stat和可见writer，未改写。配置加载映射当时仍为UNKNOWN，检查脚本误用直接反序列化/literal-key假设不能作为配置失败；须另外核实际loader。旧infra两个session在检查方不可见，只记录UNKNOWN_HANDLE，不推断服务已终止、不自行重启。最近CUA仍明确锁屏，真实登录未开始。

配置加载映射后续已按实际源码核定，报告SHA为`ad9ce93ee761b1e4badba5f7aef941ba121cbb93f723349bff7dbcd9ced04017`，范围仅为SOURCE_LINKAGE_ONLY：wrapper先读JSON再显式构造BuildConfig；launcher常量引用解析后的两项环境映射与Rust读取名称一致，normal与旧absent均不开拒绝策略。当前配置hash/权限、构建输入及既有二进制绑定一致；历史UI的owner_begin/oauth_fetch DONE不能证明旧进程原子读取了当前配置，限制保留。另一次受限GET实际HTTP200，固定issuer/Native client/resource/product匹配，报告SHA为`003a51dba558a27a4739b92c221467aba4dc4f806526197d575fbd3ce34efe39`。请求没有Authorization/cookie/body且不跟随重定向，不保存响应body/URL/header；endpoint策略的精确历史对象绑定仍未证，不据响应扩大信任。

该配置加载链已独立**PASS / SOURCE_LINKAGE_ONLY**，独立报告SHA为`456a435a9e6dd19dd2253083bc328b3a7928727a4550ffb5ef3cdc7375372b81`：候选manifest/报告、10项引用、6个私有构建输入及当前配置/二进制前后hash一致，28项源码/材料条件成立，未执行新运行测试。normal/旧absent的等价仅限拒绝策略条件，不扩大为完整环境等价；旧配置原子读取及当前运行态仍未由此证明。

基础设施只读报告SHA为`322a4f404d70de3516b15f99343be6a2a80a90d6fd564d064d99852c295f878f`：1433当前有唯一loopback listener，其PID在原始ready列表中，启动源码hash仍匹配恢复绑定。但原始记录没有per-role start/UID/executable/hash，不能排除PID复用或据此证明运行源码；Quick角色绑定同样未证。保留UNKNOWN，不停止、重启或补签这些服务。当前公开配置响应匹配不替代运行身份、真实OAuth或formal目标核定。

最小交付链为：Mac宿主本地可独立使用 → 官方登录并明确开启远程 → Web同账号发现设备 → 实际安全业务读写/流 → 持续授权与本机关闭 → Android Chrome网络变化 → 最终导出、Actions、下载与部署核对。每一步使用上一层的真实产物；手机、长跑和发布各自留证，不以模块测试合并结案。5秒目录收敛、60秒连接预算与1秒取消仍须按约定前提实测，不能仅凭默认超时之和写成保证。

2026-11-11解锁后实际进展：使用已审normal launcher启动新Native，真实启动回执及RSS采样已建立。外部工作区manifest/lock仅路径包版本由0.1.23变为0.1.24，实际运行源码和预编译二进制未变；保留源码树漂移记录，不补签历史构建。此前一次CUA选择应用引起的未登录setup实例已正常Quit，不计正式启动证据。正式实例公开点击登录后，系统Chrome实际显示预期Logto租户登录页；默认系统profile与已启用extension的profile不同，因此后者标签清单未显示该页面，不能由此推断浏览器未打开。

首轮在完成账号输入前超过callback等待期限，后续浏览器回跳被拒绝；公开退出明确完成remote_disable及owner_end后，第二次公开登录在同一系统profile正常完成callback、token_reply和sdk_login，Native显示authenticated/ready。没有跨profile转交授权URL或grant。通过公开入口打开独立本地工作区后，状态读取返回summary/workspace，双向流检查显示“本地双向流可用”。随后公开开启远程，Native显示connectable及publication ready，并明确业务入口发布已确认。这些是实际登录/本地业务/发布证据，尚不代表远程业务握手、持续同流关闭或长跑通过。

该实例fresh进程身份与启动回执匹配，Native/Gateway同进程角色源码对应构建输入，固定生命周期事件包含Gateway启动、guarded bind及Quick入口可用；实际verifier仍为本轮新inode且size0，尚无业务验证。RSS按原明确2GiB测试失败阈值持续采样，该阈值不是分配上限。下一步独立directory-only官方SPA采集须正常退出后再开始新的正式业务页面；30分钟/8小时、拒绝续期、复合关闭、手机及公开发布仍未完成。

同轮官方SPA目录预检已实际成功：r4一次运行24.630秒、exit0，网页登录、完整目录快照、正常logout、元数据保存及浏览器资源关闭全部完成，verifier仍同inode且size0，业务动作数0。此前真实失败定位并修复了采集器自身的时序问题：首次HTML响应到达时Chrome的page.url可能仍为about:blank；无关响应应先过滤，实际目录响应继续执行原有document/代际检查。真实loopback Chrome复现及旧导航反例独立通过，r4报告SHA为`6680bb53abb558469370659250740125cfc1f11ca811834eb65a49499ee92972`，实际结果SHA为`ec39e7521d8539b995a36244a4a7771bcf33edb5b208730e6ec245f8d3470e9d`。手机Android Chrome在已验证蜂窝网络下也实际显示本轮Workspace网页；尚未手机登录或建立Noise业务。另确认本地摘要未投影Host开启状态，以及D/C私有reader误按顶层校验嵌套workspace DTO，两项分别进入修复，不能以原受控PASS掩盖实际接口缺口。

本地摘要修复`f849bfe`已独立PASS并合入`e76a300`：Workspace直接读取同一Host gate投影on/off，账号摘要独立更新，去除异步cleanup覆盖摘要的路径。作者18项定向回归通过；独立新增实际cookie/CSRF HTTP与同WS场景、重跑真实Redis/JWK生命周期及迟到取消场景均通过，parent反例失败、候选通过，报告SHA为`634fea108c7dd4da6ed54a50b6c03aeed230a393882278d7f989f299f39d526e`。新私有Native实际locked/offline构建exit0，耗时7.624秒，二进制SHA为`708c13e793891ac69fcd3738c5b4759fd58bcb595878e6bd77c3a1a6dc9b306e`；外部路径包仅自版本对齐，相关运行源/registry依赖未变，旧6c33保留。新程序尚未启动：公开UI切换旧owner前，CUA再次明确返回Mac锁屏，未点击、未绕锁或强退；须解锁后正常结束旧授权再启动新实例，不能沿用旧target给新binary签发正式验收材料。D/C修复与接线仍待独立复审和实跑，旧PASS的DTO漏检已另记BLOCKED补充，GitHub/生产部署尚未执行。

已有接口工作继续复用下表CG-01～CG-12记录，不重新建立账号系统、云端业务数据库或内容代理。SDK保持Rust为主；JS默认目录只读，业务访问通过显式注入的受信安全适配器，不能宣传为内置了任意浏览器传输。

再次解锁后的新运行：旧实例公开退出完成 `remote_disable:done` / `owner_end:done` 后正常 Quit，launcher 自然 exit0。新 `708c` bundle 经同一已审 launcher 单次启动，实际进程/启动身份/二进制与回执匹配，使用新的空 verifier inode，原日志完整保留。官方网页登录在系统 Chrome SSO 后正常回调并完成 SDK 登录；公开开启远程后显示 connectable / publication ready。本地实际读取先为 `remote:off`、开启后为 `remote:on`，本地双向流检查通过。此处仅计新版本登录、本地业务和发布，不计远程长跑。

D/C r2 已独立 PASS 离线机制范围，报告 SHA 为 `2fea5d3ab10d35d9a28f7da6326cda000fcdf52b03932f18fbffd3c826e171e8`；嵌套 DTO 原反例、实际 Host HTTP / 同一本地 WS 及 driver 边界均通过，旧失败保留。解锁后实际公开 local-open 路由到系统默认 Chrome，专用测试 profile 未收到页面；据此补进程创建时的受信 initial opener 扩展，不搬运已兑换会话。它保持默认浏览器行为和原调用者、一次性兑换边界，独立审查后再用于实际关闭验收。

受信 opener 候选 `dc62f41` 已独立 PASS 并合入；四项聚焦测试零跳过通过，根分支三处源码与冻结一致，报告 SHA 为 `80e449e2c40df51a7bddc369c5dec2e24374953148bec0bba96c2bfa1ed9838f`。真实初始路由和 D/C 仍待私有 pipe 接线复审及实跑。新实例目录采集两次均在登录前 `PAGE_GOTO` 失败，唯一重试取得 `ERR_CONNECTION_CLOSED`；浏览器正常关闭、verifier 仍同一空 inode，没有产生新 target 或正式运行 gate。当前正核测试前端服务和隧道，不将网络失败记作登录或目录协议错误。

新实例在 root 未请求关闭时回到了 off / closed，撤回显示 confirmed；之后才执行公开退出授权和正常 Quit，launcher 自然 exit0。日志已完整保留。源码确认没有十几分钟的固定关闭期限；发布每60秒续期，另有独立发布到期看门狗，若干自动关闭分支尚未保存固定错误原因。实际触发原因仍未知，正在补诊断而不放宽关闭策略。测试网页本地 listener 存活，旧 Quick 角色有退出且地址归属不完整，采用新的自有静态服务及匿名隧道恢复独立测试入口，旧资源保留、生产配置未改。

关闭原因候选 `159cc90` 已独立 PASS 并合入 `74fc976`，四路径与冻结精确一致。固定首因在同一同步 gate fence 中记录，旧代际不能覆盖新操作，显式关闭不误报 owner 结束；失败关闭策略不变。独立四项定向测试零跳过通过，包含真实60秒 HTTP policy503、端点丢失及发布到期，本地同一 WS 继续可用，报告 SHA 为 `4a8e43dc30c1fa1daf8a1f1c0979b0e500bd4051f5815ec6f531a11504f525f0`。旧实例具体关闭根因仍未知。新自有测试入口已完成四文件实际 HTTP200 / 类型 / 哈希核对及隔离 SPA 回调 readback；下一实例采用该新 origin，不改生产回调或配置。

后续实际实例通过现有 `sample_view` 只读调用并仅输出固定码，取得 `PUBLICATION_UNCONFIRMED`；界面原四码筛选将其显示为 `UNKNOWN`。统一固定码映射的私有 TS 候选已独立通过，仅用于运行诊断，尚未算公开发布。该实例随后通过公开退出完成 `remote_disable`、`owner_end` 并正常 Quit。源码排查发现目录采样与 SDK 最新发布 revision 的确认可能受到独立 heartbeat 交错影响，正在做最小确定性复现；目前不能据此声称已证明该实例的具体触发时序。

新 Web origin 的首次预检请求漏了 `/api/v1`，原 HTTP405 记录不能作为 CORS 证据。正确路径的后续预检返回403，才确认现有测试协调服务未允许新 origin。其监督进程会在单个子服务退出时关闭关联服务，因此保留整组旧服务，另启动复用同一 Redis/prefix 的自有测试协调实例；新实例正确路径预检实际204且允许 origin、方法和授权头匹配。此调整仅恢复隔离测试环境，未修改生产配置，也未计为登录、业务或长跑通过。

2026-11-12，发布确认竞态已确定性复现并修复：真实 Node/Redis/JWK 在目录采样后等待默认20秒 SDK heartbeat 推进 revision，原实现返回 `PUBLICATION_UNCONFIRMED`；同一反例的修复运行通过。候选 `68b6b11` 经独立复核 PASS 后合入 `dae8e58`。确认改为读取前已确认 revision 的下界，仍要求完整 descriptor、当前意图、身份/epoch、代际、取消与有效期一致；旧 revision、不同 descriptor、改变意图及取消的负向检查通过。独立实际2项测试、零失败/跳过，报告 SHA 为 `dbfe08f871242e647d506b7235f679b0a0f3846f685bb0158cc460ef111cd055`。根合并后完整样例 workspace 实际 **36/36**、零失败/跳过（包含 ignored 仓库集成）通过。该确定性复现不能追认此前实际实例的精确触发时序，也不代替云端长跑和手机验收。

同日，Lab 传输测试选项 `c7ec15d` 已经独立复核 PASS 并合入 `be046a1`。Mac 下拉框与 Android ADB 共用可选 `directTransport=auto|webrtc-only|iroh-candidate`；默认和并行策略保留，强制回退优先。`iroh-candidate` 仅缩短 WebRTC 预算，不代表协议层禁用；实测必须依照 ACK/probe 的实际 transport 分类。独立 Rust 定向 **3/3**、Python ADB **23/23**、TypeScript 和离线执行下拉框处理器六次状态转换通过，报告 SHA 为 `5c1e273f0610224980cce96f7a3f9c4c7c5616b7c511574a38ce5e94438852ab`。Mac 测试包已构建并校验签名，Android 同签名原位更新及安装后哈希核对通过；手机尚未完成本轮官方登录与公网互通，不能将构建或安装计作 WAN 验收。

按用户要求先清理中间数据：删除 48 处不再使用的可重建 Cargo/Gradle 缓存，实际释放约 **22.18 GiB**。源码、凭据、客户端产物、运行输入及原始测试证据均保留，清理前后源码差异一致。此后仅重建当前必要产物。最新本机锁屏状态仍为 locked，当前实例的网页授权尚未开始；等待解锁期间继续核定服务存活和生产产品配置。尚未启动正式 8 小时测试，也未进行本轮 GitHub 发布或生产部署。

AnyWork另行负责三项产品改造：统一账号/设备/工作区入口并退役旧独立公网入口；管理工作区权限、业务续期及写入恢复；维护本地Web受限会话、手机交互和换钥确认。旧入口应在替代路径通过目标平台验收后退出，不能只隐藏按钮或在用户工作中直接杀进程。

对外措辞需要同步明确：CyberGrid不保存或解密业务内容，但Logto与协调存储处理必要身份和连接元数据；第三方中继只能承载端到端密文；`online`、发布确认和业务连接成功分别展示；RAM-only不承诺重启后自动远程可用。兼容矩阵须逐平台列实际证据。

| 需求 | 处理决定与优先级 | 当前证据/剩余工作 |
| --- | --- | --- |
| CG-01 | **P0 新扩展**：正式支持外部安全桥接；默认 JS 目录能力保持只读 | 005/007已合入，007两项P1已独立复审闭环；根JS59/59、新旧HTTP各1/1通过。独立实际本地Noise17检查不等于公网/云登录，010整体验收待做。完整浏览器传输另立项 |
| CG-02 | **P0 补齐**：宿主单一 token owner 与持续授权边界 | 002/003及样例官方RAM owner已合入/独立PASS；新鲜token回调不等于实际Gateway业务续期完成，真实云授权正常更新、拒绝与到期验收仍待做 |
| CG-03 | **P0 补齐**：SDK 快照、初始订阅、分层原因及发布确认 | 002/003/008/011及010公共装配已合入；实际仓库链验证发布确认/定时续期/独立截止与unknown，真实外部桥接/手机状态仍待验收。在线不等于业务可用 |
| CG-04 | **P0 已实现底座**：明确产品允许的 SPA/Native 集合 | integration-001 已合入，实际 JWT 验签/Redis 独立12场景PASS；隔离真实Logto Native与SPA登录已有分段证据，完整产品/账号拒绝矩阵仍待验收 |
| CG-05 | **P0 补齐**：复用恢复，增加连接总预算、取消和迟到结果隔离 | Rust首次发布/未知交换、JS wrapper及010公共UI迟到/取消反例已独立闭环；实际云/切网/休眠的完整链仍待验收，不自动重放业务写入 |
| CG-06 | **P0 新扩展**：受信生产入口/provider | 005/006/007已合入；006完整authority及实际非默认端口WSS/mTLS独立通过，根SDK78/78和Lab组合28/28通过。Quick Tunnel仍为测试入口，策略支持不等于正式provider运营验收 |
| CG-07 | **P0 容量安全、P1 展示关联**：分开安装、注册、运行代际与公钥 | 004/006服务端finite-v2与Rust协商已合入；006公开fresh阻断独立复审闭环，未知旧操作仍保守Pending。长期注册整体验收及生产启用未完成，默认legacy-v1仍保留旧限制 |
| CG-08 | **P1 能力、本轮桥接依赖**：版本化完整目录、reset与unknown | 005/006/007及012 console均已合入；unknown同步封gate、权限失败后旧响应不恢复online、010真实HTTP503进入UI已独立验证。云端/手机收敛基线待010，不要求先建常驻推送服务 |
| CG-09 | **P0 基础分类、P1 诊断完善**：固定错误、幂等与预算 | integration-002/003 已合入；服务/宿主完整链一致性待做。未知结果保留原操作事实，不盲目重试 |
| CG-10 | **提升为P0**：默认不开放、本机先关、撤销确认及窗口 | 003/008及010复合关闭已合入；实际Native映射TCP同步封gate、活跃流结束和独立本地服务继续已验证，实机与实际外部Gateway复合关闭仍待010 |
| CG-11 | **提升为P0，已实现底座**：静态最小权限、自部署 Management 参数 | 001和012网站/部署说明/按钮已合入并独立PASS；静态模式不依赖Management M2M，真实隔离Logto配置及生产采用仍待验收 |
| CG-12 | **P0 许可/保留口径、P1 更多平台**：兼容、协商与交付 | 009为历史导出基线；010公共模块已合入，013经独立r2/closeout、合入和根组合复测，准备模块done。后续运行修复后的最终发布仍待验收与重新导出，未改LICENSE或自动授予嵌入权 |

## 后续交付顺序

当前冻结与评审门槛：

- 006初次冻结的[BLOCKED评审](../reviews/integration-006.md)保留，修复冻结`d952d25bbf5900415fa7bbf6b1073fe0c26c3474`已[独立r2 PASS](../reviews/integration-006-r2.md)，合入`26af081`/`cd3e891`；根SDK78/78、Lab+ADB宿主28/28及消费者locked构建通过。保守Pending不属于缺陷。
- 007初次冻结的[BLOCKED评审](../reviews/integration-007.md)保留，修复冻结`a1d891ce04e9f409316a21f689095db846028bfa`已[独立r2 PASS](../reviews/integration-007-r2.md)，合入根分支`d081839`/`0465e75`。根JS59/59、新旧实际HTTP各1/1均零跳过通过；独立本地Noise17检查单列，不等同真实云/公网/手机。
- 011初次冻结的[BLOCKED评审](../reviews/integration-011.md)保留，修复冻结`ee96a179d789ff7a4b2545efe09d5acd16377d69`已[独立r2 PASS](../reviews/integration-011-r2.md)，合入`7ee1acf`/`f3b3a05`；根workspace Rust17/17、官方owner JS5/5及实际前端/原生构建通过。默认factory不可用不能当作远程样例完成，实际创建链仍属010。

1. **Lab关闭链路已闭环，继续实际样例**：integration-008 replacement已独立PASS并合入；文件循环优先关闭/live状态、pending所有权及真实WebRTC/mTLS流结束/本地继续均有实际证据。后续实机控制与样例独立Gateway复合关闭继续单独验收，不能由宿主局部PASS替代。
2. **服务端可信描述已就绪**：integration-005已合入并完成独立复核和根分支全测；只处理provider策略、桥接描述与版本化目录，不代理或解密业务内容。
3. **两个 SDK 接上实际服务**：integration-006 完成 Rust finite-v2、入口与 bridge 发布；integration-007 完成 JS watch 和受控安全适配。各自依赖005，不能仅在类型或 mock 层相接。
4. **共同样例与交付**：integration-011先实现RAM-only桌面owner/受限本地Web，与SDK任务分开路径；前置合入后010装配Mac→同账号Web/Android Chrome实际加密业务及故障/长跑，012分路径统一网站/console、部署说明与真实仓库CI。010保留最终冻结导出/扫描和发布验收，模块或012通过不能代替整体链。验证通过后才更新下载产物和部署；iOS及其他缺设备平台单列未验证。

010公共样例与源码树外实际Gateway/Cloudflare装配、012站点/console/CI已在`e4e4c1b`设计基线上分路径进入实施。公共接口仍须冻结后独立审查；私有装配必须调用同一Tauri宿主入口，不能复制另一套host绕过发布、gate或关闭逻辑。此前loopback Noise互通、模块测试和中间源码导出不计作这轮云登录、公网或长跑验收。

012初次冻结`b605ba9`的[独立评审](../reviews/integration-012.md)**BLOCKED**：应用目录权限失败后，旧设备响应仍可迟到恢复online（I012-P1-01）。实际公开登录控制流反例exit1已保留；当时打包/ESM导入、策略负控、两HTTP及workflow配置的通过未抵消该P1。010先以Rust检查点`76fc9ee`供私有装配，随后补监视/截止竞态、Web及拒绝路径进入最终冻结。

012修复冻结`44cc631`经[独立r2复审](../reviews/integration-012-r2.md)PASS并合入`2ce0dc1`/`e97ad1e`，I012-P1-01关闭；根服务145/145、JS59/59、新旧HTTP各1/1、SDK Rust78/78及原样例17/17、owner5/5、build/clippy/policy实际通过。012按模块done，不含010新Web、真实云/公网/手机/长跑。010公共样例已最终冻结`dca1fe1`（24路径），正独立评审；私有实际装配同步使用此版本，不能将模块22项Rust或Web受控目录通过当作整体完成。

010公共冻结`dca1fe1`的[独立评审](../reviews/integration-010.md)为**BLOCKED**：超限回声未关闭流（I010-P2-01）、旧连接尝试覆盖新成功并遗失peer（I010-P2-02）。两个实际UI/官方授权fixture/真实服务与SDK反例均exit1，作者修复中。原overlap harness强制等待旧错误行为，正确修复后可能提前阻止该行为；原样结果仍保留，新独立验证须确认较新连接持续拥有UI、旧尝试不派发或迟到必关闭，不能把timeout记PASS。

010修复冻结`141dcd17ad66ef4f1e392be1e32668c321ccaf54`仅改5个Web样例路径，已[独立r2 PASS](../reviews/integration-010-r2.md)，独立正确终态9项自然通过，原overflow未改且修后exit0；原overlap未改且在等待被禁止旧行为处timeout/exit1，仍未计PASS。公共模块已合入`7daad58`/`b0831ca`/`652634a`。根组合复测实际样例Rust22/22（include-ignored，零跳过）、Web仓库集成8/8自然exit0、两owner各5/5、两前端build及Rust build/严格Clippy全通过。公开样例内容与冻结一致。私有真实装配已分别观察到Native官方回调/token交换/协调ready、SPA官方登录/目录确认；Native系统浏览器打开和隔离profile经受限RAM交接继续授权为分段证据，不冒称同profile闭环。实际加密业务ready、完整隔离/关闭、长跑和手机仍未通过；010整体仍在运行验收中，013可开始发布准备。

[受控协调时间独立核对](../reviews/integration-010-time.md)已PASS其证据范围：原runner五个运行场景及一个汇总校验6/6、独立额外三个公开HTTP/SDK边界场景3/3均自然exit0。实际MemoryStore原生clock、HTTP/ES384与公开JS SDK覆盖逻辑24小时到期/同身份恢复、精确准入/租约/descriptor边界；JWT仍按真实墙钟验证，Native仅HTTP而非Rust。真实Redis TIME负控证明handler时钟跳变不会推进Redis到期，因此不计实际24小时、Redis24小时回收、Rust自然恢复或OAuth轮换；原私有fixture失败与限制保留。

最终发布准备拆为[013](../tasks/integration-013.md)：公共模块合入后补真实Web CI、精确配置示例/评审allowlist、v0.8.0及组件版本/兼容说明和完整仓库源码包。010继续持有云/公网/手机/长跑及最终公开映射门槛。只读history审计确认待上传本地历史含旧机器信息，后续从实际公开基线生成经审快照提交，不直接推送本地main。

013候选`5a7792f9da206f3c127a26d161fcb94d156391bb`经[独立r2复审](../reviews/integration-013-r2.md)PASS。原组件源码归档绕过统一公开策略的I013-P2-01已关闭，原反例保留且修后通过；实际13个产物、完整源码2,977文件、ESM导入、策略/打包/许可与导出扫描已有独立证据。本结论只覆盖候选发布准备；报告自身尚未纳入该冻结，需exact纳入后复核再合入，不标任务done，不代表Actions、原生包、生产部署或010整体验收通过。本次整理所改文档也不自动属于既有冻结，最终导出需重新绑定。

013后续小冻结`5deb038`已实际纳入报告并经独立private closeout PASS，根合入至`69cbfa3`并完成组合验证：打包/策略/许可8/7/2、117/39包许可核验、四Node构建、Web owner5/实际集成8全exit0；根13产物、完整源码2,978文件及五源码归档逐hash/inventory/内外manifest相等，八ESM/type实际解包import、plan/export/scan全通过。013按准备模块done；其后010运行修复和文档变动必须在最终发布重新冻结导出，不继承旧快照为新扫描结论。

真实装配后续已观测三次HTTP200/DTO成功（info、一次显式write、list），首轮读取失败未复现，代理候选已否证。流收到48字节JSON对象，其字段精确为type/data、input类型、data与合成文本相等，plainText不相等，定位为公开合成本机上游原样回传input封套的协议错误，按010修复。该连接后续closed，未发生第二次fresh/业务鉴权接受，不计正常续期；该阶段尚需绑定修复并分别核关闭原因/descriptor续期/lease/业务auth。截至该次运行尚无流成功、30分钟/8小时或手机互通结论，后续恢复运行另记如下。官方Native登录仍为系统打开与隔离profile继续授权的分段证据；测试用户为管理端预置，未验证真实邮件注册投递。

2026-11-11 echo修复已合入至`9c5c7f0`：严格接受Text JSON的字符串`type:"input"`及字符串`data`，拒绝额外/重复/错误字段及Binary业务输入，回复`data`的UTF-8 Binary；保留鉴权、大小及有界关闭约束。[初次BLOCKED](../reviews/integration-010-echo.md)与[修复后独立r2 PASS](../reviews/integration-010-echo-r2.md)均保留。根合并后已有完整样例Rust26/26（include-ignored、零跳过）及源码策略7/7通过记录。本次仅核对记录，未重复运行。该子缺陷已修复，010完整公网链、关闭、持续授权、长跑与手机验收仍未完成。

同日真实恢复链进一步验证：旧Native通过公开退出入口完成remote_disable/owner_end后正常退出；旧浏览器控制/infra进程已结束，未伪造SPA正常logout回执。新隔离基础服务、官方Native/SPA RAM owner及实际二进制绑定源码`1fa5547`（运行代码同`9c5c7f0`），在真实Quick Tunnel、Noise及实际JwtVerifier上完成info/一次显式create/list三次HTTP200与DTO校验，公开UI精确UTF-8 echo通过，私有adapter无封套解包。这是短链公网流成功，不是完整续期或长跑通过。同连接在鉴权后138.785秒、最后流数据后125.004秒报告`CONNECTION_CLOSED`，没有context-aborted，且未观察到Gateway/隧道子进程退出；原因仍为未确定，不能写成Cloudflare固定超时。已分离保存短链成功和后续关闭事实；新连接持续读/echo的观察不得与此连接混计。30分钟、8小时、拒绝续期、复合关闭和Android验收仍待完成。

另一次官方SPA新owner/新连接持续390.45秒，完成35次HTTP与35次精确echo、仅一次authenticated；实际JwtVerifier的新token/同subject/更晚deadline检查通过，9轮业务发生在该连接原授权截止之后。只记录为源码支持的间接ACK证据，未回溯计正式时间。公开RemoteTransport没有已接受续期的事件，初版源码位置observer独立反例发现`armLease`拒绝后也到达同一void-return，原BLOCKED保留。修订版改为成功路径末点及validated-renew调用者位置，已独立r2 PASS其机制范围：RPC8项及实际CDP4组通过，拒绝/initial/其他caller/unknown不计，精确bundle位置及预算核对；尚未挂入真实owner，不代表已取得真实ACK。原反例字节不变，修后旧descriptor提前拒绝导致其旧ARMED断言exit1，未称原样PASS。不能用增加日志而未排除失败分支的方式宣称续期成功。

隔离loopback诊断已实际构建当前interop并运行：未供fresh callback，空闲连接保守190.0167秒无额外数据帧后原连接GET200；active对照19次既有只读请求及最终GET200。实际Noise、源/锁/二进制、时间及清理证据已独立核对PASS；该次独立核对为证据审查，不增加一次运行计数。结论只支持此本地协议路径不存在无条件125秒关闭，不包含公网、真实Logto、SDK目录/wrapper或keepalive修复。周期业务读取只作对照，不作为通用空闲保活实现。公开接口无ping配置，未知Noise命令或伪造授权续期均不能作为修复。

发布快照私有helper已完成开发及独立r2 PASS：原Path排序不符和报告失败残留候选ref两项P2已修，原失败证据保留；实际exporter/扫描器、精确blob/mode、单公开父提交及事务异常清理有受控证据。此结果只完成本地候选构造工具的准备，不代表最终源码freeze、真实候选、公开提交、Actions、下载或部署通过。界面可用性按观测分别记录：normal实例后续AX可读并完成上述登录失败与正常收尾，不能继续用旧锁屏记录阻断Mac步骤。手机最近一次ADB读取为锁定/显示关闭，默认网络具备INTERNET与VALIDATED，尚无手机应用或浏览器操作。保留基础服务的实际存活状态也须重新核定，不能从旧状态文件推断。

## 责任边界与本轮不扩展的范围

CyberGrid 负责产品绑定、目录、协调、SDK 生命周期与安全扩展契约。AnyWork 负责工作区权限、短期业务授权更新、HTTP/PTY 恢复、受限本机会话、界面统一和旧入口退役；原300秒业务授权未续期与图片路径错误不归为 CyberGrid 缺陷。现有 WebRTC/iroh/Cloudflare 传输保留，不新增云端业务内容库、另一套账号系统或跨账号分享。

需要所有者明确的产品决策是 SDK 嵌入/修改/再分发许可；本轮技术整理不修改 LICENSE。生产 provider 的运营责任、支持平台与可用性限制须在交付矩阵列清，不把域名允许策略等同于正式入口已经可用。

跨模块路径必须验收：产品配置→官方登录→服务授权；宿主 token owner→SDK→交换；Rust 发布确认→JS 目录；目录→安全桥接→宿主业务授权；关闭→活跃流→撤回；安装关联→新运行身份→fence退役；SDK 快照→产品 UI。

持续测试需分别记录 30 分钟跨至少六个短业务授权周期、8 小时/10,000 混合请求及协调会话到期测试；手机浏览器的 Wi-Fi/蜂窝/挂起恢复不以原生 SDK 结果替代。缺少平台或运行证据时保持未完成，不将分析方案当已交付能力。
