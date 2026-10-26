param([Parameter(Mandatory=$true)][string]$GeneratedPath)
$ErrorActionPreference='Stop'
$audit=Join-Path $PSScriptRoot 'audit-windows-installer.ps1'
& $audit -Path $GeneratedPath
$text=Get-Content -LiteralPath $GeneratedPath -Raw
$scratch=Join-Path ([IO.Path]::GetTempPath()) ('Focuser-Installer-Audit-'+[guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
$cases=@(
    @{Name='unchecked-install-directory';Expected='Incomplete directory cleanup must fail';Text=$text.Replace('RMDir "$INSTDIR"'+[Environment]::NewLine+' IfErrors cleanup_failed','RMDir "$INSTDIR"').Replace("RMDir `"`$INSTDIR`"`n IfErrors cleanup_failed",'RMDir "$INSTDIR"')},
    @{Name='missing-complete-shortcut';Expected='Complete removal shortcut must use';Text=$text.Replace('"$INSTDIR\uninstall.exe" "/DELETE_DATA"','"$INSTDIR\uninstall.exe" "/FORCE"')},
    @{Name='missing-owner-discovery';Expected='Setup must use bundled owner discovery';Text=$text.Replace('Call DiscoverOwner','Call FreshInstallOnly')},
    @{Name='unchecked-repair-helper';Expected='Setup must use bundled owner discovery';Text=$text.Replace('"$RequestDir\request-client.exe" setup-repair','"$INSTDIR\focuser-service.exe" setup-repair')},
    @{Name='missing-removal-diagnostic';Expected='Removal denial must display the authority';Text=$text.Replace('Pop $RemovalMessage','Pop $9')},
    @{Name='unprotected-update-requester';Expected='Update requester must use newly created private';Text=$text.Replace('O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)','O:WDG:WDD:(A;;FA;;;WD)')},
    @{Name='update-fallthrough';Expected='Signed update request must exit';Text=$text.Replace('SetErrorLevel 29','SetErrorLevel 20')},
    @{Name='unexpanded';Expected='unexpanded template placeholders';Text=$text.Replace('Delete "$INSTDIR\focuser-cli.exe"','Delete "$INSTDIR{{this}}"')},
    @{Name='cleanup-before-authorization';Expected='cleanup precedes service authorization';Text=$text.Replace('Section Uninstall','Section Uninstall'+[Environment]::NewLine+'Delete "$INSTDIR\focuser-ui.exe"')},
    @{Name='missing-abort';Expected='must abort before installer cleanup';Text=$text.Replace('SetErrorLevel 26'+[Environment]::NewLine+'  Abort','SetErrorLevel 26').Replace("SetErrorLevel 26`n  Abort",'SetErrorLevel 26')},
    @{Name='ambiguous-migration';Expected='Installer missing: enrollment_invalid_ambiguous';Text=$text.Replace('enrollment_invalid_ambiguous','ignored-data-choice')},
    @{Name='per-user';Expected='Installer missing: RequestExecutionLevel admin';Text=$text.Replace('RequestExecutionLevel admin','RequestExecutionLevel user')},
    @{Name='broken-file-inheritance';Expected='file ACL inheritance must be restored';Text=$text.Replace('"$INSTDIR\*" /reset /T /Q','"$INSTDIR\*" /inheritance:r /Q')},
    @{Name='recursive-container-grants';Expected='file ACL inheritance must be restored';Text=$text.Replace('*S-1-5-32-545:(OI)(CI)RX /Q','*S-1-5-32-545:(OI)(CI)RX /T /Q')}
)
foreach($case in $cases){
    if($case.Text -eq $text){throw "Mutation did not apply: $($case.Name)"}
    $path=Join-Path $scratch ($case.Name+'.nsi')
    $case.Text | Set-Content -LiteralPath $path -Encoding utf8
    try {& $audit -Path $path;throw "Audit accepted mutation $($case.Name)"}
    catch {if($_.Exception.Message -notlike ('*'+$case.Expected+'*')){throw};Write-Output "Rejected installer regression: $($case.Name)"}
}

if($text.Contains('"/oname=WebView2Loader.dll"')){
    $missing=$text.Replace('"/oname=WebView2Loader.dll"','"/oname=wrong-loader-name.dll"')
    $path=Join-Path $scratch 'missing-loader.nsi'
    $missing | Set-Content -LiteralPath $path -Encoding utf8
    try {& $audit -Path $path -RequireWebViewLoader;throw 'Audit accepted missing GNU loader'}
    catch {if($_.Exception.Message -notlike '*Required WebView2Loader.dll resource*'){throw};Write-Output 'Rejected installer regression: missing GNU loader'}
}
Write-Output "Installer audit mutation tests passed; fixtures: $scratch"
