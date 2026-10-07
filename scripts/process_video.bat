@echo off
REM ============================================================
REM  process_video.bat
REM  One-time conversion: green-screen MP4  ->  transparent WebP frames
REM  Run this ONCE before building. Requires Python + deps.
REM  Output: frontend\assets\frames\frame_XXXX.webp + manifest.json
REM
REM  Source video search order:
REM    1. assets\character.mp4          (put it here for CI / clean repo)
REM    2. ..\greedy-bee\assets\character.mp4  (local dev side-by-side)
REM ============================================================

set FRAMES=frontend\assets\frames
set MANIFEST=%FRAMES%\manifest.json

REM Find the source video
if exist "assets\character.mp4" (
    set SOURCE=assets\character.mp4
) else if exist "..\greedy-bee\assets\character.mp4" (
    set SOURCE=..\greedy-bee\assets\character.mp4
) else (
    echo [ERROR] character.mp4 not found.
    echo         Place it at:  greedy-bee-v2\assets\character.mp4
    echo         Or keep it at: greedy-bee\assets\character.mp4  (local dev)
    exit /b 1
)

if exist "%MANIFEST%" (
    echo [INFO] %MANIFEST% already exists. Delete frontend\assets\frames to re-process.
    exit /b 0
)

echo [INFO] Converting green-screen video to transparent WebP frames...
echo        Source : %SOURCE%
echo        Output : %FRAMES%
echo        Method : border flood-fill (keeps face / skin)
echo.

if not exist "%FRAMES%" mkdir "%FRAMES%"
del /q "%FRAMES%\frame_*.webp" 2>nul
del /q "%MANIFEST%" 2>nul

python -m pip install --quiet numpy pillow scipy opencv-python-headless
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Failed to install Python dependencies.
    exit /b 1
)

python scripts\extract_character_frames.py --input "%SOURCE%" --out-dir "%FRAMES%" --fps 12 --height 300
if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] Frame extraction failed.
    exit /b 1
)

echo.
echo [OK] Done! Output: %FRAMES%
echo      You can now run build.bat or dev.bat
