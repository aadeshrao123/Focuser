param([Parameter(Mandatory=$true)][string]$ScratchRoot)
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath($ScratchRoot)
if(-not [IO.Path]::IsPathRooted($ScratchRoot) -or [IO.Path]::GetFileName($root) -notlike 'Focuser-FileGuard-*'){throw 'Only an explicit Focuser scratch fixture is supported.'}
foreach($path in @($root,(Join-Path $root 'installed'))){if((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse scratch roots are refused.'}}
$principal=[Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if(-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Actual administrator token required for this probe.'}
$image=Join-Path $root 'installed\focuser-ui.exe'
if([IO.File]::ReadAllText($image) -cne 'original payload'){throw 'Only the nonexecutable test payload may be probed.'}
$result=[ordered]@{elevated=$true;delete_denied=$false;rename_denied=$false;replace_denied=$false;unrelated_deletion_allowed=$false}
try{[IO.File]::Delete($image)}catch{$result.delete_denied=$true}
try{[IO.Directory]::Move((Join-Path $root 'installed'),(Join-Path $root 'renamed'))}catch{$result.rename_denied=$true}
try{[IO.File]::WriteAllText($image,'replacement')}catch{$result.replace_denied=$true}
$unrelated=Join-Path $root 'unrelated-updater.tmp'
if([IO.File]::ReadAllText($unrelated) -cne 'unrelated scratch'){throw 'Unexpected scratch fixture.'}
[IO.File]::Delete($unrelated)
$result.unrelated_deletion_allowed=-not [IO.File]::Exists($unrelated)
$result|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $root 'elevated-result.json') -Encoding UTF8
if(@($result.Values|Where-Object {$_ -ne $true}).Count){exit 1}
