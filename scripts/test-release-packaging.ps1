$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$saved=@{}
foreach($name in @('FOCUSER_SIGN_CERT_SHA1','FOCUSER_SIGN_TIMESTAMP','FOCUSER_AUTHENTICODE_SHA256')){
    $saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process')
    [Environment]::SetEnvironmentVariable($name,$null,'Process')
}
try {
    try {& "$PSScriptRoot/build-windows-installer.ps1";throw 'Unsigned public release was accepted'}
    catch {if($_.Exception.Message -notlike 'Release signing is required.*'){throw}}
    try {& "$PSScriptRoot/sign-windows-file.ps1" -Path (Join-Path $repo 'Cargo.toml');throw 'Signing without credentials was accepted'}
    catch {if($_.Exception.Message -notlike 'A code-signing certificate store thumbprint is required*'){throw}}
    $env:FOCUSER_SIGN_CERT_SHA1='0000000000000000000000000000000000000000'
    $env:FOCUSER_AUTHENTICODE_SHA256='0'*64
    $env:FOCUSER_SIGN_TIMESTAMP='https://timestamp.invalid'
    try {& "$PSScriptRoot/build-windows-installer.ps1";throw 'Missing private certificate was accepted'}
    catch {if($_.Exception.Message -notlike 'Configured signing certificate/private key is unavailable*'){throw}}
    Write-Output 'Release gates passed: no unsigned public build and no signing without a certificate.'
} finally {
    foreach($name in $saved.Keys){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
}
