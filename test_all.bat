@echo off
setlocal enabledelayedexpansion

for %%f in (*.nes) do (
    echo ========================================
    echo Testing: %%f
    echo ========================================
    start "" /b target\debug\nes_ui.exe "%%f"
    timeout /t 10 /nobreak >nul
    taskkill /f /im nes_ui.exe >nul 2>&1
)

echo ========================================
echo All tests complete.
echo ========================================
pause
