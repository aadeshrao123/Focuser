; Focuser strict installer. Handlebars contract pinned to tauri-cli 2.10.1.
; Intentionally replaces the entire upstream template. No stock reinstall,
; updater, per-user, taskkill, data cleanup or MSI migration path is included.
Unicode true
ManifestDPIAware true
RequestExecutionLevel admin
!include MUI2.nsh
!include LogicLib.nsh
!include FileFunc.nsh
!include TextFunc.nsh
!include nsDialogs.nsh
!if "{{install_mode}}" != "perMachine"
 !error "Focuser strict packaging requires perMachine"
!endif
!define PRODUCT "{{product_name}}"
!define VERSION "{{version}}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser"
!addplugindir "{{additional_plugins_path}}"
Name "${PRODUCT}"
OutFile "{{out_file}}"
InstallDir "$PROGRAMFILES64\Focuser"
SetCompressor /SOLID lzma
!if "{{uninstaller_sign_cmd}}" != ""
 !uninstfinalize '{{uninstaller_sign_cmd}}'
!endif
VIProductVersion "{{version_with_build}}"
VIAddVersionKey "ProductName" "{{product_name}}"
VIAddVersionKey "FileVersion" "{{version}}"
VIAddVersionKey "FileDescription" "Focuser per-machine setup"
VIAddVersionKey "LegalCopyright" "{{copyright}}"
!define MUI_ICON "{{installer_icon}}"
Var OwnerSID
Var ImportSource
Var EmptyChoice
Var OwnerField
Var SourceField
Var EmptyField
Var DeleteData
Var DeleteField
Var InstallMutex
Var RequestDir
Var RequestCreated
Var RequestResult
Var RemovalMessage
Var RepairChoice
Var SetupMessage
!insertmacro MUI_PAGE_WELCOME
Page custom EnrollmentPage EnrollmentLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
UninstPage custom un.RetentionPage un.RetentionLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function CreatePrivateRequester
 ; Never execute an installed path before checking it: an older installation
 ; could have writable binaries. Materialize only our bundled IPC requester in
 ; newly created, private BA/SYSTEM-owned objects. No policy/install files change.
 StrCpy $RequestCreated 0
 StrCpy $RequestResult 29
 StrCpy $1 0
 StrCpy $2 0
 System::Call 'kernel32::GetCurrentProcessId()i.r3'
 System::Call 'kernel32::GetTickCount()i.r4'
 StrCpy $RequestDir "$PROGRAMFILES64\Focuser.Request.$3.$4"
 System::Call 'advapi32::ConvertStringSecurityDescriptorToSecurityDescriptorW(w "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)",i 1,*p .r1,p 0)i.r0'
 ${If} $0 = 0
  Goto request_cleanup
 ${EndIf}
 ; NSIS runs as a 32-bit process: SECURITY_ATTRIBUTES is three 4-byte fields.
 System::Alloc 12
 Pop $2
 ${If} $2 = 0
  Goto request_cleanup
 ${EndIf}
 System::Call '*$2(i 12,p r1,i 0)'
 ; CREATE_NEW directory/file semantics refuse pre-existing paths/reparse points.
 System::Call 'kernel32::CreateDirectoryW(w "$RequestDir",p r2)i.r0'
 ${If} $0 = 0
  Goto request_cleanup
 ${EndIf}
 StrCpy $RequestCreated 1
 System::Call 'kernel32::LocalFree(p r1)'
 StrCpy $1 0
 System::Call 'advapi32::ConvertStringSecurityDescriptorToSecurityDescriptorW(w "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)",i 1,*p .r1,p 0)i.r0'
 ${If} $0 = 0
  Goto request_cleanup
 ${EndIf}
 System::Call '*$2(i 12,p r1,i 0)'
 System::Call 'kernel32::CreateFileW(w "$RequestDir\request-client.exe",i 0x40000000,i 0,p r2,i 1,i 0x80,p 0)p.r0'
 ${If} $0 = -1
  Goto request_cleanup
 ${EndIf}
 System::Call 'kernel32::CloseHandle(p r0)'
 System::Free $2
 StrCpy $2 0
 System::Call 'kernel32::LocalFree(p r1)'
 StrCpy $1 0
 SetOutPath "$RequestDir"
 ClearErrors
 {{#each binaries}}
 !if "{{this}}" == "focuser-service.exe"
 File "/oname=request-client.exe" "{{no-escape @key}}"
 !endif
 {{/each}}
 IfErrors request_cleanup
 Return
 request_cleanup:
 StrCpy $RequestResult 29
 Call DeletePrivateRequester
FunctionEnd
Function DeletePrivateRequester
 ${If} $2 != 0
  System::Free $2
  StrCpy $2 0
 ${EndIf}
 ${If} $1 != 0
  System::Call 'kernel32::LocalFree(p r1)'
  StrCpy $1 0
 ${EndIf}
 ${If} $RequestCreated = 1
  SetOutPath "$PROGRAMFILES64"
  ClearErrors
  Delete "$RequestDir\request-client.exe"
  RMDir "$RequestDir"
  ${If} ${Errors}
   StrCpy $RequestResult 29
  ${EndIf}
  StrCpy $RequestCreated 0
 ${EndIf}
FunctionEnd
Function RequestAuthorityUpdate
 Call CreatePrivateRequester
 ${If} $RequestCreated != 1
  SetErrorLevel 29
  Return
 ${EndIf}
 nsExec::ExecToLog /TIMEOUT=20000 '"$RequestDir\request-client.exe" update-request'
 Pop $0
 ${If} $0 == 0
  StrCpy $RequestResult 0
 ${EndIf}
 Call DeletePrivateRequester
 ${If} $RequestResult != 0
  DetailPrint "update_request_failed: The authority request or private requester cleanup failed. Installed policy and binaries were not replaced."
  SetErrorLevel 29
 ${Else}
  ; Success means the request was accepted. Query service status for completion.
  SetErrorLevel 0
 ${EndIf}
FunctionEnd

Function DiscoverOwner
 ${If} $OwnerSID != ""
  Return
 ${EndIf}
 Call CreatePrivateRequester
 ${If} $RequestCreated = 1
  nsExec::ExecToStack /TIMEOUT=15000 '"$RequestDir\request-client.exe" setup-owner'
  Pop $0
  Pop $SetupMessage
  ${If} $0 == 0
   ; Trim the helper's stdout newline; the service validates the actual SID.
   ${TrimNewLines} $OwnerSID $SetupMessage
  ${Else}
   DetailPrint "$SetupMessage"
  ${EndIf}
 ${EndIf}
 Call DeletePrivateRequester
FunctionEnd

Function RepairSetup
 Call CreatePrivateRequester
 ${If} $RequestCreated != 1
  SetErrorLevel 25
  Quit
 ${EndIf}
 nsExec::ExecToStack /TIMEOUT=120000 '"$RequestDir\request-client.exe" setup-repair'
 Pop $RequestResult
 Pop $SetupMessage
 Call DeletePrivateRequester
 DetailPrint "$SetupMessage"
 ${If} $RequestResult == 10
  ; Only an unprovisioned payload was archived. Continue explicit fresh setup.
  StrCpy $RepairChoice 0
  Return
 ${EndIf}
 ${If} $RequestResult == 0
  Call RegisterInstallation
  IfSilent +2
  MessageBox MB_OK|MB_ICONINFORMATION "Focuser repair completed. Existing commitments, history and binaries were preserved."
  SetErrorLevel 0
  Quit
 ${EndIf}
 IfSilent +2
 MessageBox MB_OK|MB_ICONSTOP "$SetupMessage"
 SetErrorLevel 25
 Quit
FunctionEnd

Function FreshInstallOnly
 ; Must precede every install section, WebView/runtime download, old-uninstaller
 ; invocation, process check, registry write and file extraction into INSTDIR.
 ReadRegStr $0 HKLM "${UNINSTKEY}" "UninstallString"
 ${If} $0 != ""
  Goto upgrade_denied
 ${EndIf}
 ReadRegStr $0 HKCU "${UNINSTKEY}" "UninstallString"
 ${If} $0 != ""
  Goto upgrade_denied
 ${EndIf}
 ${If} $OwnerSID != ""
  ReadRegStr $0 HKU "$OwnerSID\${UNINSTKEY}" "UninstallString"
  ${If} $0 != ""
   Goto upgrade_denied
  ${EndIf}
 ${EndIf}
 ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\FocuserAuthority" "ImagePath"
 ${If} $0 != ""
  Goto upgrade_denied
 ${EndIf}
 IfFileExists "$APPDATA\FocuserAuthority\*.*" upgrade_denied
 IfFileExists "$INSTDIR\*.*" upgrade_denied
 ; A previous repair may already have archived the unprovisioned payload.
 ; With no existing installation, resume ordinary explicit fresh enrollment.
 Return
 upgrade_denied:
 ${If} $RepairChoice != 1
  IfSilent repair_not_selected
  MessageBox MB_YESNO|MB_ICONQUESTION "Focuser already exists or setup was interrupted. Repair this installation? Repair preserves policy and never replaces installed application binaries. An incomplete payload without policy is archived before retrying setup." IDNO repair_not_selected
 ${EndIf}
 Call RepairSetup
 Return
 repair_not_selected:
 DetailPrint "upgrade_deferred: Direct installer replacement is unsupported. Use the service-owned update flow; older installations need a trusted bootstrap. No existing files were changed."
 IfSilent +2
 MessageBox MB_OK|MB_ICONSTOP "upgrade_deferred: Use the application or /REQUEST_UPDATE for supported authenticated updates. See service-phase3.md for older-installation bootstrap and repair."
 SetErrorLevel 22
 Quit
FunctionEnd
Function .onInit
 SetRegView 64
 SetShellVarContext all
 StrCpy $INSTDIR "$PROGRAMFILES64\Focuser"
 ; This mode stages a private requester only, never an installed replacement.
 ${GetOptions} $CMDLINE "/REQUEST_UPDATE" $0
 ${IfNot} ${Errors}
  Call RequestAuthorityUpdate
  Quit
 ${EndIf}
 System::Call 'kernel32::CreateMutexW(p 0,i 0,w "Global\Focuser.Installation")p.r0 ?e'
 Pop $1
 StrCpy $InstallMutex $0
 ${If} $0 = 0
 ${OrIf} $1 = 183
  SetErrorLevel 23
  Quit
 ${EndIf}
 ${GetOptions} $CMDLINE "/UPDATE" $0
 ${IfNot} ${Errors}
  DetailPrint "upgrade_deferred: An installer flag is not update authorization. Use /REQUEST_UPDATE or the application."
  IfSilent +2
  MessageBox MB_OK|MB_ICONSTOP "upgrade_deferred: Use /REQUEST_UPDATE or the application. This installer cannot replace installed files directly."
  SetErrorLevel 22
  Quit
 ${EndIf}
 StrCpy $OwnerSID ""
 StrCpy $ImportSource ""
 StrCpy $EmptyChoice 0
 ${GetOptions} $CMDLINE "/OWNER_SID=" $OwnerSID
 ClearErrors
 StrCpy $RepairChoice 0
 ${GetOptions} $CMDLINE "/REPAIR" $0
 ${IfNot} ${Errors}
  StrCpy $RepairChoice 1
 ${EndIf}
 Call FreshInstallOnly
 Call DiscoverOwner
 ClearErrors
 ${GetOptions} $CMDLINE "/IMPORT=" $ImportSource
 ClearErrors
 ${GetOptions} $CMDLINE "/EMPTY" $0
 ${IfNot} ${Errors}
  StrCpy $EmptyChoice 1
 ${EndIf}
 ${If} ${Silent}
  ${If} $ImportSource != ""
  ${AndIf} $EmptyChoice = 1
   DetailPrint "enrollment_invalid_ambiguous: Choose import OR empty initialization; neither was applied."
   SetErrorLevel 24
   Quit
  ${EndIf}
  ${If} $OwnerSID == ""
   SetErrorLevel 24
   Quit
  ${EndIf}
  ${If} $ImportSource == ""
  ${AndIf} $EmptyChoice != 1
   SetErrorLevel 24
   Quit
  ${EndIf}
 ${EndIf}
FunctionEnd
Function EnrollmentPage
 nsDialogs::Create 1018
 Pop $0
 ${NSD_CreateLabel} 0 0 100% 34u "The current desktop user is detected automatically, including when setup uses another administrator's credentials. Confirm the user below. Existing commitments must be explicitly imported."
 Pop $0
 ${NSD_CreateLabel} 0 38u 100% 12u "Detected user's SID (advanced: change only to enroll another user):"
 Pop $0
 ${NSD_CreateText} 0 52u 100% 12u "$OwnerSID"
 Pop $OwnerField
 ${NSD_CreateLabel} 0 70u 100% 12u "Existing profile database (absolute path), or choose a fresh policy:"
 Pop $0
 ${NSD_CreateText} 0 84u 100% 12u "$ImportSource"
 Pop $SourceField
 ${NSD_CreateCheckbox} 0 104u 100% 18u "Initialize a new, empty authority (no existing commitments to import)"
 Pop $EmptyField
 ${NSD_SetState} $EmptyField $EmptyChoice
 nsDialogs::Show
FunctionEnd
Function EnrollmentLeave
 ${NSD_GetText} $OwnerField $OwnerSID
 ${NSD_GetText} $SourceField $ImportSource
 ${NSD_GetState} $EmptyField $EmptyChoice
 ${If} $OwnerSID == ""
  MessageBox MB_OK "The desktop user could not be detected. Run setup from the intended user's desktop, or enter their SID from whoami /user."
  Abort
 ${EndIf}
 ${If} $ImportSource == ""
 ${AndIf} $EmptyChoice != 1
  MessageBox MB_OK "Select an existing database, or explicitly choose an empty authority."
  Abort
 ${EndIf}
 ${If} $ImportSource != ""
 ${AndIf} $EmptyChoice = 1
  MessageBox MB_OK "Choose import OR an empty authority."
  Abort
 ${EndIf}
FunctionEnd
Section Install
 StrCpy $INSTDIR "$PROGRAMFILES64\Focuser"
 Call FreshInstallOnly
 ; Runtime prerequisite check before files, storage or enforcement are created.
 ReadRegStr $0 HKLM "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
 ${If} $0 == ""
  ReadRegStr $0 HKLM "Software\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
 ${EndIf}
 ${If} $0 == ""
  ReadRegStr $0 HKU "$OwnerSID\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
 ${EndIf}
 ${If} $0 == ""
 ${OrIf} $0 == "0.0.0.0"
  DetailPrint "webview2_required: Install Microsoft Edge WebView2 Runtime before provisioning Focuser. No enforcement was installed."
  IfSilent +2
  MessageBox MB_OK|MB_ICONSTOP "Install Microsoft Edge WebView2 Runtime, then run this installer again. No Focuser enforcement was installed."
  SetErrorLevel 28
  Abort
 ${EndIf}
 SetOutPath "$INSTDIR"
 ClearErrors
 File "/oname={{main_binary_name}}.exe" "{{main_binary_path}}"
 {{#each binaries}}
 File "/oname={{this}}" "{{no-escape @key}}"
 {{/each}}
 {{#each resources_dirs}}
 CreateDirectory "$INSTDIR\\{{this}}"
 {{/each}}
 {{#each resources}}
 File /a "/oname={{this.[1]}}" "{{no-escape @key}}"
 {{/each}}
 IfErrors install_failed
 ; Protect the fixed per-machine directory before executing the helper.
 nsExec::ExecToLog /TIMEOUT=120000 '"$SYSDIR\icacls.exe" "$INSTDIR" /setowner *S-1-5-32-544 /T /Q'
 Pop $0
 ${If} $0 != 0
  Goto install_failed
 ${EndIf}
 ; Apply container inheritance flags to the directory only. Applying these
 ; grants recursively to files can remove every effective file ACE.
 nsExec::ExecToLog /TIMEOUT=120000 '"$SYSDIR\icacls.exe" "$INSTDIR" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F *S-1-5-32-544:(OI)(CI)F *S-1-5-32-545:(OI)(CI)RX /Q'
 Pop $0
 ${If} $0 != 0
  Goto install_failed
 ${EndIf}
 ; Children receive their effective permissions from the protected root.
 nsExec::ExecToLog /TIMEOUT=120000 '"$SYSDIR\icacls.exe" "$INSTDIR\*" /reset /T /Q'
 Pop $0
 ${If} $0 != 0
  Goto install_failed
 ${EndIf}
 ${If} $EmptyChoice = 1
  nsExec::ExecToStack /TIMEOUT=120000 '"$INSTDIR\focuser-service.exe" install --owner-sid "$OwnerSID" --initialize-empty'
 ${Else}
  nsExec::ExecToStack /TIMEOUT=120000 '"$INSTDIR\focuser-service.exe" install --owner-sid "$OwnerSID" --source "$ImportSource"'
 ${EndIf}
 Pop $0
 Pop $SetupMessage
 DetailPrint "$SetupMessage"
 ${If} $0 != 0
  Goto install_failed
 ${EndIf}
 Call RegisterInstallation
 Goto install_done
 install_failed:
 DetailPrint "provisioning_failed: Installation is incomplete. Data and existing commitments were preserved. Run this same installer again and choose Repair, or use /REPAIR."
 IfSilent +2
 MessageBox MB_OK|MB_ICONSTOP "Setup could not complete.$\r$\n$SetupMessage$\r$\nExisting policy was preserved. Run this installer again and choose Repair. See installation Details for earlier errors."
 SetErrorLevel 25
 Abort
 install_done:
SectionEnd

Function RegisterInstallation
 ClearErrors
 IfFileExists "$INSTDIR\uninstall.exe" registered_uninstaller
 WriteUninstaller "$INSTDIR\uninstall.exe"
 registered_uninstaller:
 WriteRegStr HKLM "${UNINSTKEY}" "DisplayName" "Focuser"
 WriteRegStr HKLM "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
 WriteRegStr HKLM "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
 WriteRegStr HKLM "${UNINSTKEY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
 WriteRegStr HKLM "${UNINSTKEY}" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
 WriteRegDWORD HKLM "${UNINSTKEY}" "NoModify" 1
 WriteRegDWORD HKLM "${UNINSTKEY}" "NoRepair" 1
 CreateDirectory "$SMPROGRAMS\Focuser"
 CreateShortcut "$SMPROGRAMS\Focuser\Focuser.lnk" "$INSTDIR\\{{main_binary_name}}.exe"
 CreateShortcut "$SMPROGRAMS\Focuser\Uninstall Focuser Completely.lnk" "$INSTDIR\uninstall.exe" "/DELETE_DATA"
 ${If} ${Errors}
  SetErrorLevel 25
  Abort "Setup registration failed. Existing policy was preserved; retry Repair."
 ${EndIf}
FunctionEnd

Function un.onInit
 SetRegView 64
 SetShellVarContext all
 System::Call 'kernel32::CreateMutexW(p 0,i 0,w "Global\Focuser.Installation")p.r0 ?e'
 Pop $1
 StrCpy $InstallMutex $0
 ${If} $0 = 0
 ${OrIf} $1 = 183
  SetErrorLevel 23
  Quit
 ${EndIf}
 ; No cleanup, process termination, updater exemption or data deletion here.
 StrCpy $DeleteData 0
 ${GetOptions} $CMDLINE "/DELETE_DATA" $0
 ${IfNot} ${Errors}
  StrCpy $DeleteData 1
 ${EndIf}
FunctionEnd
Function un.RetentionPage
 nsDialogs::Create 1018
 Pop $0
 ${NSD_CreateLabel} 0 0 100% 40u "Close Focuser first. The service must authorize removal. Complete removal is refused while any block or protected commitment remains, including temporary pauses and breaks. Sign in as the enrolled user so their agent can clean up."
 Pop $0
 ${NSD_CreateCheckbox} 0 50u 100% 38u "Complete removal: delete policy, budgets, history, update files and this user's Focuser data. Keep unchecked to retain data. Browser extensions and separate backups are preserved."
 Pop $DeleteField
 ${NSD_SetState} $DeleteField $DeleteData
 nsDialogs::Show
FunctionEnd
Function un.RetentionLeave
 ${NSD_GetState} $DeleteField $DeleteData
FunctionEnd
Section Uninstall
 ; AUTHORIZATION BARRIER. Identical for interactive, direct, /S and /UPDATE.
 ; The installed helper verifies the service endpoint, obtains and atomically
 ; consumes a caller/revision/retention-bound permit, drains the agent, stops
 ; SCM normally, unregisters only owned resources and applies data retention.
 ${If} $DeleteData = 1
  nsExec::ExecToStack /TIMEOUT=120000 '"$INSTDIR\focuser-service.exe" remove --delete-data'
 ${Else}
  nsExec::ExecToStack /TIMEOUT=120000 '"$INSTDIR\focuser-service.exe" remove'
 ${EndIf}
 Pop $0
 Pop $RemovalMessage
 ${If} $0 != 0
  DetailPrint "$RemovalMessage"
  DetailPrint "Removal denied or incomplete ($0). Enforcement resources were not authorized for installer cleanup. See ProgramData operation logs and administrator recovery documentation."
  IfSilent +2
  MessageBox MB_OK|MB_ICONSTOP "Removal was denied or could not complete.$\r$\n$RemovalMessage$\r$\nNo installer file cleanup was performed."
  SetErrorLevel 26
  Abort
 ${EndIf}
 ; No recursive deletion and no /REBOOTOK: a locked/missing cleanup operation
 ; must not be reported as a successful uninstall.
 ClearErrors
 Delete "$INSTDIR\\{{main_binary_name}}.exe"
 {{#each binaries}}
 Delete "$INSTDIR\\{{this}}"
 {{/each}}
 {{#each resources}}
 Delete "$INSTDIR\\{{this.[1]}}"
 {{/each}}
 {{#each resources_ancestors}}
 RMDir "$INSTDIR\\{{this}}"
 {{/each}}
 Delete "$INSTDIR\native-chromium.json"
 Delete "$INSTDIR\native-firefox.json"
 Delete "$SMPROGRAMS\Focuser\Focuser.lnk"
 Delete "$SMPROGRAMS\Focuser\Uninstall Focuser Completely.lnk"
 RMDir "$SMPROGRAMS\Focuser"
 IfErrors cleanup_failed
 ; Remove the registration only after all required binary cleanup succeeded.
 Delete "$INSTDIR\uninstall.exe"
 IfErrors cleanup_failed
 RMDir "$INSTDIR"
 IfErrors cleanup_failed
 DeleteRegKey HKLM "${UNINSTKEY}"
 IfErrors cleanup_failed
 SetErrorLevel 0
 Goto cleanup_done
 cleanup_failed:
 DetailPrint "cleanup_failed: Partial removal. Do not reinstall over retained state; use documented administrator recovery."
 SetErrorLevel 27
 Abort
 cleanup_done:
SectionEnd
