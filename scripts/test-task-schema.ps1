param([Parameter(Mandatory=$true)][string]$FixtureDirectory)
$ErrorActionPreference='Stop'
# TASK_VALIDATE_ONLY (1) validates in Windows without registering or running a
# task. Never substitute TASK_CREATE, TASK_UPDATE or TASK_CREATE_OR_UPDATE.
$scheduler=New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
$folder=$scheduler.GetFolder('\')
function Assert-Absent([string]$Name){
    try {$unexpected=$folder.GetTask($Name);throw 'Validation unexpectedly registered a task'}
    catch {if($_.Exception.HResult -ne -2147024894){throw}}
}
function Test-Definition([string]$Xml,[string]$User,[int]$Logon,[bool]$Expected){
    $name='Focuser.SchemaValidation.'+[guid]::NewGuid().ToString('N')
    Assert-Absent $name
    $accepted=$true
    try {$result=$folder.RegisterTask($name,$Xml,1,$User,$null,$Logon,$null)}
    catch {$accepted=$false;if($Expected){throw}}
    Assert-Absent $name
    if($accepted -ne $Expected){throw 'Windows accepted malformed task XML'}
}
$agent=Get-Content -LiteralPath (Join-Path $FixtureDirectory 'agent.xml') -Raw
$recovery=Get-Content -LiteralPath (Join-Path $FixtureDirectory 'recovery.xml') -Raw
$owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
Test-Definition $recovery 'SYSTEM' 5 $true
Test-Definition $agent $owner 3 $true
Test-Definition ($recovery.Replace('</UserId>','</UserId><LogonType>ServiceAccount</LogonType>')) 'SYSTEM' 5 $false
Test-Definition ($agent.Replace('LeastPrivilege','Administrator')) $owner 3 $false
# Normalize in an in-memory definition only. This exercises Windows' omission of
# default elements without creating, updating or running any scheduled task.
foreach($entry in @(@('agent',$agent),@('recovery',$recovery))){
    $definition=$scheduler.NewTask(0)
    $definition.XmlText=$entry[1]
    # Registration supplies the actual API logon type for SYSTEM; a newly
    # created definition otherwise retains its unrelated interactive default.
    if($entry[0] -ceq 'recovery'){$definition.Principal.LogonType=5}
    [IO.File]::WriteAllText((Join-Path $FixtureDirectory ($entry[0]+'-exported.xml')),$definition.XmlText,[Text.Encoding]::Unicode)
}
Write-Output 'Windows TASK_VALIDATE_ONLY passed for both generated tasks and rejected both malformed definitions. No tasks were registered or run.'
