param([Parameter(Mandatory=$true)][string]$Path,[switch]$Template,[switch]$RequireWebViewLoader)
$ErrorActionPreference='Stop'
$scriptText=(Get-Content -Raw -LiteralPath $Path) -replace '(?m)^\s*;.*$',''
$uninstall=$scriptText.Substring($scriptText.IndexOf('Section Uninstall'))
$barrier=$uninstall.IndexOf('focuser-service.exe" remove')
$firstDelete=$uninstall.IndexOf('Delete "')
if($barrier -lt 0 -or $firstDelete -lt $barrier){throw 'Uninstall cleanup precedes service authorization'}
if($uninstall.IndexOf('SetErrorLevel 26') -gt $firstDelete){throw 'Denial must abort before cleanup'}
$beforeDelete=$uninstall.Substring(0,$firstDelete)
if($beforeDelete -notmatch '(?s)\$\{If\}\s+\$0 != 0.*?SetErrorLevel 26\s+Abort\s+\$\{EndIf\}') {throw 'Nonzero helper results must abort before installer cleanup'}
if(-not $scriptText.Contains('CreateShortcut "$SMPROGRAMS\Focuser\Uninstall Focuser Completely.lnk" "$INSTDIR\uninstall.exe" "/DELETE_DATA"') -or -not $uninstall.Contains('Delete "$SMPROGRAMS\Focuser\Uninstall Focuser Completely.lnk"')){throw 'Complete removal shortcut must use the protected uninstaller and be cleaned'}
if($uninstall -notmatch 'RMDir "\$INSTDIR"\s+IfErrors cleanup_failed\s+DeleteRegKey HKLM'){throw 'Incomplete directory cleanup must fail before removing uninstall registration'}
if($beforeDelete -notmatch '(?s)nsExec::ExecToStack /TIMEOUT=120000.*?focuser-service.exe" remove.*?Pop \$0\s+Pop \$RemovalMessage' -or -not $beforeDelete.Contains('DetailPrint "$RemovalMessage"') -or -not $beforeDelete.Contains('MessageBox MB_OK|MB_ICONSTOP "Removal was denied or could not complete.$\r$\n$RemovalMessage')) {throw 'Removal denial must display the authority commitment/release diagnostic before cleanup'}
foreach($required in @('RequestExecutionLevel admin','Call FreshInstallOnly','upgrade_deferred','--delete-data','SetErrorLevel 27','enrollment_invalid_ambiguous')) {
    if(-not $scriptText.Contains($required)){throw "Installer missing: $required"}
}
foreach($forbidden in @('CheckIfAppIsRunning','taskkill','RMDir /r','/REBOOTOK','Var UpdateMode','MULTIUSER_INIT','NSIS_HOOK_PREUNINSTALL')){
    if($scriptText.Contains($forbidden)){throw "Unsafe stock-installer path remains: $forbidden"}
}
$init=$scriptText.Substring($scriptText.IndexOf('Function .onInit'))
if($init.IndexOf('Call FreshInstallOnly') -gt $init.IndexOf('FunctionEnd')){throw 'No early upgrade barrier'}
$updateRequest=$init.IndexOf('Call RequestAuthorityUpdate')
if($updateRequest -lt 0 -or $updateRequest -gt $init.IndexOf('Call FreshInstallOnly') -or $init -notmatch '(?s)/REQUEST_UPDATE.*?Call RequestAuthorityUpdate\s+Quit' -or -not $scriptText.Contains('SetErrorLevel 29')){throw 'Signed update request must exit before installer extraction'}
$requester=$scriptText.Substring($scriptText.IndexOf('Function CreatePrivateRequester'),$scriptText.IndexOf('Function FreshInstallOnly')-$scriptText.IndexOf('Function CreatePrivateRequester'))
foreach($required in @('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)','O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)','CreateDirectoryW','CreateFileW','File "/oname=request-client.exe"','"$RequestDir\request-client.exe" update-request')){if(-not $requester.Contains($required)){throw 'Update requester must use newly created private BA/SYSTEM objects'}}
if($scriptText.Contains('"$INSTDIR\focuser-service.exe" update-request') -or $requester.IndexOf('CreateFileW') -gt $requester.IndexOf('File "/oname=request-client.exe"')){throw 'Update requester must use newly created private BA/SYSTEM objects'}
foreach($required in @('Call DiscoverOwner','"$RequestDir\request-client.exe" setup-owner','"$RequestDir\request-client.exe" setup-repair','Call RepairSetup','/REPAIR','Call RegisterInstallation')){
    if(-not $scriptText.Contains($required)){throw 'Setup must use bundled owner discovery and conservative repair before extraction'}
}
if($scriptText.Contains('"$INSTDIR\focuser-service.exe" setup-repair')){throw 'Setup repair must use the private bundled helper, never an unchecked installed helper'}
$rootGrant='"$INSTDIR" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F *S-1-5-32-544:(OI)(CI)F *S-1-5-32-545:(OI)(CI)RX /Q'
$childReset='"$INSTDIR\*" /reset /T /Q'
$installHelper=$scriptText.IndexOf('focuser-service.exe" install')
if(-not $scriptText.Contains($rootGrant) -or -not $scriptText.Contains($childReset) -or $scriptText.IndexOf($rootGrant) -gt $scriptText.IndexOf($childReset) -or $scriptText.IndexOf($childReset) -gt $installHelper){throw 'Installer file ACL inheritance must be restored before provisioning'}
if($scriptText -match '/grant:r[^\r\n]* /T'){throw 'Recursive container-only grants can leave empty file ACLs'}

if(-not $Template) {
    if($scriptText.Contains('{{') -or $scriptText.Contains('}}')){throw 'Rendered installer contains unexpanded template placeholders'}
    foreach($binary in @('focuser-service','focuser-session-agent','focuser-cli','focuser-native','focuser-ui')) {
        if(-not $scriptText.Contains('File "/oname='+$binary+'.exe"')){throw "Installer does not package $binary"}
        if(-not $uninstall.Contains('Delete "$INSTDIR\'+$binary+'.exe"')){throw "Installer does not clean $binary"}
    }
}
if($RequireWebViewLoader){
    if(-not $scriptText.Contains('"/oname=WebView2Loader.dll"') -or -not $uninstall.Contains('Delete "$INSTDIR\WebView2Loader.dll"')) {throw 'Required WebView2Loader.dll resource is not packaged and cleaned'}
}

Write-Output "Installer authorization/order audit passed: $Path"
