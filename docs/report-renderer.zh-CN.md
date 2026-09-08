# Chromium 报表渲染器

Chromium 适配器以预览形式实现。本地 PDF 和 PostgreSQL 生命周期检查已通过。生产上线前必须通过 Linux 容器验收；当前 macOS 主机没有 Docker 安装，因此尚未运行。Capture 仍是默认。

```yaml
modules:
  report:
    renderer: chromium
```

重新生成项目。生成的 `report-renderer` 目录包含固定的 Node 依赖、渲染器、Dockerfile、上游 Playwright seccomp 配置以及 Compose overlay。在准备好生产后端和环境后，部署组合为：

```sh
docker compose -f compose.production.yaml -f generated/report-renderer/compose.yaml config
docker compose -f compose.production.yaml -f generated/report-renderer/compose.yaml up --build -d
```

后端通过 `APPSTRUCT_REPORT_RENDERER_SOCKET` 连接。mode 为 0660 的私有 Unix socket 由 UID/GID 10001 共享。渲染器没有后端环境文件、数据库凭据或 File 提供者访问。其容器没有网络、根文件系统只读、已丢弃 capabilities、no-new-privileges、Chromium 沙箱、512 MiB 内存且无 swap、128 个 Linux tasks、一个 CPU 以及 128 MiB 临时存储。同一时间只运行一个浏览器请求；繁忙请求会收到可重试的 adapter-unavailable 错误。Chromium 会预热并复用，每个报表使用全新隔离的 BrowserContext，默认在 100 份报表后回收。将 `APPSTRUCT_RENDERER_RECYCLE_AFTER` 设为 1 到 1000 之间的值，以在启动成本和更紧的进程回收之间权衡。浏览器崩溃和渲染超时会强制立即回收。用额外的后端/渲染器对来扩展持续吞吐量。

模板随 App Spec 交付，并在渲染前对照生成的 artifact/version 匹配。应用 MiniJinja HTML 转义和 fuel 限制。输入不能提供 HTML 模板。请求包含已解析的 HTML 以及不可变的 run/template 绑定，不包含凭据或快照加密密钥。CSS 和 HTML 解析器拒绝外部 URL、导入和活动内容。浏览器另外强制限制性 CSP、禁用页面脚本并拦截请求。所有网络 URL 在解析前都会被拒绝，包括重定向和 DNS-rebinding 目标。

镜像打包 Noto Latin 和 CJK 字体。模板可以将 PNG/JPEG/WebP 图片和 WOFF/WOFF2/OTF/TTF 字体作为 data URL 嵌入。SVG、远程资源、自定义 CSS 属性和不受支持的原始 CSS 不在第一版范围内。HTML digest 也会绑定嵌入资源的字节。

限制为 1 MiB 快照、2 MiB 已解析 HTML（含内联资源）、100 页、标准 A3/A4/Letter/Legal 尺寸以及 30 秒渲染。PDF 输出受 50 MiB 和应用 File 限制中较低者约束。worker 用 PDF 解析器校验协议身份、digest、长度和页数。切换渲染器时，现有 Capture 模板应升级为 HTML 并给定新的模板版本。

Job 租约会在执行期间续期。取消排队或正在渲染的报表会关闭适配器连接并阻止发布。租约丢失会丢弃渲染器工作，而不会覆盖新 worker 的状态。发布会锁定 run 和 job，检查所有权，并在同一事务中提交 File 元数据和报表结果。与发布竞争的取消要么在该锁之前胜出，要么在发布成功后返回冲突。崩溃产生的未引用对象会在重试或报表保留清理时回收。

被阻止的资源、无效产物/输出、耗尽的预算和无效快照会失败且不重试。适配器不可用、崩溃和超时可以在配置的 Jobs 预算内重试。错误响应包含稳定代码；适配器从不记录报表 HTML 和快照。升级期间移除或替换已编译模板版本之前，请排空排队中的 runs。

验证：

```sh
pnpm --dir crates/appstruct-codegen/templates/report-renderer test
bash scripts/run-chromium-report-e2e.sh
bash scripts/run-renderer-isolation.sh
```

数据库脚本需要专用的 `APPSTRUCT_E2E_DATABASE_URL`；它会重置该测试数据库。隔离脚本构建并测试部署中使用的同一镜像和 seccomp 设置，包括网络拒绝、文件系统权限、内存耗尽和 PID 耗尽。
