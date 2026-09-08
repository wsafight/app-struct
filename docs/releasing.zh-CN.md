# 发布 AppStruct

仓库可以构建 crates.io 包以及带校验和的 macOS/Linux/Windows 二进制。发布是维护者操作：凭据和最终仓库 URL 故意不存放在此处。

## 首次推送到 GitHub

源码检出不假定 GitHub 仓库 URL。推送前在本地克隆中显式配置 remote；永远不要把 token、密码或私钥提交到仓库：

```bash
git remote -v
git remote add origin https://github.com/<owner>/<repository>.git
git branch --show-current
git status --short --branch
git status --short --ignored
```

当已配置 GitHub SSH 密钥时，改用 SSH URL。首次推送前，检查将要发布的精确文件并运行仓库检查：

```bash
git diff --check
git ls-files -co --exclude-standard | rg '(^|/)(\.env($|\.)|node_modules|target|references|test-results|.*\.(pem|key|p12|crt|log|sqlite|db)$)' || true
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check advisories
scripts/verify-packages.sh
scripts/run-template-build.sh
```

包含占位符的 `.env.example` 文件是预期存在的。真实 `.env` 文件、凭据、证书、本地研究检出、依赖目录、构建输出和浏览器报告必须保持被忽略且未跟踪。任何生成或打包命令之后，再次审阅 `git status --short --branch`。

仅在工作树包含预期提交后推送：

```bash
git push -u origin main
```

对于默认分支不同的新仓库，将 `main` 替换为已审阅的分支名。首次推送不会发布 crates 或创建 release；使用下方流程发布包和二进制。

## 预检

1. 在 `[workspace.package]` 中只设置一次工作区版本；所有官方 crate 继承它。
2. 首次公开发布前，将规范的 `repository` URL 加入工作区包元数据。
3. 确认工作树干净，且发布说明描述兼容性和迁移风险。
4. 运行固定的质量和打包门禁：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/verify-packages.sh
```

打包脚本仅在校验尚未发布的同步 crate 集合时使用本地 crates.io patches。打包后的清单仍包含带兼容版本的常规 registry 依赖，而不是本地路径。advisory 数据库当前要求 `cargo-deny` 0.20.2 或更新版本。

## 发布 Crates

按依赖顺序发布，并在消费者发布前等待每个 crate 出现在 crates.io 索引中：

```text
appstruct-contracts
appstruct-ir, appstruct-module-sdk, and appstruct-runtime
appstruct-compiler
appstruct-migrate
appstruct-codegen
appstruct-cli
```

对每个包使用 `cargo publish -p <package> --locked`。二进制包名是 `appstruct-cli`，安装后的可执行文件是 `appstruct`。发布不由二进制发布 workflow 自动化，因此源码标签不能消费 crates.io 凭据。

## 发布二进制

创建并推送 `v<workspace-version>` 标签。`.github/workflows/release.yml` 首先运行依赖 advisory 检查、格式化、严格 Clippy、工作区测试、Node 24 和 25 上的生成 Web 构建，以及完整的 PostgreSQL E2E 矩阵。然后构建这些归档：

```text
x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu
x86_64-unknown-linux-musl
aarch64-unknown-linux-musl
aarch64-apple-darwin
x86_64-apple-darwin
x86_64-pc-windows-msvc
```

Linux 和 macOS 发布是 `.tar.gz` 归档；Windows 是包含 `appstruct.exe` 的 `.zip` 归档。每个归档还包含根 README，并有同级 `.sha256` 文件。workflow 会拒绝版本与 Cargo 元数据不匹配的标签，并且仅在所有质量、模板和 E2E 作业成功后上传产物。在宣布之前，检查已创建的 GitHub release 并测试一次全新安装。
