#!/bin/bash

# 部署脚本 - 通过 ADB 将构建产物部署到目标设备

set -e

# 切换到项目根目录
cd "$(dirname "$0")/.."

# 默认配置
TARGET_PATH="/opt/simadmin"
DEPLOY_BACKEND=true
DEPLOY_FRONTEND=true
RESTART_SERVICE=true
BUILD_TARGET="${TARGET:-aarch64-unknown-linux-musl}"

# 解析命令行参数
for arg in "$@"; do
    case $arg in
        --backend-only)
            DEPLOY_FRONTEND=false
            ;;
        --frontend-only)
            DEPLOY_BACKEND=false
            ;;
        --no-restart)
            RESTART_SERVICE=false
            ;;
        --target=*)
            TARGET_PATH="${arg#*=}"
            ;;
        --build-target=*)
            BUILD_TARGET="${arg#*=}"
            ;;
        --help|-h)
            echo "用法: ./scripts/deploy.sh [选项]"
            echo ""
            echo "选项:"
            echo "  --backend-only   只部署后端"
            echo "  --frontend-only  只部署前端"
            echo "  --no-restart     不重启服务（默认会停止现有服务）"
            echo "  --target=PATH    指定目标路径 (默认: /opt/simadmin)"
            echo "  --build-target=TARGET  后端构建目标: aarch64 或 x86_64"
            echo "  --help, -h       显示帮助信息"
            echo ""
            echo "示例:"
            echo "  ./scripts/deploy.sh                      # 部署前端和后端"
            echo "  ./scripts/deploy.sh --backend-only       # 只部署后端"
            echo "  ./scripts/deploy.sh --frontend-only      # 只部署前端"
            echo "  ./scripts/deploy.sh --no-restart         # 部署但不停止服务"
            echo "  ./scripts/deploy.sh --target=/data/app   # 部署到指定路径"
            exit 0
            ;;
        *)
            # 兼容旧版本：第一个非选项参数作为 target path
            if [[ ! "$arg" =~ ^-- ]]; then
                TARGET_PATH="$arg"
            fi
            ;;
    esac
done

case "$BUILD_TARGET" in
    aarch64|arm64) BUILD_TARGET="aarch64-unknown-linux-musl" ;;
    x86_64|amd64) BUILD_TARGET="x86_64-unknown-linux-musl" ;;
    aarch64-unknown-linux-musl|x86_64-unknown-linux-musl) ;;
    *)
        echo "❌ 错误: 不支持的后端构建目标: $BUILD_TARGET" >&2
        exit 1
        ;;
esac

BACKEND_BIN="target/$BUILD_TARGET/release/simadmin"
FRONTEND_DIR="frontend/dist"

echo "🚀 通过 ADB 部署到 ${TARGET_PATH}"
echo ""

# 显示部署模式
if [ "$DEPLOY_BACKEND" = true ] && [ "$DEPLOY_FRONTEND" = true ]; then
    echo "📋 部署模式: 前端 + 后端"
elif [ "$DEPLOY_BACKEND" = true ]; then
    echo "📋 部署模式: 仅后端"
else
    echo "📋 部署模式: 仅前端"
fi
echo ""

# 检查 ADB
if ! command -v adb &> /dev/null; then
    echo "❌ 错误: 未找到 adb 命令"
    echo "请安装 Android SDK Platform Tools"
    exit 1
fi

# 检查 ADB 连接
if ! adb devices | grep -q "device$"; then
    echo "❌ 错误: 未检测到 ADB 设备"
    echo "请确保设备已连接并启用 ADB"
    exit 1
fi

# 检查后端文件
if [ "$DEPLOY_BACKEND" = true ]; then
    if [ ! -f "$BACKEND_BIN" ]; then
        echo "❌ 后端二进制不存在，请先运行 ./scripts/build.sh"
        exit 1
    fi
fi

# 检查前端文件
if [ "$DEPLOY_FRONTEND" = true ]; then
    if [ ! -d "$FRONTEND_DIR" ]; then
        echo "❌ 前端构建产物不存在，请先运行 ./scripts/build.sh --frontend-only"
        exit 1
    fi
fi

# 停止正在运行的服务（仅在部署后端时）
if [ "$DEPLOY_BACKEND" = true ] && [ "$RESTART_SERVICE" = true ]; then
    echo "⏹️  停止现有服务..."
    adb shell "killall simadmin 2>/dev/null || true"
fi

# 部署后端
if [ "$DEPLOY_BACKEND" = true ]; then
    echo "📦 部署后端..."
    adb push "$BACKEND_BIN" "${TARGET_PATH}/simadmin"
    adb shell "chmod +x ${TARGET_PATH}/simadmin"
    echo "✅ 后端部署完成"
fi

# 部署前端（先删除旧文件，避免文件损坏问题）
if [ "$DEPLOY_FRONTEND" = true ]; then
    echo "📦 部署前端..."
    adb shell "rm -rf ${TARGET_PATH}/www && mkdir -p ${TARGET_PATH}/www"
    adb push "$FRONTEND_DIR/." "${TARGET_PATH}/www/"
    echo "✅ 前端部署完成"
fi

echo ""
echo "✅ 部署完成！"
echo ""
echo "在设备上运行:"
echo "  adb shell"
echo "  cd ${TARGET_PATH} && ./simadmin"
echo ""
echo "或直接运行:"
echo "  adb shell 'cd ${TARGET_PATH} && ./simadmin'"
