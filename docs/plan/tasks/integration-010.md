---
id: integration-010
scope: common sample and release acceptance
status: in-progress
depends-on: [integration-006, integration-007, integration-008, integration-009, integration-011]
release-gates: [integration-012, integration-013]
---

## objective

打通 Mac RAM-only 宿主、桌面 Web 与 Android Chrome 的共同样例，消费实际 Rust/JS SDK 和服务；完成真实登录、业务桥接、故障/长跑及发布准备。不能以 SDK 单元测试、已有 AnyWork 源码或旧版原生手机结果替代这条链。先验证再上传/部署，保持元数据和许可边界。

桌面授权owner与本地业务模块由011先行交付；本任务负责把它们接到006/007及真实外部桥接，完成完整用户链，不减少原验收范围。

网站/console、仓库CI及初步源码导出入口由012独立交付，实施路径与样例分离；本任务保留整体运行验收、最终源码冻结后的导出/扫描、版本/下载核对及实际发布部署门槛。012与样例装配可以在前置合入后分路径推进，只有其完成后010才能结束最终交付。

012模块已通过；实际新Web的最终CI、版本、源码策略和完整仓库归档由013在公共样例独立通过并合入后完成。013只做发布准备，最终运行与公开发布仍由本任务负责。原始反例若在到达断言前强制等待旧错误行为，修复后保留原文件并原样记录实际timeout/exit，再以独立新反例验证正确终态；不得为迁就旧harness放宽契约，也不得把timeout称为PASS。

`depends-on`是开始装配前必须已合入的真实模块；`release-gates`是完成发布前必须已通过的独立交付，不阻止不同路径的样例开发。012未完成时本任务不能进入最终发布或标done。

## context

- `docs/integration-contract.md`、`docs/browser-bridge.md`。
- `docs/common-sample.md`：样例模块、创建链、唯一RAM owner与本地/远程Web边界。
- `docs/metadata-and-compatibility.md`、`docs/lab-remote-controls.md`。
- `docs/plan/analysis/integration-progress.md`、`docs/releasing.md`。

## path

`examples/ram-only-workspace/`共同样例及其使用/验收说明、最终交付证据与发布执行。网站/console由012负责，最终CI/策略/版本/归档由013负责，不与其并行改相同路径。外部桥接验证装配/证据放源码树外，不进入公开产物。遵守共同样例模块合同；不得改写外部 AnyWork 源码、复制 AGPL 实现或新增中央业务票据服务。跨模块缺陷按原 owner 修复，不在样例里绕过校验。

## verification

1. 真实模块链：产品配置→官方 Logto SPA/Native 登录→Rust RAM-only owner→准入/发布→JS directory/watch→安全 adapter→实际宿主业务授权。无 token/地址/端口手工输入；本地业务在未登录/云端不可用时可用，保持 origin/CSRF/loopback 身份边界。
2. 发现/发布确认/业务可用分层；目录目标5s收敛测基线，连接总预算60s，取消1s内结束UI等待并验证迟到结果关闭。换用户、错issuer/resource/client/scope、公钥变化、安装关联、新epoch、休眠/断网/旧close/关闭远程全部拒绝或明确恢复。
3. 真实加密业务长跑：30min跨至少6个短授权周期、正常与拒绝续期；8h且超过10,000混合请求及活跃流，记录实际运行时长、内存/去重有界和无自动写重放。24h协调/准入到期另用受控时钟真实存储/SDK或独立长跑，不能将短模拟称为8h运行。
4. Android Chrome 的 Wi-Fi/蜂窝/切网/锁屏返回实测；iOS Safari 或其他缺少设备的平台明确列为未验证/不承诺，不用 Android native 结果填空。既有原生 WAN 任务继续单列。
5. public site/README/部署按钮与当前静态策略一致；console 目录不可达时显示unknown而不是保留旧online。兼容矩阵、版本号/下载包清晰，源码导出新增文件全纳入且无本地隐私，许可证保持已授权边界。
6. 只有实际达到本次支持范围且独立评审通过后，才能按原用户授权提交/发布 GitHub、Actions 构建与部署正式域名；生产验收和回滚点单独记录。缺失平台/许可/登录条件如实保留，不用mock通过结案。

## execution evidence

实施与验收采用三份独立绑定：公共样例冻结提交、外部Gateway/RemoteTransport/JwtVerifier相关源码哈希、私有装配与runner哈希。公共样例改变后不得沿用旧装配的通过结论；外部仓库仅无关文档变化时仍复核相关文件哈希。最终发布另记录本地冻结→经审导出tree→公开提交→Actions/下载包→正式部署的映射。

30分钟和8小时运行从真实业务ready起使用单调时钟计时，不能从编译或等待登录开始计。至少记录连接代际、成功读/显式写/流计数、校验结果、已接受授权的顺序号与到期推进、旧到期后同连接成功、最大队列/去重大小及内存采样；只记录计数和固定原因，不保存token、私钥、原始业务正文或本机文件路径。令牌回调成功数不能代替实际`auth.renewed`与业务成功证据。拒绝续期单独运行，不能拿关闭后新建连接继续累计成同一条正常续期连接。

云账号登录、公网Cloudflare、真实浏览器、受控授权fixture及手机网络分别标注。现有实际Gateway将业务授权上限裁为五分钟，正常长跑可使用其真实JwtVerifier及官方owner强制刷新取得不同资源token，无需为凑周期放宽授权或修改既有生产resource。若只能完成合成轮换验证器路径，只记为受控协议证据，真实云持续授权仍待验收。

手机浏览器验收开始前记录ADB实际可用设备及网络前提；无设备时保持未验证，不以桌面浏览器viewport或Android原生测试代替。对已存在的登录owner、grant或进程先核归属，不为占用回调端口直接杀进程、清应用数据或转移凭据。云测试只新增独立测试资源/客户端/用户及隔离协调prefix，不修改生产应用、签名密钥、既有授权或正式域名。

实际公网装配后续已观测官方登录、发布、Noise/业务鉴权和三次成功HTTP/DTO操作（info、一次显式write、list）；UTF-8 echo未通过，收到的是上游原样回传的input JSON封套。须按`common-sample.md`本机terminal输入协议修复公开`gateway-upstream`及实际WebSocket测试，不在私有adapter猜测/剥离终端输出。实现冻结后独立复核并更新私有binary/source绑定，再开始新的实际链与长跑；既有读取失败未复现但原因未证，不将其归因于已被正反例否证的系统代理。现有连接随后关闭且尚无授权renew证据，仍需诊断租约/descriptor/授权各层关闭原因，不能将首条stream-data记为流验收通过。

2026-11-11子缺陷进展：上述echo修复已通过[独立r2复审](../reviews/integration-010-echo-r2.md)并合入`bdaefb3`/`4f7ab46`，报告纳入至`9c5c7f0`；既有根分支完整样例Rust26/26（include-ignored、零跳过）与源码策略7/7均exit0。此条更新将“需修复”收敛为“需重新绑定实际装配并验收”，不修改010的in-progress状态。实际公网流成功、复合关闭、正常/拒绝持续授权、30分钟/8小时与手机验收仍待完成；已知分段登录证据及早先失败保留，不补写为完整闭环。

同日恢复装配绑定`1fa5547`后，新的官方Native/SPA登录、实际Quick/Noise/JwtVerifier、三次HTTP200/DTO及公开UI精确UTF-8公网echo已通过，原“实际公网流成功”子项获得短链证据；adapter未猜测或剥离输出。该同连接在鉴权后约139秒报告`CONNECTION_CLOSED`，无context取消或Gateway/provider退出的观测，关闭原因未定且未达到正常续期。后续持续流量的正常续期、单独标明fault-injection的业务验证器拒绝，以及复合关闭/本地继续均须分别留证；不把测试policy拒绝冒充Logto云撤权，不在新连接上累计旧连接时长或续期数。010整体保持in-progress。

验收准备后续：正式runner r2独立复审发现请求query的真实接口不兼容，实际SDK→HTTP服务200后被目标observer错误拒绝，报告SHA为`ff4d55e6320afbac15c6063a735804716cfb48c9d85313a0cf6fd05f2546dde3`；r2不得投入运行。r3在新目录仅修正该query，实际SDK/服务组合和原锁定反例通过，已独立PASS离线机制范围，报告SHA为`ad8ce2e09290beae473df7d586ad242ab6f7f2779dc7c380fde033d38cc24893`；真实gate与运行仍待核定。旧Native已正常关闭远程、结束owner并退出；其后二进制逐字节保留。首次locked/offline构建因依赖锁需更新而exit101；一行路径包版本修复经隔离解析及独立核对后，新build exit0且已绑定新二进制，未替换原bundle。私有拒绝策略的源码/构建准备不能视为实际拒绝通过。接下来核新实例、日志、目录目标和唯一连接，再执行新的正常/拒绝及关闭验收；当前工具、构建和运行门槛统一见[覆盖表](../analysis/integration-progress.md)。

## publication gaps and disposition

以下问题在010建立时记录，保留其验收来源。网站正文/部署按钮、console权限失败和迟到响应已由012修复、独立复审并合入；旧BLOCKED报告保留。现行剩余准备以013为准：补实际新Web CI、版本/锁/兼容矩阵、精确源码清单及完整仓库归档。历史问题描述不能继续用作当前根分支缺陷结论，最终运行与实际发布门槛仍由010持有。

`server/public/index.html` 仍介绍目录M2M为默认必配，Vercel按钮仍要求Management密钥，与已更新的 `docs/deployment.md` 静态产品推荐不一致；本任务须统一正文、按钮参数和静态文档页面。旧v0.7下载的版本说明应保留其实际能力，新能力只能绑定新版本及真实发布产物，不能直接替换旧版描述。

控制台 `refreshDevices` 当前失败只显示消息，仍可能保留旧online卡片；应用切换时也须隔离上一产品迟到的读取结果。改造时按当前已选择产品/页面代际处理响应，失败先使可用结论unknown，不能只靠错误横幅，也不能把上一产品目录画到新产品下。

Rust单独源码包不带服务端/Node/Redis，其仓库集成测试可以明确标为外部依赖；完整仓库CI与最终验收必须安装依赖并使用 `--include-ignored` 执行这些真实集成，不能沿用默认cargo test跳过后宣称覆盖。最终报告分开SDK独立测试与仓库完整链计数。

当前Actions仅运行JS旧`test:integration`和原生打包，未执行006的真实仓库集成或007新增`test:bridge-integration`。最终CI须安装实际Redis、设置`REQUIRE_REDIS=1`，构建服务与两个SDK后执行相应完整链，并让目录/策略/SDK相关源码变化触发验证；新样例模块有自己的构建和验收入口。公开CI不拉取或打包私有AnyWork装配，也不以公开生命周期替身替代另行记录的实际Noise/云登录/手机验收。

最终source policy须显式收录common-sample、task010/011/012及006-r2/007-r2等新增评审和实际样例源码。integration-009的冻结基线检查不覆盖后来新增文件；最终freeze后重建导出计划、隐私扫描、许可/依赖核对及下载哈希，不能沿用旧报告。

源码公开还须审查即将上传的commit范围，当前tree扫描不能证明历史无本机信息。未发布的本地评审历史包含后来已脱敏的绝对路径，不能直接把整条本地分支推送并声称已完成历史检查。最终保留私有原始证据；必要时从已公开基线生成经审导出快照的发布提交，记录本地冻结、导出tree及公开commit映射并验证实际公开内容。不为清理本机信息改写已发布历史，不能把GitHub上传成功当作源码一致性验收。
