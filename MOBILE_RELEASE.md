# Runory Mobile Release

Phase 6 的移动端与 Desktop 共用 React UI、Rust Domain Service、CredentialVault 接口、Host Verification、ServerSession 和 SSH Channel 架构。Desktop Vault 使用 Stronghold；移动端 Vault 使用 Argon2id + AES-256-GCM 认证加密，以保持 Android/iOS 构建链为纯 Rust。

## Android

要求：Android Studio/SDK、JDK 17、Android NDK 27.2（或经过 CI 验证的更新版本），以及 Rust Android targets。旧 NDK 21/22 缺少当前 Rust 工具链链接所需的 Android ARM64 运行库，不属于支持的发布环境。

```powershell
pnpm tauri android init --ci
pnpm tauri android build --apk
pnpm tauri android build --aab
```

正式 Play Store 发布前，在本机或 CI Secret Store 配置 Android signing keystore。Keystore、alias 和密码不得提交到仓库或写入 Runory 配置文件。上传 AAB 后完成 Data Safety、应用内容分级、截图和隐私政策。

Windows 上 Cargo registry 与生成的 Android 工程可能位于不同盘符；工程已关闭 Kotlin incremental compilation，避免 Kotlin 路径缓存无法跨盘归一化。构建缓存可以通过 `CARGO_TARGET_DIR` 放到空间充足的独立目录。

## iOS

iOS 工程生成、签名和归档必须在 macOS + Xcode 完成：

```bash
pnpm tauri ios init --ci
pnpm tauri ios build --export-method app-store-connect
```

`Info.ios.plist` 已包含 Face ID 用途说明。正式归档前设置 Apple Developer Team、唯一 Bundle Identifier、Distribution Certificate 与 App Store Connect Provisioning Profile，并在真机验证 Face ID/Touch ID、后台隐私遮罩、系统文件选择器、SSH、SFTP 和网络切换。

## Release Gate

- 不提交签名证书、Provisioning Profile、Android keystore 或密码
- Profile JSON 不包含密码、口令、私钥正文或 Vault Master Secret
- Unknown Host 仍要求显式确认，Changed Host Key 仍强制阻止
- 在真机验证后台恢复、生物识别取消、设备无生物识别、文件 Provider 授权失效和大文件传输
- 分别运行前端测试、Rust 测试、Android build；iOS build 必须在 macOS Runner 执行
