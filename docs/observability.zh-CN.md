# 指标与数据库工作负载

每个生成的后端都在 `/metrics` 提供 Prometheus 文本。计数器和直方图是进程本地的，重启时重置。抓取每个副本；该端点不查询 PostgreSQL。把它保持在内部网络上。生产 Web 代理不会暴露它。

| 指标 | 含义 |
| --- | --- |
| `appstruct_health_ready` | 应用生命周期就绪；用 `/health/ready` 做数据库 ping |
| `appstruct_http_request_duration_seconds` | 到响应头的时间直方图 |
| `appstruct_http_in_flight` | 等待响应头的请求 |
| `appstruct_http_dropped_observations_total` | 达到标签限制后省略的观测 |
| `appstruct_job_duration_seconds` | 已领取 attempt 持续时间直方图，包含持久化 |
| `appstruct_jobs_in_flight` | 活动的 job attempts |
| `appstruct_job_retries_total` | attempt 编号大于一的已领取 attempts |

HTTP 标签是 `route`（Axum 匹配的模板或 `unmatched`）、`method`（七种标准方法或 `OTHER`）和 `status_class`（`1xx` 到 `5xx`）。健康探测和指标抓取被排除。查询字符串、记录 ID 和租户 ID 永远不是标签。注册表最多接受 512 组 HTTP 标签，并将路由长度限制为 256 字节。每个直方图发出十个有限桶加上 `+Inf`、sum 和 count。添加大量路由时关注 dropped 计数器。

Job kinds 折叠为 `mail`、`report`、`report_cleanup` 或 `custom`。结果是 `succeeded`、`failed`、`cancelled`、`lease_lost`、`database_error` 或 `interrupted`；这些是 attempt 结果，不是持久化的队列深度或最终任务状态。丢弃活动 attempt 会递减 gauge 并记录 `interrupted`。HTTP 持续时间不包括流式响应体，包括 SSE。

PromQL 示例：

```promql
histogram_quantile(0.95, sum by (le, route) (rate(appstruct_http_request_duration_seconds_bucket[5m])))
sum(rate(appstruct_http_request_duration_seconds_count{status_class="5xx"}[5m]))
sum(rate(appstruct_job_duration_seconds_count{outcome="lease_lost"}[5m]))
```

## PostgreSQL API 基准测试

使用名称包含 `test` 或 `e2e` 的一次性 PostgreSQL 数据库。运行器会**重置其 public schema**，生成租户/审计夹具，迁移它并启动本地后端。

```bash
APPSTRUCT_E2E_DATABASE_URL=postgresql://localhost/appstruct_benchmark_test \
  bash scripts/run-postgres-benchmark.sh
```

默认数据集每个租户 25,000 行，共两个租户。运行器检查字段访问和租户隔离，然后在 1、8 和 24 个并发客户端下测量偏移列表、游标列表、聚合计数、单条读取以及带审计的创建/更新/软删除旅程。每个阶段预热五次并运行 200 次操作。最后 15 秒的混合阶段在最高并发下组合读取、游标列表、计数和带审计的写入。CRUD 延迟针对完整的三请求旅程。响应会被校验，因此即使延迟很低，授权错误或不正确的行数也会失败。它还会验证变化的资源 ID 和未匹配 URL 不会创建指标标签集。

结果写入 `output/benchmarks/postgres-api.json`，包含 p50/p95/p99/max 延迟、吞吐量、错误率、运行时/主机详情和工作负载配置。同级 `.prom` 文件包含指标快照。PostgreSQL CI 会上传两者。错误会使运行失败。发布模式的 p95 预算是查询/读取阶段 500 ms、混合持续负载 750 ms，以及三请求带审计 CRUD 旅程 1,000 ms；调试模式默认值是这些值的两倍。

覆盖项：`APPSTRUCT_BENCH_ROWS`（每个租户，最多 1,000,000）、`APPSTRUCT_BENCH_ITERATIONS`（最多 10,000）、逗号分隔的 `APPSTRUCT_BENCH_CONCURRENCIES`（每个最多 64）、`APPSTRUCT_BENCH_MIXED_SECONDS`、`APPSTRUCT_BENCH_P95_MS`、`APPSTRUCT_BENCH_PROFILE`（默认 `release` 或 `debug`）以及 `APPSTRUCT_BENCH_OUTPUT`。单数的 `APPSTRUCT_BENCH_CONCURRENCY` 仍可作为单层简写接受。API/Web 端口使用现有的 `APPSTRUCT_E2E_API_PORT` 和 `APPSTRUCT_E2E_WEB_PORT` 覆盖。

这默认是针对发布后端的有界闭环回归工作负载，不是生产容量估计。在同一主机、PostgreSQL 版本、数据集和构建配置上比较结果。它不建模开环到达、连接池耗尽、生产存储或网络延迟。容量规划请使用能代表部署的流量。
