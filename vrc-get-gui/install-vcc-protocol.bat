@setlocal

@echo ALCOMD3 vcc: protocol installer
@echo this script will register the vcc: protocol with ALCOMD3
@echo the function of this script will be integrated into ALCOMD3 itself in the future
@echo but for now, you need to run this script manually 
@echo.
@echo how to use
@echo execute this script if you've installed ALCOMD3 to %LOCALAPPDATA%\Programs\ALCOMD3\ALCOMD3.exe.
@echo if you have changed the installation path, drag and drop ALCOMD3.exe to this script.

@echo do you actually want to continue? ctrl + c to cancel

@pause

@if "%~1"=="" (
  @set ALCOMD3_EXE=%LOCALAPPDATA%\Programs\ALCOMD3\ALCOMD3.exe
) else (
  @set ALCOMD3_EXE=%~1
)

@if not exist "%ALCOMD3_EXE%" (
  @echo error: ALCOMD3.exe not found at %ALCOMD3_EXE%
  @exit /b 1
)

@echo registering vcc: using ALCOMD3 path %ALCOMD3_EXE%

@reg add HKCU\Software\Classes\vcc                    /f /v "URL Protocol" /t REG_SZ /d ""                              > NUL
@reg add HKCU\Software\Classes\vcc\DefaultIcon        /f /v ""             /t REG_SZ /d "\"%ALCOMD3_EXE%\",0"             > NUL
@reg add HKCU\Software\Classes\vcc\shell\open\command /f /v ""             /t REG_SZ /d "\"%ALCOMD3_EXE%\" link \"%%1\""  > NUL

@echo registered vcc: for ALCOMD3 with %ALCOMD3_EXE%

@pause
