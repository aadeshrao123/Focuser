; Focuser NSIS Installer Hooks
; 1. Creates a scheduled task to run Focuser at logon with highest privileges (no UAC prompt)
; 2. Registers the native messaging host for Chrome, Edge, and Firefox
; 3. Starts the Focuser service in the background

!include "StrFunc.nsh"
${StrRep}

!macro NSIS_HOOK_POSTINSTALL
  ; Create a scheduled task that runs Focuser at logon with admin rights
  nsExec::ExecToLog 'schtasks /create /tn "Focuser" /tr "\"$INSTDIR\Focuser.exe\"" /sc onlogon /rl highest /f'

  ; ─── Register Native Messaging Host for browsers ──────────────────

  ; Build JSON-safe path (replace \ with /)
  ${StrRep} $1 "$INSTDIR\focuser-native.exe" "\" "/"

  ; Write Chrome/Edge native messaging manifest
  CreateDirectory "$LOCALAPPDATA\Focuser\native-messaging"
  FileOpen $0 "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.json" w
  FileWrite $0 '{"name":"com.focuser.native","description":"Focuser Native Messaging Host","path":"$1","type":"stdio","allowed_origins":["chrome-extension://kdkmjbjegdcjdlbciifigfkifookbppg/"]}'
  FileClose $0

  ; Write Firefox native messaging manifest
  FileOpen $0 "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.firefox.json" w
  FileWrite $0 '{"name":"com.focuser.native","description":"Focuser Native Messaging Host","path":"$1","type":"stdio","allowed_extensions":["focuser@focuser-app"]}'
  FileClose $0

  ; Chrome registry
  WriteRegStr HKCU "Software\Google\Chrome\NativeMessagingHosts\com.focuser.native" "" "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.json"

  ; Edge registry
  WriteRegStr HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native" "" "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.json"

  ; Brave registry (uses Chrome path)
  WriteRegStr HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.focuser.native" "" "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.json"

  ; Firefox registry
  WriteRegStr HKCU "Software\Mozilla\NativeMessagingHosts\com.focuser.native" "" "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.firefox.json"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove the scheduled task on uninstall
  nsExec::ExecToLog 'schtasks /delete /tn "Focuser" /f'

  ; Remove native messaging registry keys
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.focuser.native"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.focuser.native"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\com.focuser.native"

  ; Remove native messaging manifests
  Delete "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.json"
  Delete "$LOCALAPPDATA\Focuser\native-messaging\com.focuser.native.firefox.json"
  RMDir "$LOCALAPPDATA\Focuser\native-messaging"
!macroend
