# 文件与审计生命周期

## 文件引用与回收

File 为每个文件保存业务引用集合和最后解除引用的时间。上传后默认没有引用。业务代码在完成记录授权之后，在同一 `RequestContext` 中调用：

```rust
ctx.retain_file(&object_key, "app::Order", &order_id.to_string()).await?;
// 删除记录或替换附件时，使用相同的 entity 与 record_id：
ctx.release_file(&object_key, "app::Order", &order_id.to_string()).await?;
```

引用按照 `(entity, record_id)` 去重，最多 1000 个。最后一个引用解除后才开始保留期；重复解除不会延长保留期。模块检查租户，业务代码负责确认当前 actor 对该记录及文件的使用权。引用不是外键，记录删除时应用必须主动解除。

可信的维护任务可以持有 `FileState` 并调用 `collect_unreferenced(30, 100).await`，每批最多 100 个。该方法仅回收超过指定天数、没有显式业务引用的元数据文件，并排除仍被 Report 和 Activity 使用的文件。它没有开放 HTTP 清理入口，也不会默认启动定时删除。先在数据库副本上核对候选、备份及保留要求，再由应用注册 Jobs handler 或外部维护进程调度。

删除先原子标记 `deleting`，持有元数据行锁期间删除对象，最后删除元数据并提交事务；已标记的文件不能重新引用。存储失败保留元数据，后续维护可重试，批次中的其他对象仍继续处理。自定义 `FileProvider::delete` 必须对不存在的对象返回成功，因为并发维护和重试可能重复删除。显式 `delete` 同样拒绝带业务引用的文件。

数据库事务不能回滚对象存储操作。业务删除推荐在事务中解除引用，提交后交给维护任务回收；不要把 `delete_file` 当作可回滚的存储事务。上传后业务事务回滚可能留下没有元数据的对象，元数据扫描无法发现它们；需要存储侧清单与数据库对账，并在留出在途上传窗口后人工处理。文件直传、分片和病毒扫描不在这个接口中。

## 审计脱敏与留存

应用启动时读取并校验以下配置，配置非法会阻止启动：

```dotenv
APPSTRUCT_AUDIT_REDACT_FIELDS={"*":["/email"],"app::Order":["/payment/token","/items/0/secret"]}
APPSTRUCT_AUDIT_RETENTION_DAYS=90
```

键是 IR 中的实体 ID（例如 `app::Order`），`*` 应用于全部实体；值是 JSON Pointer 数组。支持嵌套路径、数组位置和 `~0` / `~1` 转义，不支持路径通配符。匹配字段在 before、after 和 metadata **写入前**替换为 `[REDACTED]`，其他字段保留。不存在的路径忽略，因此改字段名时需要同步调整配置。规则适用于后续写入，不会追溯清理既有快照。配置在进程中固定，修改后需重启。

审计继续使用业务事务，失败会一起回滚。留存删除是单独的可信维护操作：

```rust
let removed = appstruct_generated_backend::audit::prune_expired(&database, 1000).await?;
```

未配置留存时返回 0。配置留存也不会自动删除，必须主动调用；每批 1–10000 条，使用锁跳过其他维护任务已锁定的事件。删除前由应用负责导出和核验所需归档；这里没有内置归档服务。不要向普通用户暴露跨租户维护方法。审计读取权限继续由 reader roles 和租户作用域控制。
