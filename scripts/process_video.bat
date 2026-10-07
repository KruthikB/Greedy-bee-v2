@echo off
REM ============================================================
REM  process_video.bat
REM  Build character assets from green-screen video packs
REM  Output: frontend\assets\characters\... + catalog.json
REM ============================================================

set OUT=frontend\assets\characters
set MANIFEST=%OUT%\catalog.json

if exist "%MANIFEST%" (
    echo [INFO] %MANIFEST% already exists. Delete frontend\assets\characters to re-process.
    exit /b 0
)

echo [INFO] Building character assets from assets\characters\...
echo.

python -m pip install --quiet numpy pillow scipy opencv-python-headless
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Failed to install Python dependencies.
    exit /b 1
)

python scripts\build_character_assets.py --packs assets\characters --out %OUT%
if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] Character asset build failed.
    exit /b 1
)

echo.
echo [OK] Done! Output: %OUT%
echo      You can now run build.bat or dev.bat
