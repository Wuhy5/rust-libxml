@echo off
REM 构建libxml2的Windows批处理脚本

set SCRIPT_DIR=%~dp0
set PROJECT_ROOT=%SCRIPT_DIR%..
set BUILD_DIR=%PROJECT_ROOT%\build
set PREBUILT_DIR=%PROJECT_ROOT%\prebuilt

REM 检查Python是否可用
python --version >nul 2>&1
if %errorlevel% neq 0 (
    echo Python is required but not found in PATH
    exit /b 1
)

REM 创建构建目录
if not exist "%BUILD_DIR%" mkdir "%BUILD_DIR%"
if not exist "%PREBUILT_DIR%" mkdir "%PREBUILT_DIR%"

REM 设置默认参数
set ANDROID_NDK_ROOT=%ANDROID_NDK_ROOT%
if "%ANDROID_NDK_ROOT%"=="" (
    echo ANDROID_NDK_ROOT environment variable is not set
    echo Please set it to your Android NDK installation path
    exit /b 1
)

echo Building libxml2 for Android architectures...

REM 构建各个Android架构
for %%a in (arm64-v8a armeabi-v7a x86 x86_64) do (
    echo Building for %%a...
    python "%SCRIPT_DIR%build_libxml2.py" ^
        --platform android ^
        --arch %%a ^
        --ndk-path "%ANDROID_NDK_ROOT%" ^
        --api-level 21 ^
        --src-dir "%BUILD_DIR%\src" ^
        --install-dir "%PREBUILT_DIR%\android-21"
    
    if %errorlevel% neq 0 (
        echo Failed to build for %%a
        exit /b 1
    )
)

echo All Android architectures built successfully!
echo Prebuilt libraries are in: %PREBUILT_DIR%

REM 设置环境变量供Rust使用
echo.
echo To use these libraries in your Rust project, set:
echo set LIBXML2_PREBUILT_PATH=%PREBUILT_DIR%\android-21
echo.
echo Or add to your .cargo\config.toml:
echo [env]
echo LIBXML2_PREBUILT_PATH = "%PREBUILT_DIR%\android-21"
