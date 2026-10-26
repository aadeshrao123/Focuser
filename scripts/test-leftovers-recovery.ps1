$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'leftovers-recovery-common.ps1')
$script:passed=0
function Check([string]$Name,[scriptblock]$Body){& $Body;$script:passed++;Write-Output ('Passed: '+$Name)}
function Refuses([scriptblock]$Body){$failed=$false;try{& $Body}catch{$failed=$true};if(-not $failed){throw 'Unsafe recovery case was accepted.'}}
$scratch=Join-Path ([IO.Path]::GetTempPath()) ('Focuser-Recovery-Test-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $missing=Join-Path $scratch 'missing-install'
    Check 'missing installed files with no runtime is a cold state' {Assert-ColdRuntime $null $missing @()}
    Check 'running services cannot enter recovery' {Refuses {Assert-ColdRuntime @{State='Running';PathName='';StartName='LocalSystem'} $missing @()}}
    Check 'foreign stopped services cannot be deleted' {Refuses {Assert-ColdRuntime @{State='Stopped';PathName='C:\Other\service.exe';StartName='LocalSystem'} $missing @()}}
    Check 'installed files prevent recovery fallback' {Refuses {Assert-ColdRuntime $null $scratch @()}}
    Check 'live owned processes prevent recovery' {Refuses {Assert-ColdRuntime $null $missing @(@{ExecutablePath=(Join-Path $missing 'focuser-session-agent.exe')})}}
    $task='<Task xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><RegistrationInfo><Description>Focuser session agent install</Description></RegistrationInfo><Principals><Principal id="Owner"><UserId>S-1-5-21-1</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Actions Context="Owner"><Exec><Command>C:\Program Files\Focuser\focuser-session-agent.exe</Command></Exec></Actions></Task>'
    Check 'matching owner task is recognized without title heuristics' {Assert-RecoveryTask $task 'SessionAgent' 'C:\Program Files\Focuser' 'C:\ProgramData\FocuserAuthority' 'S-1-5-21-1' 'install'}
    Check 'Windows export may omit the least-privilege default' {Assert-RecoveryTask ($task.Replace('<RunLevel>LeastPrivilege</RunLevel>','')) 'SessionAgent' 'C:\Program Files\Focuser' 'C:\ProgramData\FocuserAuthority' 'S-1-5-21-1' 'install'}
    Check 'logon trigger SID is separate from the principal SID' {Assert-RecoveryTask ($task.Replace('<Principals>','<Triggers><LogonTrigger><UserId>S-1-5-21-1</UserId></LogonTrigger></Triggers><Principals>')) 'SessionAgent' 'C:\Program Files\Focuser' 'C:\ProgramData\FocuserAuthority' 'S-1-5-21-1' 'install'}
    Check 'explicitly elevated agent tasks are refused' {Refuses {Assert-RecoveryTask ($task.Replace('LeastPrivilege','HighestAvailable')) 'SessionAgent' 'C:\Program Files\Focuser' 'C:\ProgramData\FocuserAuthority' 'S-1-5-21-1' 'install'}}
    Check 'foreign task commands are preserved' {Refuses {Assert-RecoveryTask ($task.Replace('focuser-session-agent.exe','other.exe')) 'SessionAgent' 'C:\Program Files\Focuser' 'C:\ProgramData\FocuserAuthority' 'S-1-5-21-1' 'install'}}
    Check 'old installation task IDs do not authorize cleanup' {Refuses {Assert-RecoveryTask $task 'SessionAgent' 'C:\Program Files\Focuser' 'C:\ProgramData\FocuserAuthority' 'S-1-5-21-1' 'different'}}
    Check 'injected task actions are refused' {Refuses {Assert-RecoveryTask ($task.Replace('</Actions>','<Exec><Command>cmd.exe</Command></Exec></Actions>')) 'SessionAgent' 'C:\Program Files\Focuser' 'C:\ProgramData\FocuserAuthority' 'S-1-5-21-1' 'install'}}
    $encoding=[Text.UTF8Encoding]::new($false)
    $bar=([char]0x2500).ToString()*4
    Check 'unrelated hosts bytes remain identical' {$original=$encoding.GetBytes("# unrelated`r`n127.0.0.1 localhost`r`n");if([Convert]::ToBase64String((Get-RecoveryHosts $original)) -cne [Convert]::ToBase64String($original)){throw 'Hosts bytes changed'}}
    Check 'only the complete marked hosts section is removed' {$text="before`r`n# $bar BEGIN FOCUSER BLOCK $bar`r`n0.0.0.0 example.com`r`n# $bar END FOCUSER BLOCK $bar`r`nafter`r`n";if($encoding.GetString((Get-RecoveryHosts ($encoding.GetBytes($text)))) -cne "before`r`nafter`r`n"){throw 'Wrong hosts cleanup'}}
    Check 'malformed hosts markers are not authorization' {Refuses {Get-RecoveryHosts ($encoding.GetBytes("# $bar BEGIN FOCUSER BLOCK $bar`r`nother`r`n"))}}
    $outside=Join-Path $scratch 'outside';$owned=Join-Path $scratch 'owned';$link=Join-Path $owned 'redirected'
    New-Item -ItemType Directory -Path $outside,$owned | Out-Null
    [IO.File]::WriteAllText((Join-Path $outside 'keep.txt'),'keep')
    New-Item -ItemType Junction -Path $link -Target $outside | Out-Null
    Check 'junction inventories fail before any cleanup' {Refuses {Get-RecoveryTree $owned};if([IO.File]::ReadAllText((Join-Path $outside 'keep.txt')) -cne 'keep'){throw 'Unrelated data changed'}}
    [IO.Directory]::Delete($link)
    $source=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'Recover-FocuserLeftovers.ps1') -Raw
    Check 'fresh decision precedes all destructive machine cleanup' {if($source.IndexOf('$fresh=Invoke-ColdInspection') -gt $source.IndexOf('$folder.DeleteTask') -or $source.IndexOf('$fresh=Invoke-ColdInspection') -gt $source.IndexOf(' delete FocuserAuthority')){throw 'Cleanup precedes inspection'}}
    Check 'backup and scope validation precede recovery move' {if($source.IndexOf("'hosts.original'") -gt $source.IndexOf('[IO.Directory]::Move') -or $source.IndexOf('Recovery move escaped') -gt $source.IndexOf('[IO.Directory]::Move')){throw 'Unsafe recovery move'}}
    Write-Output ("$script:passed leftovers recovery checks passed.")
}finally{
    $absolute=[IO.Path]::GetFullPath($scratch)
    if([IO.Path]::GetDirectoryName($absolute) -ine [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') -or [IO.Path]::GetFileName($absolute) -notlike 'Focuser-Recovery-Test-*'){throw 'Unexpected test cleanup path'}
    Remove-Item -LiteralPath $absolute -Recurse -Force
}
