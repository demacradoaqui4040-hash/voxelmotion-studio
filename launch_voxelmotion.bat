@echo off
cd /d "%~dp0"
if exist "target\release\VoxelMotion.exe" (
    start "" "target\release\VoxelMotion.exe"
) else (
    echo Building VoxelMotion Studio for first launch...
    cargo build --release
    if exist "target\release\VoxelMotion.exe" start "" "target\release\VoxelMotion.exe"
)
