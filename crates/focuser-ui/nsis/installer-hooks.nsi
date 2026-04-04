; Focuser NSIS Installer Hooks
; Creates a scheduled task to run Focuser at logon with highest privileges (no UAC prompt)

!macro CUSTOM_INSTALL
  ; Create a scheduled task that runs Focuser at logon with admin rights
  nsExec::ExecToLog 'schtasks /create /tn "Focuser" /tr "\"$INSTDIR\Focuser.exe\"" /sc onlogon /rl highest /f'
!macroend

!macro CUSTOM_UNINSTALL
  ; Remove the scheduled task on uninstall
  nsExec::ExecToLog 'schtasks /delete /tn "Focuser" /f'
!macroend
