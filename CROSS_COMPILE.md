# libxml2 交叉编译指南

本项目提供了完整的libxml2交叉编译解决方案，支持Android和iOS平台。

## 快速开始

### 构建Android库

1. 确保已安装Android NDK
2. 设置环境变量：
   ```bash
   export ANDROID_NDK_ROOT="/path/to/android-ndk"
   ```
3. 构建所有Android架构：
   ```bash
   ./scripts/build_all_android.sh
   ```
4. 或者构建单个架构：
   ```bash
   ./scripts/build_libxml2.sh --platform android --arch arm64-v8a --ndk-path $ANDROID_NDK_ROOT
   ```

### 构建iOS库

1. 确保在macOS上运行
2. 安装Xcode命令行工具
3. 构建所有iOS架构：
   ```bash
   ./scripts/build_all_ios.sh
   ```
4. 或者构建单个架构：
   ```bash
   ./scripts/build_libxml2.sh --platform ios --arch arm64
   ```

## 使用预构建库

### Android

构建完成后，设置环境变量：
```bash
export LIBXML2_PREBUILT_PATH="./prebuilt/android-21"
```

然后正常构建你的Rust项目：
```bash
cargo build --target aarch64-linux-android
```

### iOS

构建完成后，设置环境变量：
```bash
export LIBXML2_IOS_PATH="./prebuilt/ios/universal"
```

然后构建你的Rust项目：
```bash
cargo build --target aarch64-apple-ios
```

## 支持的架构

### Android
- `arm64-v8a` - ARM64架构
- `armeabi-v7a` - ARM v7a架构
- `x86` - x86架构
- `x86_64` - x86_64架构

### iOS
- `arm64` - ARM64架构（设备）
- `x86_64` - x86_64架构（模拟器）

## 脚本说明

### `build_libxml2.sh`
主要的构建脚本，支持以下选项：
- `-p, --platform` - 目标平台（android|ios）
- `-a, --arch` - 目标架构
- `-n, --ndk-path` - Android NDK路径（Android构建必需）
- `-l, --api-level` - Android API级别（默认：21）
- `-s, --src-dir` - 源码目录（默认：./build/src）
- `-i, --install-dir` - 安装目录（默认：./build/install）

### `build_all_android.sh`
构建所有Android架构的便捷脚本。

### `build_all_ios.sh`
构建所有iOS架构的便捷脚本，并创建通用库。

## GitHub Actions

项目包含完整的CI/CD配置，在每次push时自动构建所有平台的库。

## 故障排除

### Android构建失败
1. 检查NDK路径是否正确
2. 确保NDK版本兼容（推荐r25c或更高）
3. 检查API级别设置

### iOS构建失败
1. 确保在macOS上运行
2. 检查Xcode命令行工具是否安装
3. 确保iOS SDK可用

### 通用问题
1. 检查网络连接（需要下载libxml2源码）
2. 确保有足够的磁盘空间
3. 检查构建工具是否安装（curl, tar, make等）

## 环境变量

### 构建时环境变量
- `ANDROID_NDK_ROOT` - Android NDK路径
- `API_LEVEL` - Android API级别

### 运行时环境变量
- `LIBXML2_PREBUILT_PATH` - Android预构建库路径
- `LIBXML2_IOS_PATH` - iOS预构建库路径
- `LIBXML2` - 指定特定的libxml2库文件

## 版本信息

- libxml2版本：2.10.3
- 支持的Android API级别：21+
- 支持的iOS版本：9.0+

## 许可证

与原项目相同的MIT许可证。
