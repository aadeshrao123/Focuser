param(
 [Parameter(Mandatory=$true)][string]$ReferenceDirectory,
 [Parameter(Mandatory=$true)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
if($PSVersionTable.PSEdition -ne 'Desktop'){throw 'Use Windows PowerShell with the Windows ConfigCI module. This script only creates an audit XML artifact.'}
$reference=[IO.Path]::GetFullPath($ReferenceDirectory)
$output=[IO.Path]::GetFullPath($OutputDirectory)
if(-not [IO.Directory]::Exists($reference) -or $reference -eq [IO.Path]::GetPathRoot($reference)){throw 'Supply a reviewed reference application directory, not a whole drive.'}
if($output -eq [IO.Path]::GetPathRoot($output)){throw 'A dedicated output directory is required.'}
if($output.StartsWith($env:WINDIR,[StringComparison]::OrdinalIgnoreCase) -or $output.StartsWith($env:ProgramFiles,[StringComparison]::OrdinalIgnoreCase) -or $output.StartsWith((Join-Path $env:ProgramData 'FocuserAuthority'),[StringComparison]::OrdinalIgnoreCase)){throw 'Audit artifacts must not target Windows, installed binaries or authoritative storage.'}
$policy=Join-Path $output 'Focuser-REFERENCE-AUDIT.xml'
if(Test-Path -LiteralPath $policy){throw 'An audit artifact already exists; no overwrite performed.'}
Import-Module ConfigCI -ErrorAction Stop
[IO.Directory]::CreateDirectory($output)|Out-Null
# Hash rules avoid trusting every executable signed by a broad publisher. This
# reference scan is NOT a complete boot/application compatibility policy.
New-CIPolicy -ScanPath $reference -FilePath $policy -Level Hash -UserPEs -ErrorAction Stop
Set-RuleOption -FilePath $policy -Option 3 # Enabled:Audit Mode; never enforcement.
[ordered]@{mode='audit-reference-only';source=$reference;policy_sha256=(Get-FileHash -LiteralPath $policy).Hash;complete_system_allowlist=$false;signed=$false;deployed=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $output 'audit-reference.json') -Encoding UTF8
Write-Output 'Audit reference XML created. It was not compiled, signed, deployed or enabled. Full OS compatibility review is still required.'
