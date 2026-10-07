@echo off
REM ============================================================
REM  process_video.bat
REM  One-time conversion: green-screen MP4  ->  transparent WebP frames
REM  Run this ONCE before building. Requires ffmpeg in PATH.
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
echo        Output : %FRAMES%\frame_%%%%04d.webp
echo.

if not exist "%FRAMES%" mkdir "%FRAMES%"
del /q "%FRAMES%\frame_*.webp" 2>nul
del /q "%MANIFEST%" 2>nul

ffmpeg -y -i "%SOURCE%" ^
  -vf "chromakey=0x00b140:similarity=0.35:blend=0.15,format=rgba,scale=-1:300,fps=12" ^
  -c:v libwebp ^
  -lossless 0 ^
  -compression_level 4 ^
  -q:v 55 ^
  -an ^
  "%FRAMES%\frame_%%04d.webp"

if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] ffmpeg failed. Make sure:
    echo   1. ffmpeg is installed  (winget install ffmpeg)
    echo   2. The background colour in the video matches 0x00b140
    echo      If not, update the chromakey= value in this script.
    exit /b 1
)

powershell -NoProfile -Command ^
  "$c=(Get-ChildItem '%FRAMES%\frame_*.webp').Count; @{frameCount=$c;fps=12} | ConvertTo-Json | Set-Content -Encoding utf8 '%MANIFEST%'; Write-Host \"Generated $c character frames.\""

if %ERRORLEVEL% neq 0 exit /b 1

echo.
echo [OK] Done! Output: %FRAMES%
echo      You can now run build.bat or dev.bat
