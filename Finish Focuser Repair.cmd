@echo off
setlocal
rem Resume the existing enrollment. No /EMPTY, import, or update operation.
if not exist "C:\Program Files\Focuser\focuser-service.exe" (
 echo Focuser's installed service file is missing. Ordinary Repair cannot restore deleted files.
 echo Keep the retained authority data. Administrator recovery is required before using this repair launcher.
 echo Do not initialize an empty database to work around this error.
 pause
 exit /b 2
)
echo This completes repair of the retained Focuser installation and starts its service.
echo Your existing policy and original commitment expiry are preserved.
powershell.exe -NoProfile -Command "$ErrorActionPreference='Stop'; $installer='D:\Focuser\Focuser-Setup-CURRENT.exe'; if((Get-FileHash -LiteralPath $installer).Hash -cne 'F2F4CDF19F24B243304F920F6581A737AEA74057DDEC88C48916E0449EA8DA17'){throw 'Installer changed. Repair was not started.'}; $process=Start-Process -FilePath $installer -ArgumentList '/REPAIR' -Verb RunAs -PassThru -Wait; if($process.ExitCode -ne 0){throw ('Repair returned '+$process.ExitCode+'. Read the setup error; retained data must not be reset.')}"
if errorlevel 1 (
 echo Repair did not complete. Keep the retained data and recovery backup.
 pause
 exit /b 1
)
echo Repair completed.
pause
