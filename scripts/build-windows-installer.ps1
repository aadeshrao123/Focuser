param(
    [ValidateSet('x86_64-pc-windows-msvc','x86_64-pc-windows-gnu')][string]$Target='x86_64-pc-windows-msvc',
    [switch]$DebugBuild,
    [switch]$HostBuild,
    [switch]$SkipFrontend,
    [switch]$AllowUnsignedRelease
)
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $repo
$profile=if($DebugBuild){'debug'}else{'release'}
$signed=(-not [string]::IsNullOrWhiteSpace($env:FOCUSER_SIGN_CERT_SHA1))
if(-not $DebugBuild -and -not $signed -and -not $AllowUnsignedRelease){throw 'Release signing is required. Configure FOCUSER_SIGN_CERT_SHA1, FOCUSER_SIGN_TIMESTAMP and FOCUSER_AUTHENTICODE_SHA256. For a local unsigned release candidate only, explicitly use -AllowUnsignedRelease.'}
if($signed -and ($env:FOCUSER_AUTHENTICODE_SHA256 -notmatch '^[a-fA-F0-9]{64}$' -or -not $env:FOCUSER_SIGN_TIMESTAMP)){throw 'Signing requires the publisher SHA-256 pin and timestamp URL'}
if($signed){
    if($env:FOCUSER_SIGN_CERT_SHA1 -notmatch '^[a-fA-F0-9]{40}$'){throw 'Invalid signing certificate store thumbprint'}
    $certificate=@(foreach($store in @('Cert:\CurrentUser\My','Cert:\LocalMachine\My')){
        Get-Item -LiteralPath (Join-Path $store $env:FOCUSER_SIGN_CERT_SHA1) -ErrorAction SilentlyContinue | Where-Object {$_.HasPrivateKey -and $_.NotAfter -gt [DateTime]::Now}
    })
    if(-not $certificate.Count){throw 'Configured signing certificate/private key is unavailable; no release build started'}
}
if($DebugBuild -and $AllowUnsignedRelease){throw 'AllowUnsignedRelease is only for release-mode candidates'}
if($signed -and $AllowUnsignedRelease){throw 'Choose signed packaging or an unsigned candidate, not both'}
if($signed -and $Target -like '*-gnu' -and -not $HostBuild){throw 'Signed GNU packages require HostBuild so the WebView2 loader is signed before bundling'}
if($AllowUnsignedRelease){Write-Warning 'UNSIGNED RELEASE CANDIDATE: not a signed public release. Production updates remain unavailable without publisher configuration.'}
$targetRoot=if($env:CARGO_TARGET_DIR){[IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)}else{Join-Path $repo 'target'}
$outputRoot=if($HostBuild){Join-Path $targetRoot $profile}else{Join-Path $targetRoot "$Target/$profile"}
$buildArgs=@('build','--locked','-p','focuser-service','-p','focuser-session-agent','-p','focuser-cli','-p','focuser-native')
if(-not $HostBuild){$buildArgs+=@('--target',$Target)}
if(-not $DebugBuild){$buildArgs+='--release'}
& cargo @buildArgs
if($LASTEXITCODE -ne 0){throw 'Sidecar build failed'}
New-Item -ItemType Directory -Path 'crates/focuser-ui/binaries' -Force | Out-Null
foreach($binary in @('focuser-service','focuser-session-agent','focuser-cli','focuser-native')){
    $destination="crates/focuser-ui/binaries/$binary-$Target.exe"
    Copy-Item -LiteralPath "$outputRoot/$binary.exe" -Destination $destination
    if($DebugBuild -and $Target -like '*-gnu'){
        & objcopy --strip-debug $destination
        if($LASTEXITCODE -ne 0){throw 'Debug sidecar strip failed'}
    }
    if($signed){& "$PSScriptRoot/sign-windows-file.ps1" -Path $destination}
}
Set-Location 'crates/focuser-ui'
$tauriArgs=@('--yes','@tauri-apps/cli@2.10.1','build')
if($HostBuild){$tauriArgs+='--no-bundle'}else{$tauriArgs+=@('--target',$Target,'--bundles','nsis')}
if($DebugBuild){$tauriArgs+='--debug'}
$override=Join-Path $outputRoot 'focuser-build.override.json'
$overrideData=@{}
if($SkipFrontend){$overrideData.build=@{beforeBuildCommand=''}}
if($signed){
    $signScript=Join-Path $repo 'scripts/sign-windows-file.ps1'
    $overrideData.bundle=@{windows=@{signCommand=@{cmd='powershell.exe';args=@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$signScript,'-Path','%1')}}}
}
$overrideData | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $override -Encoding utf8
$tauriArgs+=@('--config',$override)
& npx.cmd @tauriArgs
if($LASTEXITCODE -ne 0){throw 'Protected NSIS build failed'}
if($HostBuild){
    # Tauri's prebuilt Windows CLI defaults bundling to MSVC even when cargo's
    # host is GNU. Bundle with the actual ABI and a real target output directory.
    # Only the main EXE is needed here; external binaries are staged above.
    $hostTriple=((& rustc -vV | Select-String '^host: ').Line -replace '^host: ','').Trim()
    if($hostTriple -ne $Target){throw "HostBuild requires the actual Rust host $hostTriple"}
    $bundleRoot=Join-Path $targetRoot "$Target/$profile"
    if((Test-Path -LiteralPath $bundleRoot) -and (Get-Item -LiteralPath $bundleRoot).LinkType){throw 'Host bundle staging must be a real directory'}
    New-Item -ItemType Directory -Path $bundleRoot -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $outputRoot 'focuser-ui.exe') -Destination (Join-Path $bundleRoot 'focuser-ui.exe')
    if($Target -like '*-gnu'){
        $loader=Join-Path $outputRoot 'WebView2Loader.dll'
        if(-not (Test-Path -LiteralPath $loader)){throw 'GNU UI requires WebView2Loader.dll from the compiled Tauri output'}
        Copy-Item -LiteralPath $loader -Destination (Join-Path $bundleRoot 'WebView2Loader.dll')
        if($signed){& "$PSScriptRoot/sign-windows-file.ps1" -Path (Join-Path $bundleRoot 'WebView2Loader.dll')}
    }
    $bundleArgs=@('--yes','@tauri-apps/cli@2.10.1','bundle','--target',$Target,'--bundles','nsis','--config',$override)
    if($DebugBuild){$bundleArgs+='--debug'}
    & npx.cmd @bundleArgs
    if($LASTEXITCODE -ne 0){throw 'Protected host NSIS bundle failed'}
    $outputRoot=$bundleRoot
}
Set-Location $repo
$generated="$outputRoot/nsis/$Target/installer.nsi"
if(-not (Test-Path -LiteralPath $generated)){
    $candidates=@(Get-ChildItem -LiteralPath $outputRoot -Filter installer.nsi -Recurse | Where-Object { $_.FullName -notlike '*node_modules*' })
    if($candidates.Count -ne 1){throw 'Cannot uniquely locate rendered NSIS script for audit'}
    $generated=$candidates[0].FullName
}
if($Target -like '*-gnu') {& "$PSScriptRoot/audit-windows-installer.ps1" -Path $generated -RequireWebViewLoader}
else {& "$PSScriptRoot/audit-windows-installer.ps1" -Path $generated}
& "$PSScriptRoot/test-installer-audit.ps1" -GeneratedPath $generated
& "$PSScriptRoot/test-installer-permissions.ps1" -GeneratedPath $generated
$forbidden=@(Get-ChildItem "$outputRoot/bundle" -Recurse -File | Where-Object {$_.Extension -eq '.msi' -or $_.Name -like '*.nsis.zip*'})
if($forbidden.Count){throw 'Unprotected MSI/updater artifact found; do not release this directory'}
Get-ChildItem "$outputRoot/bundle/nsis/*.exe" | Select-Object FullName,Length
$artifacts=@(Get-ChildItem "$outputRoot/bundle/nsis/*.exe")
if($artifacts.Count -ne 1){throw 'Expected exactly one NSIS installer artifact'}
if($signed){
    $payloads=@((Join-Path $outputRoot 'focuser-ui.exe'))
    foreach($name in @('focuser-service','focuser-session-agent','focuser-cli','focuser-native')){$payloads+=Join-Path $repo "crates/focuser-ui/binaries/$name-$Target.exe"}
    if($Target -like '*-gnu'){$payloads+=Join-Path $outputRoot 'WebView2Loader.dll'}
    foreach($payload in $payloads){
        $signature=Get-AuthenticodeSignature -LiteralPath $payload
        if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $env:FOCUSER_SIGN_CERT_SHA1 -or -not $signature.TimeStamperCertificate){throw "Payload signing verification failed: $payload"}
    }
}
foreach($artifact in $artifacts){
    if($signed){
        $signature=Get-AuthenticodeSignature -LiteralPath $artifact.FullName
        if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $env:FOCUSER_SIGN_CERT_SHA1 -or -not $signature.TimeStamperCertificate){throw 'Installer signing verification failed'}
    }
}
$manifest=[ordered]@{profile=$profile;target=$Target;signed=$signed;unsigned_candidate=[bool]$AllowUnsignedRelease;generated_utc=[DateTime]::UtcNow.ToString('o');artifacts=@($artifacts | ForEach-Object {[ordered]@{name=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}})}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath "$outputRoot/bundle/nsis/build-manifest.json" -Encoding utf8
