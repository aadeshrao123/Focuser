param([switch]$AdministratorWorker,[string]$RecoveryId,[switch]$InspectOnly)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'leftovers-recovery-common.ps1')
if(-not [Environment]::Is64BitProcess){throw 'Use 64-bit Windows PowerShell.'}
$package=Join-Path (Split-Path $PSScriptRoot -Parent) 'target\windows-leftovers-recovery'
$manifest=Get-Content -LiteralPath (Join-Path $package 'tools.json') -Raw | ConvertFrom-Json
foreach($name in @('focuser-service','focuser-session-agent')){
    $tool=Join-Path $package ($name+'.exe')
    if((Get-Item -LiteralPath $tool -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected recovery tool refused.'}
    if((Get-FileHash -LiteralPath $tool -Algorithm SHA256).Hash -cne $manifest.$name){throw ('Recovery tool integrity mismatch: '+$name)}
}
$parent=[Environment]::GetFolderPath('CommonApplicationData')
$root=Join-Path $parent 'FocuserAuthority'
$installed=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Focuser'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if(-not $AdministratorWorker){
    $principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Start the leftovers launcher normally. It elevates only machine cleanup, then restores user data without elevation.'}
    $RecoveryId=[guid]::NewGuid().ToString('N')
    $archive=Join-Path $parent ('FocuserRecovery-'+$RecoveryId)
    $arguments='-NoProfile -ExecutionPolicy Bypass -File "'+$PSCommandPath+'" -AdministratorWorker -RecoveryId '+$RecoveryId
    if($InspectOnly){$arguments+=' -InspectOnly'}
    $worker=Start-Process -FilePath (Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe') -ArgumentList $arguments -Verb RunAs -WindowStyle Hidden -PassThru -Wait
    $receipt=Join-Path $archive 'result.json'
    if($worker.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $receipt)){
        $failure=Join-Path $archive 'failure.txt'
        if(Test-Path -LiteralPath $failure){throw (Get-Content -LiteralPath $failure -Raw)}
        throw 'Administrator recovery was refused or cancelled. No unrestricted replacement was started. See the administrator recovery error.'
    }
    $result=Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
    if($InspectOnly){$result | ConvertTo-Json -Depth 6;return}
    if(-not $result.machine_complete -or $result.owner_sid -cne $sid){throw 'Machine cleanup is incomplete or the enrolled user differs; user data was preserved.'}
    $arguments=@('--cleanup-orphaned-user-data')
    $agent=Start-Process -FilePath (Join-Path $package 'focuser-session-agent.exe') -ArgumentList $arguments -WindowStyle Hidden -PassThru -Wait
    if($agent.ExitCode -ne 0){throw ('Machine leftovers were removed, but user/browser cleanup failed. Original authority and registrations are recoverable at '+$archive+'. Do not report complete removal; review browser recovery journals in the enrolled user session.')}
    Write-Output ('Focuser leftovers removed. Recovery backup: '+$archive)
    return
}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Machine recovery requires administrator permission.'}
if($RecoveryId -notmatch '^[0-9a-f]{32}$'){throw 'Invalid recovery identifier.'}
$archive=Join-Path $parent ('FocuserRecovery-'+$RecoveryId)
$mutex=[Threading.Mutex]::new($false,'Global\Focuser.Installation')
$held=$false;$authority=$null;$maintenance=$null
function Write-RecoveryRecord([string]$Name,$Value){ConvertTo-Json -InputObject $Value -Depth 12 | Set-Content -LiteralPath (Join-Path $archive $Name) -Encoding UTF8}
try {
    try{$held=$mutex.WaitOne(0)}catch [Threading.AbandonedMutexException]{$held=$true}
    if(-not $held){throw 'Another Focuser installation/maintenance operation is running.'}
    foreach($path in @($parent,[Environment]::GetFolderPath('ProgramFiles'))){if((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected system folder refused.'}}
    # Create a new private archive before any deletion. Only this user can read
    # diagnostics; original authority ACLs remain unchanged when moved inside it.
    if(Test-Path -LiteralPath $archive){throw 'Recovery archive already exists.'}
    $acl=[Security.AccessControl.DirectorySecurity]::new()
    $acl.SetSecurityDescriptorSddlForm(('O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;'+$sid+')'))
    [IO.Directory]::CreateDirectory($archive,$acl) | Out-Null
    $service=Get-CimInstance Win32_Service -Filter "Name='FocuserAuthority'"
    $processes=@(Get-CimInstance Win32_Process -Filter "Name='focuser-ui.exe' OR Name='focuser-service.exe' OR Name='focuser-session-agent.exe' OR Name='focuser-native.exe' OR Name='update-broker.exe'")
    Assert-ColdRuntime $service $installed $processes
    $scheduler=New-Object -ComObject 'Schedule.Service';$scheduler.Connect()
    $folder=$null
    try{$folder=$scheduler.GetFolder('\Focuser')}catch{if($_.Exception.HResult -ne -2147024894){throw}}
    $tasks=@();$check=$null;$owner=$sid
    if(Test-Path -LiteralPath $root){
        $entries=@(Get-RecoveryTree $root);Assert-PrivateRecoveryTree $entries
        $config=Get-Content -LiteralPath (Join-Path $root 'service.json') -Raw | ConvertFrom-Json
        if($config.version -ne 1 -or $config.owner_sid -cne $sid -or $config.agent_image -ine (Join-Path $installed 'focuser-session-agent.exe') -or $config.native_image -ine (Join-Path $installed 'focuser-native.exe')){throw 'Enrollment differs. Run recovery from the enrolled administrator user; files were preserved.'}
        $maintenance=[IO.File]::Open((Join-Path $root 'maintenance.lock'),'Open','ReadWrite','None')
        $authority=[IO.File]::Open((Join-Path $root 'authority.lock'),'Open','ReadWrite','None')
        $check=Invoke-ColdInspection (Join-Path $package 'focuser-service.exe') (Join-Path $root 'authority.db')
        if($folder){foreach($task in $folder.GetTasks(1)){
            if($task.Name -in @('SessionAgent','UpdateRecovery')){
                if($task.State -eq 4){throw 'A Focuser task is running; cold recovery cannot terminate it.'}
                Assert-RecoveryTask $task.Xml $task.Name $installed $root $sid $check.installation
                $tasks+=@{name=$task.Name;xml=$task.Xml}
            }
        }}
    }elseif($service -or ($folder -and $folder.GetTasks(1).Count -gt 0)){throw 'Policy/enrollment is missing but service/task resources remain. Manual administrator recovery is required; missing data is not authorization.'}
    $registrations=@()
    $native=@('Software\Google\Chrome\NativeMessagingHosts\com.focuser.native','Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native','Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.focuser.native','Software\Mozilla\NativeMessagingHosts\com.focuser.native')
    foreach($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)){
        foreach($hive in @([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::CurrentUser)){
            $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey($hive,$view)
            try{foreach($keyPath in @($native)+@('Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser')){
                $key=$base.OpenSubKey($keyPath)
                if(-not $key){continue}
                try{
                    if($key.GetSubKeyNames().Count -ne 0){throw 'Unexpected registration subkeys; manual review required.'}
                    if($keyPath -in $native){$expected=Join-Path $installed $(if($keyPath -eq $native[3]){'native-firefox.json'}else{'native-chromium.json'});if($key.GetValue('') -ine $expected -or $key.GetValueNames().Count -ne 1){throw 'Native registration belongs to another installation.'}}
                    elseif($key.GetValue('InstallLocation') -ine $installed){throw 'Uninstall registration belongs to another installation.'}
                    $values=@($key.GetValueNames() | ForEach-Object {@{name=$_;kind=$key.GetValueKind($_).ToString();value=$key.GetValue($_,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}})
                    $registrations+=@{hive=$hive.ToString();view=$view.ToString();path=$keyPath;values=$values}
                }finally{$key.Dispose()}
            }}finally{$base.Dispose()}
        }
    }
    $hosts=Join-Path $env:WINDIR 'System32\drivers\etc\hosts'
    $hostsBefore=[IO.File]::ReadAllBytes($hosts);$hostsAfter=Get-RecoveryHosts $hostsBefore
    [IO.File]::WriteAllBytes((Join-Path $archive 'hosts.original'),$hostsBefore)
    Write-RecoveryRecord 'registrations.json' $registrations
    Write-RecoveryRecord 'tasks.json' $tasks
    Write-RecoveryRecord 'service.json' $service
    Write-RecoveryRecord 'inspection.json' $check
    $menu=Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'Focuser'
    $shortcuts=@()
    if(Test-Path -LiteralPath $menu){
        if((Get-Item -LiteralPath $menu -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected Start menu folder refused.'}
        $shell=New-Object -ComObject WScript.Shell
        foreach($name in @('Focuser.lnk','Uninstall Focuser Completely.lnk')){
            $path=Join-Path $menu $name
            if(Test-Path -LiteralPath $path){
                if((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected shortcut refused.'}
                $link=$shell.CreateShortcut($path)
                $target=Join-Path $installed $(if($name -ceq 'Focuser.lnk'){'focuser-ui.exe'}else{'uninstall.exe'})
                if($link.TargetPath -ine $target -or ($name -ceq 'Focuser.lnk' -and $link.Arguments -ne '') -or ($name -cne 'Focuser.lnk' -and $link.Arguments -cne '/DELETE_DATA')){throw 'Start menu shortcut belongs to another installation.'}
                Copy-Item -LiteralPath $path -Destination (Join-Path $archive $name)
                $shortcuts+=@{path=$path;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}
            }
        }
    }
    if($InspectOnly){Write-RecoveryRecord 'result.json' @{inspected=$true;eligible=$true;owner_sid=$owner;inspection=$check;service_present=[bool]$service;task_count=$tasks.Count;registration_count=$registrations.Count;archive=$archive};return}
    # Final fresh policy/runtime check is BEFORE the first cleanup operation.
    Assert-ColdRuntime (Get-CimInstance Win32_Service -Filter "Name='FocuserAuthority'") $installed @(Get-CimInstance Win32_Process -Filter "Name='focuser-ui.exe' OR Name='focuser-service.exe' OR Name='focuser-session-agent.exe' OR Name='focuser-native.exe' OR Name='update-broker.exe'")
    if($check){$fresh=Invoke-ColdInspection (Join-Path $package 'focuser-service.exe') (Join-Path $root 'authority.db');if($fresh.database_sha256 -cne $check.database_sha256 -or $fresh.installation -cne $check.installation){throw 'Authority changed during recovery. No cleanup was performed.'}}
    foreach($task in $tasks){$current=$folder.GetTask($task.name);if($current.Xml -cne $task.xml -or $current.State -eq 4){throw 'Task changed during recovery.'};$folder.DeleteTask($task.name,0)}
    if($folder -and $folder.GetTasks(1).Count -eq 0 -and $folder.GetFolders(0).Count -eq 0){$scheduler.GetFolder('\').DeleteFolder('Focuser',0)}
    if($service){& (Join-Path $env:WINDIR 'System32\sc.exe') delete FocuserAuthority;if($LASTEXITCODE -ne 0){throw 'Failed to delete the verified stopped service.'}}
    foreach($registration in $registrations){
        $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::$($registration.hive),[Microsoft.Win32.RegistryView]::$($registration.view))
        try{$base.DeleteSubKey($registration.path,$true)}finally{$base.Dispose()}
    }
    if([Convert]::ToBase64String([IO.File]::ReadAllBytes($hosts)) -cne [Convert]::ToBase64String($hostsBefore)){throw 'Hosts file changed; preserved original remains in recovery archive.'}
    if([Convert]::ToBase64String($hostsBefore) -cne [Convert]::ToBase64String($hostsAfter)){
        [IO.File]::WriteAllBytes($hosts,$hostsAfter)
        & (Join-Path $env:WINDIR 'System32\ipconfig.exe') /flushdns | Out-Null
        if($LASTEXITCODE -ne 0){throw 'Hosts were restored but DNS cache cleanup failed; recovery is incomplete.'}
    }
    foreach($shortcut in $shortcuts){if((Get-FileHash -LiteralPath $shortcut.path -Algorithm SHA256).Hash -cne $shortcut.sha256){throw 'Shortcut changed during recovery.'};Remove-Item -LiteralPath $shortcut.path -Force}
    if((Test-Path -LiteralPath $menu) -and @(Get-ChildItem -LiteralPath $menu -Force).Count -eq 0){[IO.Directory]::Delete($menu)}
    if($authority){$authority.Dispose();$authority=$null};if($maintenance){$maintenance.Dispose();$maintenance=$null}
    if($check){
        $resolved=[IO.Path]::GetFullPath($root);$destination=Join-Path $archive 'Authority'
        if($resolved -ine (Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'FocuserAuthority') -or [IO.Path]::GetDirectoryName($destination) -ine $archive){throw 'Recovery move escaped its fixed paths.'}
        [IO.Directory]::Move($resolved,$destination)
        if((Get-FileHash -LiteralPath (Join-Path $destination 'authority.db') -Algorithm SHA256).Hash -cne $check.database_sha256){throw 'Archive verification failed; all original data remains in the archive.'}
    }
    if(@(Get-CimInstance Win32_Service -Filter "Name='FocuserAuthority'").Count -ne 0){throw 'Service deletion is still pending; cleanup is incomplete.'}
    Write-RecoveryRecord 'result.json' @{machine_complete=$true;owner_sid=$owner;browser_cleanup_required=($check -and $check.browser_cleanup_required);archive=$archive}
}catch{
    $message=$_.Exception.Message
    if(Test-Path -LiteralPath $archive){$message | Set-Content -LiteralPath (Join-Path $archive 'failure.txt') -Encoding UTF8}
    Write-Error $message
    exit 1
}finally{
    if($authority){$authority.Dispose()};if($maintenance){$maintenance.Dispose()}
    if($held){$mutex.ReleaseMutex()};$mutex.Dispose()
}
