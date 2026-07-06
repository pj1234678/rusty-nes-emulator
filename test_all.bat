@echo off
set "LOG_FILE=nes_execution.log"

echo Processing started with a 3-second kill delay. Check %LOG_FILE% for details...

(
    echo ====================================================
    echo  NES ROM Processing Log - %DATE% %TIME%
    echo ====================================================
    
    for %%F in (*.nes) do (
        echo.
        echo [PROCESSING] %%F
        echo ----------------------------------------------------
        
        :: 'start /B' launches the app in the background so the script can keep running
        start "" /B nes_ui.exe "%%F"
        
        :: Waits for 3 seconds (4 pings = 3 seconds of delay)
        ping 127.0.0.1 -n 2 >nul
        
        :: Forcefully kills the emulator process
        taskkill /f /im nes_ui.exe >nul 2>&1
    )
    
    echo.
    echo ====================================================
    echo  Processing Complete
    echo ====================================================
) > "%LOG_FILE%" 2>&1

echo Done!
pause