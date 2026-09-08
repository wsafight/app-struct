# 生产报表渲染器适配器 RFC

> Status: preview implementation available; Linux isolation acceptance pending
> Date: 2026-09-05

## 结果

已实现的 v1 契约和验证状态见 [report-renderer.md](report-renderer.md)。该文档取代下方提议的传输和资源预算：v1 使用私有 Unix socket、2 MiB HTML/内联资源包以及 128-task 容器限制（Linux tasks 包括浏览器线程）。远程服务和独立资源包仍是后续工作。

`capture-v1` 仍是开发和简单文本报表的确定性默认渲染器。具备浏览器能力的生产渲染器必须运行在 API 进程之外，以及生成的 Job worker 信任边界之外。本 RFC 定义适配器在生产中被选用之前所需的安全和生命周期契约。

## 信任模型

第一版生产版本只渲染随应用源码交付并由生成产物 digest 锁定的模板。不接受租户上传的 HTML、JavaScript、字体或可执行模板。报表输入仍是当前 Report API 创建的加密、大小有界的 JSON 快照。

渲染器接收完全解析的文档请求。它没有数据库凭据、会话 cookie、云凭据、租户密钥或直接的 File 提供者写访问。Job worker 在校验响应后，通过现有的租户绑定 File 模块发布成功字节。

## 隔离

渲染器作为专用进程、容器或远程服务，以非 root 身份运行。其文件系统除每个 run 的全新临时目录外只读。Linux 部署使用 seccomp 配置、已丢弃的 capabilities、PID 限制和内存限制。浏览器沙箱是必需的，但不被当作外层安全边界。

默认在操作系统或网络策略层拒绝出站网络访问。渲染器拒绝 `file:`、超过字节预算的 `data:` 文档、loopback、link-local、private、multicast、Unix socket 以及云元数据目标。重定向和每个已解析地址都会再次检查，以防止 DNS rebinding。版本 1 没有远程资源允许列表；字体、样式和图片必须是包含在请求包中的应用产物。

## 适配器请求

每个请求绑定这些不可变值：

- ReportRun ID、若存在则包含租户 ID、模板名称/版本以及产物 digest；
- 渲染器协议版本和渲染器实现版本；
- locale、时区、纸张、方向以及解密后的 JSON 快照；
- 绝对截止时间和限定于此 run 的取消令牌；
- 每个捆绑 HTML、CSS、字体和图片产物的内容 digest。

传输经过认证并受完整性保护。本地进程可以使用带对等凭据的继承管道或 Unix socket。远程服务需要双向认证 TLS 和短时请求签名。请求和日志从不包含快照加密密钥。

## 资源预算

初始最大值是适配器配置的一部分，每个部署只能下调：

| 预算 | 最大值 |
| --- | ---: |
| JSON 快照 | 现有 Report `max_input_bytes`，最多 1 MiB |
| 已解析 HTML | 2 MiB |
| 捆绑资源 | 总计 20 MiB，每个 5 MiB |
| 渲染页数 | 100 |
| 页面尺寸 | 2,000 × 2,000 mm |
| 输出 PDF | 50 MiB |
| 墙钟时间 | 30 秒 |
| 浏览器进程 | 每个 run 16 个 |
| 内存 | 每个 run 512 MiB |
| 临时存储 | 每个 run 128 MiB |

Job 租约必须在渲染期间续期，并在硬截止时间加上发布余量之后仍然有效。丢失租约或收到取消会终止渲染器进程，删除其临时目录，并阻止发布。取消会在启动前、渲染期间、结果校验之后以及原子 File 发布紧前检查。

## 结果与失败

成功返回 PDF 字节或私有临时句柄、媒体类型、字节长度、页数和 SHA-256 digest。worker 校验所有声明值，并原子发布恰好一个对象。同一 ReportRun 的重试可以复用已校验的已发布对象，匹配当前幂等发布行为。

失败映射为有界稳定代码：invalid template artifact、blocked resource、render timeout、resource limit、browser crash、invalid output、cancelled 以及 adapter unavailable。详细浏览器错误保留在已脱敏的运维日志中，从不返回给租户。可重试性由代码决定，而不是解析消息。

## 运维与验收

指标包括队列延迟、渲染时长、取消延迟、重试次数、输出字节、资源限制终止以及适配器可用性，仅用有界的 template/version 标签标记。日志绑定 run 和租户 ID，但脱敏报表输入和渲染内容。

验收需要针对网络拒绝、DNS rebinding、重定向、`file:` 访问、元数据端点、过大资源/输出、内存和 PID 耗尽、超时、每个生命周期阶段的取消、租约丢失、崩溃清理、重试幂等以及租户安全发布的集成测试。在这些测试于部署所用的同一隔离环境中运行之前，不得启用任何生产适配器。
