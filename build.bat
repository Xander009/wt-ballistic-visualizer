@echo off
rem Double-click: builds the app. 1 = run it from ../data, 2 = Lite exe, 3 = Max exe, 4 = both (in dist\).
cd /d "%~dp0rust" || goto :fail
echo 1  Run the app (fastest, to try a change)
echo 2  Build the Lite exe (about 3 min)
echo 3  Build the Max exe (about 7 min)
echo 4  Build the Lite and Max exes
choice /c 1234 /n /m "Choose 1, 2, 3 or 4: "
set pick=%errorlevel%

if %pick%==1 (
  cargo run --release --bin wtv || goto :fail
  goto :end
)

rem an open exe can't be overwritten
tasklist | find /i "WT-Ballistic-Visualizer" >nul && (
  echo.
  echo Close WT-Ballistic-Visualizer first, then run this again.
  goto :end
)
cargo build --release || goto :fail
if not %pick%==3 (
  cargo run --release --bin bundle || goto :fail
)
if not %pick%==2 (
  cargo run --release --bin bundle -- --max || goto :fail
)
echo.
echo Done: the exes are in dist\
goto :end

:fail
echo.
echo FAILED, see the messages above.
:end
pause
