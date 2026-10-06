@echo off
REM ============================================================
REM  dev.bat  —  Start Greedy Bee v2 in development mode
REM  Hot-reloads the frontend on file changes.
REM  Prerequisites: Rust, cargo-tauri  (same as build.bat)
REM ============================================================

REM Make sure the transparent video exists (needed even in dev)
if not exist "frontend\assets\character_transparent.webm" (
    echo [INFO] Transparent video not found — running process_video.bat first...
    call scripts\process_video.bat
    if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%
)

cargo tauri dev
