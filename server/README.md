# CyberGrid 协调服务与网站

身份和应用由Logto管理。设备、短会话hash、端口、证书指纹、信令和限流只存唯一owner的有界RAM；不使用PostgreSQL、Supabase、磁盘快照或Logto customData保存协调状态，也不接收或转发应用业务内容。

网站和API入口运行于Vercel；固定受信HTTPS代理把协调请求交给一个Node owner。Vercel实例不共享RAM，不能在Function中创建MemoryStore副本；上游不可用时明确503。完整参数和Vercel一键部署按钮见 [部署文档](../docs/deployment.md)。

## Owner本地启动

需要Node.js22+、POSIX Python3标准库fcntl。`tsx`是固定运行依赖，production安装也会包含它。

```sh
npm ci
cp .env.example .env
chmod 600 .env
# 填写自己的Logto、公共origin和独立密钥
node --env-file=.env --import tsx src/owner.ts
```

默认只监听 `127.0.0.1:7400`，可通过 `CYBERGRID_BIND`／`PORT` 修改。公开入口由可信HTTPS反向代理／隧道转发到该端口；不要直接暴露无proxy认证的业务端口。

`CYBERGRID_OWNER_LOCK`指定固定租约目录，生产建议使用唯一绝对路径。Python helper持有固定inode的POSIX flock，Node死亡后stdin EOF使helper释放内核租约；不按陈旧PID删除锁文件，也不手工unlink lease。一个部署不能同时运行Node cluster或多个owner。

`npm run dev`是独立loopback开发服务，网站和API同端口，默认7300；仍需要真实Logto，不与production owner共同接收同一组客户端。`CYBERGRID_DEV_MODE=1`仅加强loopback限制，不启用假登录，生产／Vercel拒绝该开关。

## 配置职责

owner私有.env需要：

- `LOGTO_ISSUER`：固定HTTPS issuer，允许包含真实issuer路径。
- `LOGTO_BROWSER_CLIENT_ID`：独立SPA控制台应用。
- `LOGTO_MANAGEMENT_CLIENT_ID`／`LOGTO_MANAGEMENT_CLIENT_SECRET`：读取固定租户目录的M2M凭据。当前Logto权限较宽，仅保存在owner。
- `CYBERGRID_PROXY_SECRET`：32–256 UTF8字节，需与Vercel相同。
- `CYBERGRID_PUBLIC_ORIGIN`：公共HTTPS origin，用于受信请求重建，不能信任转发Host头。
- `CYBERGRID_ADMIN_KEY`：可选独立32–256 UTF8字节管理密钥，留空时后台接口明确禁用。
- `LOGTO_RESOURCE`：可选，默认公共origin加`/api`，JWT验签audience始终固定于配置。
- `CYBERGRID_CONSOLE_REDIRECT_URI`：可选同origin回跳，默认`/console/callback/`；根路径及`/api`、`/admin`保留路径不可用，自定义时须在owner、Vercel与Logto SPA配置相同值。

Vercel只需要 `CYBERGRID_OWNER_URL`、`CYBERGRID_PROXY_SECRET`、`CYBERGRID_PUBLIC_ORIGIN` 和公开 `LOGTO_ISSUER`（用于精确浏览器CSP）；如自定义callback，再配置相同 `CYBERGRID_CONSOLE_REDIRECT_URI`。不把M2M secret或admin key传给Vercel前端。`CYBERGRID_OWNER_URL`必须是无path／query／凭据的固定HTTPS origin。

`CYBERGRID_MEMORY_LIMITS`可选覆盖有限容量；未知键、非正安全整数等错误启动失败。注册、会话、设备、信令数量和字节、限流及长轮询等待者都受限；容量不足明确429或503及有界Retry-After，不悄悄驱逐有效注册fence。

## 官网与构建

```sh
npm ci --include=dev
npm run build
```

构建检查TypeScript、Middleware和静态页面，不下载或打包SDK大产物。首页及 `/docs/` 提供接入和部署说明，所有SDK／App下载转至 [GitHub Releases](https://github.com/ywateru/CyberGrid/releases)。Vercel模板以本目录为根目录，可单独部署网站／代理。

`/console/` 使用独立Logto SPA Authorization Code+PKCE，只读查看当前用户设备；console session不注册设备、不发送信令。token及中央session仅留页面RAM，sessionStorage仅保存短期PKCE/state/nonce。退出立即清本机状态，并如实显示中央注销是否确认。

`/admin/` 使用 `.env` 的管理密钥。页面RAM保存密钥，经同源Authorization Bearer请求 `GET /api/v1/admin/overview`，不写URL、cookie或local/sessionStorage。退出或刷新重新输入，失败清除旧成功状态。

后台显示协调层有效会话、设备在线／离线、端口、排队信令和等待者，按应用／在线状态分页（默认50、最大100）。不返回session hash/key、用户邮箱、注册密钥、证书、完整信令或tunnel URL。在线表示心跳有效，不代表业务TCP已经建立；后台不能查看应用内容。缺密钥503、错密钥401，可信IP每分钟限30次，所有响应no-store。

## 协议与恢复

`GET /api/v1/auth/config?appId=真实NativeID`返回Logto和memory-v1配置。Native ID大小写敏感，严格等于验签JWT的client_id。应用只在Logto管理，旧密码登录和应用CRUD接口返回410。

每次boot生成新epoch。设备交换使用私有registrationKey、严格递增registrationRevision及幂等requestId；同逻辑设备的新revision产生新incarnation，旧请求不能夺回设备。心跳按publishRevision提交完整端口／指纹／回退快照，信令包含当前双方incarnation、epoch与UUID去重。

所有响应包含 `X-CyberGrid-Epoch`，受保护客户端回送epoch。长轮询最多15秒、取消释放等待者；代理从入站body开始贯穿25秒总预算，Vercel Function预算30秒，请求body和响应均流式限长。

owner重启清空协调RAM，SDK通过已保存身份重新注册和发布；原映射可保留监听意图，恢复后新的TCP重新握手。旧流可能中断，不缓存或重放业务字节。会话撤销、应用失效、身份改变或SESSION_SUPERSEDED须停止并提示登录。

## 开发检查

```sh
npm run typecheck
npm test
```

测试显式注入Identity／Directory fixture，不在production提供身份fallback。管理后台新增检查位于 `test/admin.test.ts`。真实WAN传输、Logto授权及设备测试应另按实际环境记录，不能用本地fixture或在线列表替代成功证据。
