# 开发体验性能

Compiler 微基准无法反映开发者等待脚手架、生成、安装 Web 依赖和编译生成后端的真实时间。
发布前，或修改构建、缓存、生成器和依赖行为后，在稳定机器上运行端到端基准：

```bash
bash scripts/run-developer-experience-benchmark.sh
```

基准会在仓库外创建一次性 `minimal` 项目，测量脚手架、冷/热生成、冷/热生产构建、项目缓存
和生成目录体积。退出时会删除测试项目，只把结果保留到
`output/benchmarks/developer-experience.json`。CLI 自身的构建时间不计入结果。

默认预算面向普通开发笔记本：脚手架 5 秒、冷生成 30 秒、热生成 3 秒、冷生产构建 15 分钟、
热构建 2 分钟、项目缓存 8 GiB、生成目录 1 GiB。可通过
`APPSTRUCT_DX_*_BUDGET_MS` 或 `APPSTRUCT_DX_*_BUDGET_MB` 覆盖；`APPSTRUCT_DX_CLI` 可指定
待测二进制，`APPSTRUCT_DX_OUTPUT` 可用于保留连续结果。

只有同一主机和工具链上的结果适合直接比较。此基准覆盖数据库启动前的构建路径；PostgreSQL
就绪和交互式 `appstruct dev` 旅程继续由部署与浏览器 E2E 验证。
