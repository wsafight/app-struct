# 远程模块注册表

远程模块需要先通过 CLI 显式安装，之后由编译器离线消费。本页说明 lockfile、摘要和信任钉扎；它不是公开的模块市场。

```bash
appstruct module install vendor/analytics@1.2.3 \
  --registry https://registry.example.com \
  --public-key BASE64_ED25519_PUBLIC_KEY
appstruct module list
appstruct module verify
appstruct module update vendor/analytics@1.2.4
appstruct module uninstall vendor/analytics
```

安装会校验信封的 SHA-256 与 Ed25519 签名，检查 AppStruct 与 Module API 兼容版本，验证每个清单/产物路径，并把包写入 `modules/.registry/`。提交的 `appstruct.modules.lock` 记录注册表、公钥、包与清单摘要、缓存路径以及兼容版本。常规 `check`、`generate`、`build` 和运行时启动从不访问注册表，并会拒绝缺失或被修改的缓存文件。

签名载荷包含模块清单和 base64 编码的产物。信封对精确解码后的载荷字节签名。远程模块仍是带空运行时启动器的静态产物；不支持动态加载第三方 Rust 库。

`verify` 离线重新检查锁定签名、兼容元数据、清单以及每个缓存产物。传入模块名时只校验该条目。`update` 必须指定目标版本，并复用已锁定的注册表与公钥，除非提供 `--registry` 或 `--public-key`。下载或签名检查失败时，会保留先前的锁条目。`uninstall` 先删除锁条目，再删除其未被引用的内容寻址缓存目录。
