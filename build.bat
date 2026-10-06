@echo off
REM ============================================================
REM  build.bat  —  Full production build of Greedy Bee v2 (Tauri)
REM  Prerequisites:
REM    - Rust toolchain  : https://rustup.rs
REM    - Tauri CLI       : cargo install tauri-cli
REM    - ffmpeg          : winget install ffmpeg
REM    - Node.js (npm)   : only needed if you later add a JS bundler
REM ============================================================

echo === Greedy Bee v2 (Tauri) — Production Build ===
echo.

REM Step 1: Process video (skipped automatically if already done)
echo [1/3] Processing character video...
call scripts\process_video.bat
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

REM Step 2: Generate tray icon (reuse the Python script from v1)
echo [2/3] Generating icons...
if exist "..\greedy-bee\create_icon.py" (
    python "..\greedy-bee\create_icon.py"
    if exist "..\greedy-bee\assets\tray_icon.ico" (
        copy /Y "..\greedy-bee\assets\tray_icon.ico" "src-tauri\icons\icon.ico" >nul
    )
) else (
    echo [WARN] create_icon.py not found — using placeholder icon if present.
)

REM Step 3: Tauri release build
echo [3/3] Building Tauri app...
cargo tauri build

if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] Build failed. Check errors above.
    exit /b 1
)

echo.
echo === Build complete! ===
echo Installer: src-tauri\target\release\bundle\nsis\Greedy Bee_1.0.0_x64-setup.exe
