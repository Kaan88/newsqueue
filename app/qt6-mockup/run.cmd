@echo off
setlocal
set "preview=%~dp0..\target\qt6-mockup\preview\newsqueue-qt-compact.exe"
if exist "%preview%" (
    start "" "%preview%"
) else (
    echo Build the Qt mockup first with build.ps1 in this folder.
    pause
    exit /b 1
)
