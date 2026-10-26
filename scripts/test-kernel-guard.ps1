param([string]$Compiler,[string]$OutputDirectory)
$ErrorActionPreference='Stop'
if(-not $Compiler){$Compiler=Join-Path $env:LOCALAPPDATA 'FocuserDevTools\ucrt64\bin\gcc.exe'}
if(-not $OutputDirectory){$OutputDirectory=Join-Path $env:LOCALAPPDATA 'FocuserDevTools\kernel-guard-tests'}
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output=[IO.Path]::GetFullPath($OutputDirectory)
if($output -eq [IO.Path]::GetPathRoot($output)){throw 'A dedicated output directory is required.'}
[IO.Directory]::CreateDirectory($output)|Out-Null
$exe=Join-Path $output 'guard-policy-tests.exe'
$compilerDirectory=Split-Path -Parent $Compiler
$previousPath=$env:PATH
try{
 $env:PATH="$compilerDirectory;$previousPath"
 & $Compiler -std=c11 -Wall -Wextra -Werror -pedantic '-O2' (Join-Path $root 'drivers\focuser-guard\shared\guard_policy.c') (Join-Path $root 'drivers\focuser-guard\tests\policy_test.c') -o $exe
 if($LASTEXITCODE -ne 0){throw 'Portable kernel policy compilation failed.'}
 & $exe; if($LASTEXITCODE -ne 0){throw 'Kernel policy tests failed.'}
}
finally{$env:PATH=$previousPath}
