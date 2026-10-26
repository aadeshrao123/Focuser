param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
if (-not $IsWindows -and $env:OS -ne 'Windows_NT') { throw 'Windows is required' }
if (-not $SkipBuild) {
    Push-Location $workspace
    try {
        & cargo build -p focuser-service -p focuser-cli -p focuser-session-agent -p focuser-native
        if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    } finally { Pop-Location }
}
$binaryDirectory = Join-Path $workspace 'target\debug'
$scratch = Join-Path ([System.IO.Path]::GetTempPath()) ('Focuser-Authority-Test-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
$pipeName = 'harness-' + [guid]::NewGuid()
$authorityProcess = $null
function Start-ScratchAuthority {
    $arguments = @('foreground', '--dev', '--db', ('"' + (Join-Path $scratch 'test.db') + '"'), '--pipe-name', $pipeName,
        '--agent-image', ('"' + (Join-Path $binaryDirectory 'focuser-session-agent.exe') + '"'),
        '--native-image', ('"' + (Join-Path $binaryDirectory 'focuser-native.exe') + '"'))
    Start-Process -FilePath (Join-Path $binaryDirectory 'focuser-service.exe') -ArgumentList $arguments -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $scratch ('stdout-' + [guid]::NewGuid() + '.log')) `
        -RedirectStandardError (Join-Path $scratch ('stderr-' + [guid]::NewGuid() + '.log'))
}
function Invoke-TestCli([string[]]$Arguments, [int]$Expected = 0) {
    $text = & (Join-Path $binaryDirectory 'focuser-cli.exe') --dev --dev-pipe $pipeName --dev-server-pid $authorityProcess.Id --json @Arguments 2>&1
    if ($LASTEXITCODE -ne $Expected) { throw "CLI exit $LASTEXITCODE (expected $Expected): $text" }
    $text -join "`n"
}
try {
    $authorityProcess = Start-ScratchAuthority
    # The client has a bounded connect retry, so no unbounded readiness sleep.
    $created = Invoke-TestCli -Arguments @('list', 'create', 'Continuity harness') | ConvertFrom-Json
    $listId = $created.data.id
    if (-not $listId) { throw 'Missing created list ID' }
    # Separate CLI lifetimes exercise the same authority, like UI disconnects.
    $lists = Invoke-TestCli -Arguments @('list', 'ls')
    if ($lists -notmatch [regex]::Escape($listId)) { throw 'State was not shared' }
    $null = Invoke-TestCli -Arguments @('protect', 'enable', $listId, '--minutes', '60')
    $denial = Invoke-TestCli -Arguments @('list', 'rm', $listId) -Expected 5
    if ($denial -notmatch 'protected') { throw 'Protection denial was not preserved' }
    Stop-Process -Id $authorityProcess.Id
    $authorityProcess.WaitForExit()
    $authorityProcess = Start-ScratchAuthority
    $lists = Invoke-TestCli -Arguments @('list', 'ls')
    if ($lists -notmatch [regex]::Escape($listId)) { throw 'State did not survive restart' }
    $denial = Invoke-TestCli -Arguments @('list', 'rm', $listId) -Expected 5
    if ($denial -notmatch 'protected') { throw 'Commitment did not survive restart' }
    Write-Output "Scratch authority continuity passed. Logs/database retained in $scratch"
} finally {
    if ($authorityProcess -and -not $authorityProcess.HasExited) {
        Stop-Process -Id $authorityProcess.Id
        $authorityProcess.WaitForExit()
    }
}
