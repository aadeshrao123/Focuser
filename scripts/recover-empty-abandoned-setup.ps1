param([switch]$Apply,[string]$ExpectedDatabaseSha256,[string]$ExpectedInstallation)
$ErrorActionPreference='Stop'
# Explicit administrator recovery of a proven EMPTY, abandoned setup only.
# This is not called by the installer, service, UI, CLI or ordinary uninstall.
# Archive every original byte/ACL; never delete/reset data or launch enforcement.
if(-not [Environment]::Is64BitProcess){throw 'Use 64-bit PowerShell'}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator recovery requires elevation'}
$parent=[Environment]::GetFolderPath('CommonApplicationData')
$root=Join-Path $parent 'FocuserAuthority'
$installed=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Focuser'
$mutex=[Threading.Mutex]::new($false,'Global\Focuser.Installation')
$held=$false
$maintenance=$null
try {
    try {$held=$mutex.WaitOne(0)} catch [Threading.AbandonedMutexException] {$held=$true}
    if(-not $held){throw 'Close Focuser Setup before administrator recovery'}
    if(@([ServiceProcess.ServiceController]::GetServices() | Where-Object {$_.ServiceName -eq 'FocuserAuthority'}).Count){throw 'An authority service exists; use ordinary service-authorized maintenance'}
    if(Test-Path -LiteralPath $installed){throw 'Installed program files exist; this recovery is only for the missing-payload failure'}
    $native=@('Software\Google\Chrome\NativeMessagingHosts\com.focuser.native','Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native','Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.focuser.native','Software\Mozilla\NativeMessagingHosts\com.focuser.native')
    foreach($p in @($parent,$root)){
        if((Get-Item -LiteralPath $p -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse-point recovery path refused'}
    }
    $config=Get-Content -LiteralPath (Join-Path $root 'service.json') -Raw | ConvertFrom-Json
    if($config.version -ne 1 -or $config.agent_image -ine (Join-Path $installed 'focuser-session-agent.exe') -or $config.native_image -ine (Join-Path $installed 'focuser-native.exe')){throw 'Unexpected enrollment configuration'}
    # Inspect both registry views; alternate administrator credentials must not
    # hide the enrolled user's uninstall or native registration.
    foreach($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)){
        foreach($hive in @([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::Users)){
            $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey($hive,$view)
            try {
                $prefix=if($hive -eq [Microsoft.Win32.RegistryHive]::Users){$config.owner_sid+'\'}else{''}
                $keys=@('Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser')+$native
                if($hive -eq [Microsoft.Win32.RegistryHive]::LocalMachine){$keys+=@('SYSTEM\CurrentControlSet\Services\FocuserAuthority')}
                foreach($name in $keys){
                    $key=$base.OpenSubKey($prefix+$name)
                    if($key){
                        try {
                            # Preserve matching native registrations in place;
                            # the normal installer recreates their manifest files.
                            if($name -in $native){
                                $manifest=if($name -eq $native[3]){'native-firefox.json'}else{'native-chromium.json'}
                                if($key.GetValue('') -ine (Join-Path $installed $manifest) -or $key.GetSubKeyNames().Count -ne 0 -or $key.GetValueNames().Count -ne 1){throw 'Unexpected native registration ownership'}
                            } else {throw ('A Focuser registration remains: '+$hive+'/'+$view+'/'+$prefix+$name+'. Nothing was archived.')}
                        } finally {$key.Dispose()}
                    }
                }
            } finally {$base.Dispose()}
        }
    }
    $tasks=Join-Path $env:WINDIR 'System32\Tasks\Focuser'
    $entries=@(Get-ChildItem -LiteralPath $root -Force)
    foreach($entry in $entries){
        if($entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -or ($entry.Name -notin @('authority.db','authority.lock','maintenance.lock','service.json','session-agent.xml') -and $entry.Name -notmatch '^operation-[0-9a-f-]+\.log$')){throw 'Unknown files or pending maintenance require manual recovery'}
    }
    # Refuse unreadable/untrusted private ACLs; do not loosen any permissions.
    foreach($entry in @((Get-Item -LiteralPath $root -Force))+$entries){
        $acl=Get-Acl -LiteralPath $entry.FullName
        $owner=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
        if($owner -notin @('S-1-5-18','S-1-5-32-544')){throw 'Unexpected private file owner'}
        $rules=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
        if(-not $rules.Count){throw 'Missing private ACL'}
        foreach($rule in $rules){if($rule.AccessControlType -ne 'Allow' -or $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')){throw 'Unexpected private ACL'}}
    }
    $maintenance=[IO.File]::Open((Join-Path $root 'maintenance.lock'),[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    # A running authority, including one started outside SCM, holds this file.
    $authority=[IO.File]::Open((Join-Path $root 'authority.lock'),[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try {
        $node=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'nodejs\node.exe'
        $script=Join-Path $PSScriptRoot 'inspect-empty-authority.cjs'
        $info=[Diagnostics.ProcessStartInfo]::new($node,('--no-warnings "'+$script+'" "'+(Join-Path $root 'authority.db')+'"'))
        $info.UseShellExecute=$false; $info.CreateNoWindow=$true
        $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
        $probe=[Diagnostics.Process]::Start($info)
        $output=$probe.StandardOutput.ReadToEndAsync(); $errors=$probe.StandardError.ReadToEndAsync()
        if(-not $probe.WaitForExit(15000)){$probe.Kill();throw 'Owned read-only inspection process timed out'}
        if($probe.ExitCode -ne 0){throw ('Empty-setup recovery refused: '+$errors.Result)}
        $check=$output.Result | ConvertFrom-Json
        $probe.Dispose()
        if(Test-Path -LiteralPath $tasks){
            $taskFiles=@(Get-ChildItem -LiteralPath $tasks -Force)
            if($taskFiles.Count -ne 1 -or $taskFiles[0].Name -cne 'SessionAgent' -or $taskFiles[0].PSIsContainer -or ($taskFiles[0].Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Unknown startup tasks require manual recovery'}
            $settings=[Xml.XmlReaderSettings]::new();$settings.DtdProcessing=[Xml.DtdProcessing]::Prohibit
            $reader=[Xml.XmlReader]::Create($taskFiles[0].FullName,$settings)
            try {$task=[Xml.XmlDocument]::new();$task.XmlResolver=$null;$task.Load($reader)} finally {$reader.Dispose()}
            $ns=[Xml.XmlNamespaceManager]::new($task.NameTable);$ns.AddNamespace('t','http://schemas.microsoft.com/windows/2004/02/mit/task')
            $principal=$task.SelectNodes('/t:Task/t:Principals/t:Principal',$ns)
            $actions=$task.SelectNodes('/t:Task/t:Actions/*',$ns)
            if($principal.Count -ne 1 -or $actions.Count -ne 1 -or $actions[0].LocalName -cne 'Exec'){throw 'Unexpected startup task structure'}
            $checks=@{
                '/t:Task/t:RegistrationInfo/t:Description'=('Focuser session agent '+$check.installation)
                '/t:Task/t:Principals/t:Principal/t:UserId'=$config.owner_sid
                '/t:Task/t:Principals/t:Principal/t:LogonType'='InteractiveToken'
                '/t:Task/t:Principals/t:Principal/t:RunLevel'='LeastPrivilege'
                '/t:Task/t:Actions/t:Exec/t:Command'=$config.agent_image
            }
            foreach($xpath in $checks.Keys){$nodes=$task.SelectNodes($xpath,$ns);if($nodes.Count -ne 1 -or $nodes[0].InnerText.Trim() -ine $checks[$xpath]){throw 'Existing startup task does not match this installation'}}
            $arguments=$task.SelectNodes('/t:Task/t:Actions/t:Exec/t:Arguments',$ns)
            if($arguments.Count -gt 1 -or ($arguments.Count -eq 1 -and $arguments[0].InnerText.Trim() -ne '') -or $task.SelectSingleNode('/t:Task/t:Actions',$ns).GetAttribute('Context') -cne $principal[0].GetAttribute('id')){throw 'Unexpected startup task arguments or principal'}
            # Leave the owned task untouched. Import preserves its installation
            # identity, so the existing provisioning code can repair it normally.
        }
        if($authority.Length -ne 0 -or $maintenance.Length -ne 0){throw 'Unexpected lock file contents'}
        $emptyHash='E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855'
        $fingerprints=@($entries | ForEach-Object {
            $hash=if($_.Name -in @('authority.lock','maintenance.lock')){$emptyHash}else{(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
            [ordered]@{name=$_.Name;sha256=$hash}
        })
        if(-not $Apply){[ordered]@{eligible=$true;installation=$check.installation;database_sha256=$check.sha256;owner_sid=$config.owner_sid;files=$fingerprints} | ConvertTo-Json -Depth 5;return}
        if($ExpectedDatabaseSha256 -notmatch '^[0-9a-fA-F]{64}$' -or $ExpectedDatabaseSha256 -ine $check.sha256 -or $ExpectedInstallation -cne $check.installation){throw 'Expected installation/database changed; inspect again before recovery'}
        if((Get-FileHash -LiteralPath (Join-Path $root 'authority.db') -Algorithm SHA256).Hash -ine $check.sha256){throw 'Database changed during inspection'}
        $archive=Join-Path $parent ('FocuserAuthority.Recovery-'+[guid]::NewGuid().ToString('N'))
        if([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($archive)) -ine [IO.Path]::GetFullPath($parent).TrimEnd('\')){throw 'Unexpected recovery archive path'}
    } finally {$authority.Dispose()}
    $maintenance.Dispose();$maintenance=$null
    # Same-volume, whole-directory rename preserves all data and original ACLs.
    # No replacement authority is initialized. The user must explicitly import.
    [IO.Directory]::Move($root,$archive)
    foreach($file in $fingerprints){if((Get-FileHash -LiteralPath (Join-Path $archive $file.name) -Algorithm SHA256).Hash -ine $file.sha256){throw ('Recovery verification failed; all files retained at '+$archive)}}
    [ordered]@{archived=$archive;source=(Join-Path $archive 'authority.db');owner_sid=$config.owner_sid;installation=$check.installation;database_sha256=$check.sha256;files=$fingerprints;enforcement_started=$false} | ConvertTo-Json -Depth 5
} finally {
    if($maintenance){$maintenance.Dispose()}
    if($held){$mutex.ReleaseMutex()}
    $mutex.Dispose()
}
