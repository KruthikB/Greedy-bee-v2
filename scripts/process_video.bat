@echo off
REM ============================================================
REM  process_video.bat
REM  One-time conversion: green-screen MP4  ->  transparent WebM
REM  Run this ONCE before building. Requires ffmpeg in PATH.
REM  Output: frontend\assets\character_transparent.webm
REM
REM  Source video search order:
REM    1. assets\character.mp4          (put it here for CI / clean repo)
REM    2. ..\greedy-bee\assets\character.mp4  (local dev side-by-side)
REM ============================================================

set OUTPUT=frontend\assets\character_transparent.webm

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

if exist "%OUTPUT%" (
    echo [INFO] %OUTPUT% already exists. Delete it to re-process.
    exit /b 0
)

echo [INFO] Converting green-screen video to transparent WebM...
echo        Source : %SOURCE%
echo        Output : %OUTPUT%
echo.

ffmpeg -i "%SOURCE%" ^
  -vf "chromakey=0x00b140:similarity=0.35:blend=0.15,format=yuva420p" ^
  -c:v libvpx-vp9 ^
  -b:v 0 ^
  -crf 30 ^
  -auto-alt-ref 0 ^
  -an ^
  "%OUTPUT%"

if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] ffmpeg failed. Make sure:
    echo   1. ffmpeg is installed  (winget install ffmpeg)
    echo   2. The background colour in the video matches 0x00b140
    echo      If not, update the chromakey= value in this script.
    exit /b 1
)

echo.
echo [OK] Done! Output: %OUTPUT%
echo      You can now run build.bat or dev.bat
