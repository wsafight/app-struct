# 安装

AppStruct 可以从源码检出安装。发布自动化会打包带校验和的 macOS/Linux/Windows 归档以及版本固定的安装器。技术预览阶段不假定已有公开发布；在标签及其资源发布之前，请使用源码安装。

本页用于安装 CLI 并创建第一个项目。应用已经存在时看 [升级](upgrading.zh-CN.md)；发布不可变产物时看 [部署](deployment.zh-CN.md)。

## 选择路径

1. **源码安装 CLI** — 技术预览阶段的默认方式。
2. **已发布安装器** — 仅在 GitHub Release 提供带校验和的归档之后使用。
3. **第一个应用** — `minimal` 连接你已有的 PostgreSQL；`dashboard` 和 `saas` 可通过 Docker Compose 托管数据库。

## 环境要求

| 依赖 | 所需版本 | 用途 |
| --- | --- | --- |
| Rust | 1.98.0，带 rustfmt 和 Clippy | CLI 和生成的后端 |
| PostgreSQL | 推荐 17 | 迁移和生成的 API |
| Node.js | 推荐 24 | 生成的 Web 应用 |
| pnpm | 11.25.0 | 按锁文件安装 Web 依赖 |
| Docker Compose | 当前版本，可选 | 仅用于 `database.dev.mode: managed` |

仓库根目录的 `rust-toolchain.toml` 以及每个生成的项目都会固定 Rust 1.98.0。这是当前实现基线所使用的本地最新 Rust 版本。

## 安装 CLI

从 AppStruct 仓库根目录执行：

```bash
rustup toolchain install 1.98.0 --component clippy,rustfmt
cargo build --release --locked -p appstruct-cli
./target/release/appstruct --version
```

从工作区根目录构建是刻意为之：这样 Cargo 会遵守已提交的根目录 `Cargo.lock`。在当前源码布局下，`cargo install --path crates/appstruct-cli --locked` 不会使用该工作区锁文件，因此不是可复现的安装路径。

可以将本次检出中的 `target/release` 加入 `PATH`，也可以把构建好的二进制安装到已有的用户目录（该目录需在 `PATH` 中）。例如在 macOS 或 Linux 上：

```bash
install -d "$HOME/.local/bin"
install -m 0755 target/release/appstruct "$HOME/.local/bin/appstruct"
```

确认已安装的二进制和所需工具可见：

```bash
appstruct --version
rustc --version
pnpm --version
```

将本次检出切换到另一个 AppStruct 修订后，请重新执行锁定的发布构建并替换已安装的二进制。

## 安装已发布版本

从选定的 GitHub Release 下载 `install.sh` 及其校验和，审阅脚本，然后用显式版本运行。脚本不会选择会变动的 `latest` 发布：

```bash
shasum -a 256 -c install.sh.sha256
bash install.sh --version <version>
```

它会选择原生 macOS 归档或静态 Linux musl 归档，校验归档的 SHA-256，并安装到 `$HOME/.local/bin`。使用 `--bin-dir` 指定其他目录，`--target` 选择已发布的 glibc 归档，或 `--archive-dir` 使用已本地下载的归档和校验和。不需要 root 权限，也不会修改 PATH。下载使用 HTTPS。安装只流式写入预期的二进制条目，并在校验成功后原子替换已有二进制。校验和或解压失败会保留已安装版本。校验和用于确认下载完整性；信任来源仍是发布账号和 HTTPS。

在 x64 Windows 上，从同一发布下载并审阅 `install.ps1` 及其校验和：

```powershell
$expected = (Get-Content install.ps1.sha256).Split()[0]
if ((Get-FileHash install.ps1 -Algorithm SHA256).Hash -ine $expected) { throw "checksum mismatch" }
./install.ps1 -Version <version>
```

Windows 默认目录是 `$env:LOCALAPPDATA\Programs\AppStruct`。`-BinDir` 和 `-ArchiveDir` 提供等价覆盖。通过操作系统的常规设置将该目录加入 PATH。两个安装器都支持用任意显式选定的发布替换版本，包括回滚。替换 CLI 不会回滚数据库迁移。

当发布提供平台归档时，将 `.tar.gz` 和匹配的 `.sha256` 文件下载到同一目录，校验后安装其中的二进制：

```bash
shasum -a 256 -c appstruct-<version>-<target>.tar.gz.sha256
tar -xzf appstruct-<version>-<target>.tar.gz
install -m 0755 appstruct-<version>-<target>/appstruct "$HOME/.local/bin/appstruct"
```

Linux 用户可以使用 `sha256sum -c`。校验和失败时不要安装归档。crates 发布后，`cargo install appstruct-cli --version <version> --locked` 是 registry 等价方式；源码与二进制发布版本保持同步。

发布标签会为 Linux x86-64/ARM64 提供 glibc 和静态 musl 归档，为 Apple Silicon 和 Intel macOS 提供原生归档，以及 x86-64 Windows `.zip`。在 Windows 上，从 PowerShell 校验并解压：

```powershell
$archive = "appstruct-<version>-x86_64-pc-windows-msvc.zip"
$expected = (Get-Content "$archive.sha256").Split()[0]
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw "checksum mismatch" }
Expand-Archive $archive -DestinationPath .
```

## 使用外部 PostgreSQL 开始

`minimal` 模板需要已有数据库：

```bash
appstruct new notes --template minimal
cd notes
cp .env.example .env
```

使用常规 PostgreSQL 管理流程创建数据库，然后编辑 `.env`，使 `DATABASE_URL` 包含正确的用户、密码、主机、端口和数据库。示例 URL 仅在本地开发中禁用 TLS。

校验环境并启动应用：

```bash
export DATABASE_URL=postgresql://user:password@127.0.0.1:5432/notes
appstruct migrate dev --accept
appstruct doctor
appstruct dev
```

外部模式默认使用 `database.dev.migration: unmanaged`，因此显式迁移命令会在首次启动前初始化数据库。开发服务器随后会生成并构建后端，使用已提交的 pnpm 锁安装 Web 依赖，并启动两个服务。必要时覆盖端口：

```bash
appstruct dev --api-port 3100 --web-port 5200
```

端口必须非零且互不相同。外部模式永远不会启动或停止 PostgreSQL。

## 使用托管 PostgreSQL 开始

`dashboard` 模板包含 `compose.yaml`，默认使用托管模式：

```bash
appstruct new project-hub --template dashboard
cd project-hub
appstruct doctor
appstruct dev
```

托管模式只启动 Compose 的 `postgres` 服务。在 Ctrl-C 时，仅当当前开发会话启动了该服务才会停止它，并保留命名的数据库卷。如果服务已经在运行，AppStruct 会让它继续运行。

托管模式默认使用 `database.dev.migration: prompt`。首次启动会显示安全计划并在创建或应用迁移前询问；之后当数据库已是最新时不再询问。使用 `auto` 进行受控自动化，`never` 只做只读兼容性强制，或 `unmanaged` 在启动时不让 AppStruct 检查迁移或 schema 状态。

仅在覆盖托管默认值时才将 `.env.example` 复制为 `.env`。进程环境变量优先于 `.env` 中的值；密钥永远不会写入生成产物或命令输出。

## 从 SaaS 预设开始

`saas` 模板同样使用托管 PostgreSQL，并锁定 `appstruct/saas@1`：

```bash
appstruct new saas-demo --template saas
cd saas-demo
appstruct preset show
appstruct doctor
appstruct dev
```

注册后，创建组织并使用生成的 Project 和 Task 资源。两者都是租户隔离且带审计的。开发默认在 `.appstruct/files` 下捕获邮件和本地文件；Jobs/Outbox 使用 PostgreSQL。用 `appstruct preset show --expanded` 检查生效的模块配置。

新注册使用 `member` 角色。第一个操作员注册后，从受信任主机恰好执行一次配置：

```bash
appstruct auth bootstrap-admin --email admin@example.com
```

生产环境中，通过已审阅的 App Spec 覆盖和运行时环境变量替换 capture/local 提供者。将生成的预设摘要和精确模块版本保留在 `appstruct.lock` 中。Admin 控制台包含只读的邮件/文件检查，以及 Jobs 重试/重放和 schedule 暂停/恢复/立即运行控件。Billing 不在预设版本 1 的范围内。

## 故障排除

导出捆绑的 Draft 2020-12 schema 用于编辑器集成，并在 CI 中使用结构化诊断或严格警告：

```bash
appstruct schema > appstruct.schema.json
appstruct doctor --format json
appstruct check --format json
appstruct check --deny-warnings
```

常见失败包括：

- `DATABASE_URL` 缺失或外部数据库不可达。
- 托管项目缺少 Docker 或 Compose。
- 已安装的 Rust 或 pnpm 版本与项目固定版本不同。
- API 或 Web 端口已被占用。
- 已应用的迁移校验和或实时 PostgreSQL schema 发生漂移。

运行 `appstruct migrate status` 查看迁移历史和漂移详情。AppStruct 不会自动修复校验和、脏历史或 catalog 漂移。

## 下一步

- [升级](upgrading.zh-CN.md)：同步升级 CLI、生成代码和数据库
- [部署](deployment.zh-CN.md)：构建不可变产物并显式执行 migrate apply
- [迁移检查](migration-lint.zh-CN.md)：接受 Schema 变更前的只读风险检查
