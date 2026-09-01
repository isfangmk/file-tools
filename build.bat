@echo off
REM 使用国内 npm 镜像构建 File Tools（Tauri）
cd /d "%~dp0"

where rustc >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Rust not found. Install: https://rustup.rs
  exit /b 1
)

where npm >nul 2>nul
if errorlevel 1 (
  echo [ERROR] npm not found.
  exit /b 1
)

echo [1/2] Installing npm dependencies...
call npm install --registry=https://registry.npmmirror.com

echo [2/2] Building...
call npm run build

echo.
echo Done. Bundle: src-tauri\target\release\bundle\
