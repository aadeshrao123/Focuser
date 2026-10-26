param(
    [Parameter(Mandatory=$true)][string]$PayloadDirectory,
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [Parameter(Mandatory=$true)][string]$Version,
    [Parameter(Mandatory=$true)][uint64]$SecurityVersion,
    [Parameter(Mandatory=$true)][uint64]$SecurityFloor,
    [Parameter(Mandatory=$true)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$PublisherSha256,
    [Parameter(Mandatory=$true)][string]$SigningKeyPath
)
$ErrorActionPreference='Stop'
# Release tooling only. The live authority never accepts these paths or keys.
# Authenticode-sign every binary first; this script only signs the update manifest.
if($Version -notmatch '^\d+\.\d+\.\d+$' -or $SecurityFloor -gt $SecurityVersion -or $SecurityFloor -lt 1){throw 'Invalid release/security version'}
$payload=(Resolve-Path -LiteralPath $PayloadDirectory).Path
$key=(Resolve-Path -LiteralPath $SigningKeyPath).Path
$files=@();$total=[uint64]0
$names=@('focuser-ui.exe','focuser-service.exe','focuser-session-agent.exe','focuser-cli.exe','focuser-native.exe')
if(Test-Path -LiteralPath (Join-Path $payload 'WebView2Loader.dll')){$names+='WebView2Loader.dll'}
foreach($name in $names){
    $path=Join-Path $payload $name
    $item=Get-Item -LiteralPath $path
    if($item.Attributes -band [IO.FileAttributes]::ReparsePoint -or $item.Length -le 0 -or $item.Length -gt 192MB){throw "Invalid payload: $name"}
    $signature=Get-AuthenticodeSignature -LiteralPath $path
    if($signature.Status -ne 'Valid' -or -not $signature.SignerCertificate){throw "Authenticode signing required before packaging: $name"}
    $certHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($signature.SignerCertificate.RawData)).ToLowerInvariant()
    if($certHash -ne $PublisherSha256.ToLowerInvariant()){throw "Wrong Windows signing identity: $name"}
    $total+=$item.Length
    $files+=[ordered]@{name=$name;url="https://github.com/aadeshrao123/Focuser/releases/download/v$Version/$name";bytes=$item.Length;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
}
if($total -gt 512MB){throw 'Update exceeds authority staging quota'}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$output=(Resolve-Path -LiteralPath $OutputDirectory).Path
$manifest=[ordered]@{product='com.focuser.authority.windows-x64';version=$Version;security_version=$SecurityVersion;security_floor=$SecurityFloor;protocol_min=1;protocol_max=1;schema_read_min=8;schema_read_max=8;schema_write=8;expires_at=[DateTimeOffset]::UtcNow.AddDays(2).ToUnixTimeSeconds();publisher_sha256=$PublisherSha256.ToLowerInvariant();files=$files}
$manifestPath=Join-Path $output 'windows-authority-manifest.json'
[IO.File]::WriteAllText($manifestPath,($manifest | ConvertTo-Json -Depth 10 -Compress),[Text.UTF8Encoding]::new($false))
& npx.cmd --yes '@tauri-apps/cli@2.10.1' signer sign --private-key-path $key $manifestPath
if($LASTEXITCODE -ne 0){throw 'Tauri/Minisign manifest signing failed'}
$envelope=[ordered]@{manifest=[Convert]::ToBase64String([IO.File]::ReadAllBytes($manifestPath));signature=([IO.File]::ReadAllText($manifestPath+'.sig')).Trim()}
$feed=Join-Path $output 'windows-authority-update.json'
[IO.File]::WriteAllText($feed,($envelope | ConvertTo-Json -Compress),[Text.UTF8Encoding]::new($false))
foreach($name in $names){Copy-Item -LiteralPath (Join-Path $payload $name) -Destination (Join-Path $output $name)}
Get-FileHash -LiteralPath $feed -Algorithm SHA256
