param([switch]$Apply)
$ErrorActionPreference='Stop'
# Recovery for the pre-provisioning ACL defect only. This is not an uninstall,
# upgrade, authorization bypass or repair path for an enrolled installation.
$programFiles=[Environment]::GetFolderPath('ProgramFiles')
$programData=[Environment]::GetFolderPath('CommonApplicationData')
$installed=Join-Path $programFiles 'Focuser'
$expected=@('focuser-ui.exe','focuser-cli.exe','focuser-native.exe','focuser-service.exe','focuser-session-agent.exe','WebView2Loader.dll')
function Assert-Unprovisioned {
    if(-not [Environment]::Is64BitProcess){throw 'Run the 64-bit PowerShell host'}
    # Enumerate the readable parent so an inaccessible authority directory is
    # never mistaken for an absent one.
    if(@([IO.Directory]::GetFileSystemEntries($programData) | Where-Object {[IO.Path]::GetFileName($_) -ieq 'FocuserAuthority'}).Count){throw 'Authority storage exists; use service-authorized repair/recovery instead'}
    if(@(Get-CimInstance Win32_Service -Filter "Name='FocuserAuthority'").Count){throw 'Registered authority exists; refusing incomplete-installer recovery'}
    foreach($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)){
        foreach($hive in @([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::CurrentUser)){
            $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey($hive,$view)
            try {
                $key=$base.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser')
                if($key){$key.Dispose();throw 'Completed installer registration exists; ordinary removal must use the authority'}
            } finally {$base.Dispose()}
        }
    }
    $scheduler=New-Object -ComObject 'Schedule.Service'
    $scheduler.Connect()
    try {
        $folder=$scheduler.GetFolder('\Focuser')
        if(@($folder.GetTasks(1)).Count){throw 'Focuser startup task exists; refusing pre-provisioning recovery'}
    } catch {
        if($_.Exception.HResult -ne -2147024894){throw}
    }
    $item=Get-Item -LiteralPath $installed -Force
    if(-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Unexpected installation directory'}
    foreach($parent in @($programFiles,$installed)){
        if((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse-point installation path refused'}
    }
    $files=@(Get-ChildItem -LiteralPath $installed -Force)
    if($files.Count -ne $expected.Count){throw 'Unexpected installation contents; no files changed'}
    foreach($file in $files){
        if($file.PSIsContainer -or $file.Name -notin $expected -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Unexpected installation contents; no files changed'}
    }
}
Assert-Unprovisioned
if(-not $Apply){Write-Output 'Confirmed pre-provisioning payload only. -Apply will restore readable protected ACLs and archive these six files without deleting them.';return}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator elevation is required to repair the failed Program Files ACLs'}
Assert-Unprovisioned
$icacls=Join-Path $env:WINDIR 'System32\icacls.exe'
& $icacls $installed /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX' /Q
if($LASTEXITCODE -ne 0){throw 'Could not secure the installation directory'}
& $icacls (Join-Path $installed '*') /reset /T /Q
if($LASTEXITCODE -ne 0){throw 'Could not restore inherited payload permissions'}
Assert-Unprovisioned
$archive=Join-Path $programFiles ('Focuser-Incomplete-'+[guid]::NewGuid())
$resolved=(Resolve-Path -LiteralPath $installed).Path
if($resolved -ine [IO.Path]::GetFullPath((Join-Path $programFiles 'Focuser')) -or [IO.Path]::GetDirectoryName($archive) -ine $programFiles){throw 'Unexpected recovery paths'}
Move-Item -LiteralPath $resolved -Destination $archive
Write-Output "Incomplete payload preserved at: $archive"
Write-Output 'No service, task, browser integration or policy data was installed, stopped, removed or initialized.'
