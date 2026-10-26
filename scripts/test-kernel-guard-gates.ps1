$ErrorActionPreference='Stop'
$output=Join-Path $env:LOCALAPPDATA ('FocuserDevTools\guard-gate-test-'+[guid]::NewGuid().ToString('N'))
$builder=Join-Path $PSScriptRoot 'build-kernel-guard.ps1'
$rejected=$false
try{& $builder -AllocatedAltitude '0' -OutputDirectory $output -Production}
catch{if($_.Exception.Message -notmatch 'kernel_guard_not_certified'){throw};$rejected=$true}
if(-not $rejected -or (Test-Path -LiteralPath $output)){throw 'Production gate failed or touched output before refusing.'}
$rejected=$false
try{& $builder -AllocatedAltitude '0' -OutputDirectory $output -MSBuild (Join-Path $output 'missing-msbuild.exe')}
catch{if($_.Exception.Message -notmatch 'kernel_guard_toolchain_missing'){throw};$rejected=$true}
if(-not $rejected -or (Test-Path -LiteralPath $output)){throw 'Missing-toolchain gate failed or touched output before refusing.'}
Write-Output 'PASS: production and missing-toolchain gates reject before output or installation side effects.'
