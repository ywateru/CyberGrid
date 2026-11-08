# 部署与参数

CyberGrid 不使用数据库。完整部署包含一个单进程内存 owner、Vercel 网站／API 代理，以及自己的 Logto 租户。Vercel 按钮只部署网站与代理，不能代替 owner 宿主。

## 准备 Logto

在 Logto 创建并配置：

- 启用 Device Flow 的 Native 应用，公开 Application ID 传给 SDK／Lab。
- 用于个人设备控制台的 SPA 应用，允许回跳 `https://你的域名/console/callback/`、退出地址与站点 origin。支持公开注册时，在 Logto 开启相应登录／注册方式。
- 标识符为 `https://你的域名/api` 的 API resource，配置本项目使用的 `connect` 权限，允许 Native／SPA 获取该 resource 的 access token。
- 读取应用目录的 M2M Management API 凭据。当前 Logto Management API 的 `all` 权限较宽，须只保存在 owner；运行代码只对固定租户执行目录 GET，不将凭据下发客户端。

确认租户方案支持自定义 API resource 和所需配额。issuer、resource 和 client_id 必须精确一致，不能用其他项目的公开 ID 代替。SMTP／邮件验证在 Logto 中配置；不要把 SMTP 密码放到 SDK、前端或 Vercel public 变量。

## 运行单一 owner

需要 Node.js22+、POSIX Python3 标准库 `fcntl`、可供 Vercel访问的可信 HTTPS入口。不运行 Node cluster 或多实例负载均衡。克隆源码后：

```sh
cd server
npm ci
cp .env.example .env
chmod 600 .env
# 编辑 .env 后启动
node --env-file=.env --import tsx src/owner.ts
```

owner 的 `.env` 参数如下。所有秘密均填写自己的值，不提交真实文件。

| 参数 | 用途 |
| --- | --- |
| `LOGTO_ISSUER` | 租户 HTTPS issuer，无末尾 `/`，对应可信 OAuth 端点 |
| `LOGTO_BROWSER_CLIENT_ID` | 个人控制台的 SPA Application ID |
| `LOGTO_MANAGEMENT_CLIENT_ID` | 读取目录的 M2M client ID |
| `LOGTO_MANAGEMENT_CLIENT_SECRET` | M2M 私密凭据，仅 owner 保存 |
| `LOGTO_RESOURCE` | 可选 API resource；默认公共 origin 加 `/api` |
| `CYBERGRID_PUBLIC_ORIGIN` | 外部站点的 HTTPS origin，不含路径，须与 Vercel域名及 Logto回跳配置匹配 |
| `CYBERGRID_PROXY_SECRET` | 32–256 UTF8字节随机秘密；owner与Vercel必须相同 |
| `CYBERGRID_ADMIN_KEY` | 独立的32–256 UTF8字节管理密钥；不复用proxy秘密；未配置时管理接口禁用 |
| `CYBERGRID_OWNER_LOCK` | owner宿主上的唯一绝对租约路径，保持固定，不手工删除lease文件 |
| `CYBERGRID_CONSOLE_REDIRECT_URI` | 可选同origin回跳地址，默认公共origin加`/console/callback/`；根路径及`/api`、`/admin`保留路径不可用 |

使用 `.env.example` 中的实际端口参数配置监听。默认loopback，需要可信反向代理或HTTPS隧道将公开入口转发到它。Quick Tunnel可用于测试，但没有SLA；长期运行应选择稳定入口。固定owner上游必须是HTTPS origin，不能包含用户名、密码、路径、查询或片段。

生成密钥可使用 `openssl rand -hex 32`，分别生成proxy和admin密钥，写入私有文件；不要在命令记录、截图或公共工单中粘贴秘密。后台入口为 `/admin/`，密钥只在当前页面内存保存。

## 一键部署 Vercel

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2Fywateru%2FCyberGrid%2Ftree%2Fmain%2Fserver&env=CYBERGRID_OWNER_URL,CYBERGRID_PROXY_SECRET,CYBERGRID_PUBLIC_ORIGIN,LOGTO_ISSUER&envDescription=Configure%20your%20owner%2C%20proxy%20key%2C%20public%20origin%20and%20Logto%20issuer&envLink=https%3A%2F%2Fgithub.com%2Fywateru%2FCyberGrid%2Fblob%2Fmain%2Fdocs%2Fdeployment.md&project-name=cybergrid&repository-name=cybergrid)

模板目录为 `server/`，使用该目录自己的package与Vercel配置。若直接导入整个仓库，手动将Root Directory设为`server`。按钮不需要GitHub PAT或任何现有部署的秘密。

| Vercel参数 | 填写内容 |
| --- | --- |
| `CYBERGRID_OWNER_URL` | 已运行owner的固定可信HTTPS上游origin |
| `CYBERGRID_PROXY_SECRET` | 与owner相同的私密proxy密钥 |
| `CYBERGRID_PUBLIC_ORIGIN` | 要对外使用的站点HTTPS origin |
| `LOGTO_ISSUER` | 与owner相同的公开issuer，供Middleware精确配置浏览器连接策略；不含M2M秘密 |
| `CYBERGRID_CONSOLE_REDIRECT_URI` | 可选，自定义回跳时与owner及Logto SPA相同，默认无需配置 |

Vercel上不需要数据库URL、Logto M2M secret或后台管理密钥。创建项目后绑定目标域名，核对公共origin、Logto SPA的回跳／允许origin；修改issuer后重新部署。SDK始终填写公共站点地址，不直连私密owner端口。

管理后台密钥放在owner `.env`；访问`/admin/`时输入它，由同源代理传给owner校验。HTTPS代理认证与管理身份是两层独立校验。普通用户使用`/console/`的Logto登录查看自己的设备，不能调用管理API。

## GitHub Actions 与发布

仓库的Actions根据tag或手动触发构建macOS arm64与Android arm64测试产物，汇总SDK／App源码、provider、许可证及SHA256SUMS到GitHub Release。构建不需要Logto用户凭据或部署密钥，发布使用仓库作用域的`GITHUB_TOKEN`。网站链接直接指向GitHub Releases，不再在Vercel构建时下载和托管SDK大文件。

源码公开前排除本机环境、真实.env、签名私钥、账号／设备测试记录、缓存和旧部署材料。正式签名、公证、商店发布及长期owner监控需要另外配置；不要把debug-test包描述为正式商店发行。
