# integration-005 独立评审

结论：**PASS**。绑定冻结提交 `70bfb807601f61ba6d65ef5a2d0354ab78a6e02d`，开发基线 `b46f9b2e95a6a19727a47a9acd68e5efab5c7d10`。隔离树 clean；15个源码、测试、环境示例及 server README 路径逐项匹配开发 manifest SHA256，5份开发日志哈希一致。没有修改实现。

## Findings

没有发现阻塞或需单列的非阻塞问题。原2个预算测试把非法超长单DNS label替换为显式允许的合法自有origin/path，仍保留完整24KiB和2MiB拒绝断言；新增Quick label的63/64字符正负控制，没有放宽URL校验。

根合同较冻结分支另外明确了“新旧目录完整响应2MiB，包装字段计入，不截断、不隐式分页”。该说明与冻结实现一致，不构成合同缺口。评审覆盖的是服务端协调元数据能力，不代表JS/Rust桥接适配器、实际业务加密桥接、浏览器或线上已经支持。

## 源码审查

- 完整审查15条变更路径，包括provider配置、规范URL/bridge校验、服务路由、CORS、Memory/Redis采样和历史、Lua事务及测试修订。
- provider配置固定、严格解析且失败关闭；内置Quick规则不能覆盖。设备HTTPS根入口、端口HTTPS path和bridge显式WSS入口分别校验，不通过parser消除的默认端口、dot segment、编码、反斜线等放宽原始文本。
- 目录在Store单次原子采样中重新检查已存储session，使用已存owner/product及同一次Redis TIME；服务按当前角色和入口策略作最终白名单投影。Console不返回iroh，admin保持原白名单，私有修订历史和精度字段不公开。
- scope HMAC按合同分域并绑定epoch、owner、product、策略及角色投影。内容hash只排除lastSeen与publishRevision确认计数，完整快照仍返回实际值；策略、lease、入口期限和实际元数据变化仍参与有效内容。
- 新publishRevision在Lua提交时校验bridge期限和有界历史，拒绝检查先于设备快照写入。完全相同的旧revision可以在地址到期后确认，但不延长期限；撤回不能消除本incarnation的历史。Node保留规范bridges JSON与十进制精确revision字符串，避免Lua cjson重编码丢失安全整数精度，公开DTO仍为number。

## 独立实际验证

在源码树外运行独立反例脚本，直接导入冻结实现；使用随机隔离前缀、独立本地AOF Redis、两个RedisStore及4个loopback HTTP handler（含策略收紧和EVAL延迟变体）。身份通过实际LogtoIdentity、本地JWK及ES384签名验证；没有替换为直接信任任意token的Identity。

结果：**15个场景PASS，0 fail / 0 skip，退出码0**。

1. 同owner/product的Native/Console设备集合正确，跨账号、产品及错误角色不能扩权；共享SPA只查看当前允许产品。
2. 独立重算分域HMAC；跨角色、账号和产品cursor reset。
3. 心跳带回更高确认revision但不改变cursor；其他账号变更不影响当前目录。
4. 新旧Native/Console及admin五类公开投影均不返回私有history、精度字段、session hash或seed；Console/admin不泄漏iroh地址。
5. 收紧provider后四种新旧目录都移除不再受信入口，并重置策略scope。
6. 同addressRevision仅延长expiry时更新内容hash，不改变地址身份。
7. 撤回后不能用同地址版本换key或缩短expiry；累计四个历史id上限保留，拒绝第五项不推进发布revision。
8. 通过两HTTP/Store交叉发布、重放MAX_SAFE_INTEGER-1/MAX_SAFE_INTEGER，读回精确数字；较旧发布和同revision换身份被拒绝。
9. 等待真实Redis时间越过bridge到期后，同publishRevision重放仍确认但目录过滤地址；新revision重用过期地址拒绝且不改旧revision。
10. Node到EVAL之间注入延迟，使地址在提交前过期；Redis提交拒绝，不被Node解析时的旧时间放行。
11. Redis记录越过lease边界后online变为false，有效目录hash改变。
12. HTTP预检后、原子采样前由另一Store撤销session，目录返回401且没有devices。
13. 原始URL规范化绕过及64字符Quick label被拒绝。
14. 实际HTTP注册回执重放不清历史；新incarnation才重置；旧session关闭不会删除新descriptor。
15. 在测试装配中禁止所有非fixture HTTP目标，验证发布、过滤及读目录没有对provider地址发起fetch或redirect。

实际命令使用已安装的tsx执行私有 `counterexamples.mts`，设置 `REQUIRE_REDIS=1` 与明确Redis binary；未使用线上环境或凭据。初次装配因测试ProductPolicy漏列canonical NativeID而在测试断言前被正确拒绝；该诊断日志已保留，修正测试配置后最终15场景完成，不能将此配置错误归为产品失败。

开发另提供123/123全server测试、真实Redis零skip、build/typecheck，以及最后endpoint/budget专项日志。独立评审核对源码与这些实际日志绑定，未把开发计数并入独立15场景，也未重复全量构建。

## 证据与边界

私有证据在源码树外 `integration-005-independent`，包括冻结绑定、独立脚本、完整实际输出和 `review-final.json`。Redis与HTTP fixture收尾已等待，结束检查无遗留redis-server；冻结树仍clean。

本评审未进行部署、生产Redis变更、真实Logto Cloud用户登录、手机、Wi-Fi/4G、provider公网或浏览器加密业务互通。不能恢复升级前已由历史cjson舍入的数字，不能约束仍运行旧provider策略的公开部署；上线收紧时仍须退役旧实例。SDK目录watch与真实外部adapter属于后续任务，不由本PASS代替。
