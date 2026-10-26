Set-StrictMode -Version Latest

function Assert-ColdRuntime($Service, [string]$Installed, $Processes) {
    if (Test-Path -LiteralPath $Installed) { throw 'Installed program files still exist. Use the ordinary service-authorized uninstaller; this tool only recovers a missing installation.' }
    if ($Service -and $Service.State -ne 'Stopped') { throw 'Focuser service is still running or transitioning. Recovery will not stop enforcement. Use ordinary uninstall.' }
    $expected='"'+(Join-Path $Installed 'focuser-service.exe')+'" service'
    if ($Service -and ($Service.PathName -ine $expected -or $Service.StartName -notin @('LocalSystem','NT AUTHORITY\SYSTEM'))) { throw 'The service registration does not belong to this installation.' }
    foreach($process in $Processes) {
        if ($process.ExecutablePath -and ($process.ExecutablePath.StartsWith($Installed+'\',[StringComparison]::OrdinalIgnoreCase) -or $process.ExecutablePath -ieq (Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'FocuserAuthority\update-broker.exe'))) { throw 'An installed Focuser process is still alive. Recovery will not terminate it.' }
    }
}
function Get-RecoveryTree([string]$Root) {
    $rootPath=[IO.Path]::GetFullPath($Root)
    $entries=[Collections.Generic.List[object]]::new()
    function Visit([string]$Path,[int]$Depth) {
        if($Depth -gt 32 -or $entries.Count -ge 50000){throw 'Recovery inventory exceeds its safety bound.'}
        $item=Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw ('Refusing redirected recovery object: '+$Path)}
        if(-not ($item.FullName -ieq $rootPath -or $item.FullName.StartsWith($rootPath+'\',[StringComparison]::OrdinalIgnoreCase))){throw 'Recovery object escaped its fixed root.'}
        $entries.Add($item)
        if($item.PSIsContainer){foreach($child in Get-ChildItem -LiteralPath $Path -Force -ErrorAction Stop){Visit $child.FullName ($Depth+1)}}
    }
    Visit $rootPath 0
    return $entries.ToArray()
}
function Assert-PrivateRecoveryTree($Entries) {
    foreach($entry in $Entries){
        $acl=Get-Acl -LiteralPath $entry.FullName
        if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin @('S-1-5-18','S-1-5-32-544')){throw 'Untrusted authority file owner.'}
        $rules=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
        if(-not $rules.Count){throw 'Missing authority ACL.'}
        foreach($rule in $rules){if($rule.AccessControlType -ne 'Allow' -or $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')){throw 'Authority ACL is not private; manual recovery is required.'}}
    }
}
function Assert-RecoveryTask([string]$Xml,[string]$Name,[string]$Installed,[string]$Root,[string]$Sid,[string]$Installation) {
    $settings=[Xml.XmlReaderSettings]::new();$settings.DtdProcessing=[Xml.DtdProcessing]::Prohibit
    $reader=[Xml.XmlReader]::Create([IO.StringReader]::new($Xml),$settings)
    try{$doc=[Xml.XmlDocument]::new();$doc.XmlResolver=$null;$doc.Load($reader)}finally{$reader.Dispose()}
    $ns=[Xml.XmlNamespaceManager]::new($doc.NameTable);$ns.AddNamespace('t','http://schemas.microsoft.com/windows/2004/02/mit/task')
    $principals=$doc.SelectNodes('/t:Task/t:Principals/t:Principal',$ns)
    $actions=$doc.SelectNodes('/t:Task/t:Actions/*',$ns)
    if($principals.Count -ne 1 -or $actions.Count -ne 1 -or $actions[0].LocalName -cne 'Exec'){throw 'Unexpected Focuser task actions/principals.'}
    if($doc.SelectSingleNode('/t:Task/t:Actions',$ns).GetAttribute('Context') -cne $principals[0].GetAttribute('id')){throw 'Task principal context mismatch.'}
    if($Name -ceq 'SessionAgent'){
        $checks=@{'Description'=('Focuser session agent '+$Installation);'UserId'=$Sid;'LogonType'='InteractiveToken';'Command'=(Join-Path $Installed 'focuser-session-agent.exe')}
        # Task Scheduler drops the default LeastPrivilege field on export.
        $levels=$doc.SelectNodes('/t:Task/t:Principals/t:Principal/t:RunLevel',$ns)
        if($levels.Count -gt 1 -or ($levels.Count -eq 1 -and $levels[0].InnerText.Trim() -cne 'LeastPrivilege')){throw 'Unexpected agent task privilege level.'}
        $expectedArgs=''
    }elseif($Name -ceq 'UpdateRecovery'){
        $checks=@{'Description'=('Focuser update recovery '+$Installation);'UserId'='S-1-5-18';'RunLevel'='HighestAvailable';'Command'=(Join-Path $Root 'update-broker.exe')}
        $expectedArgs='update-broker'
    }else{throw 'Unknown task name.'}
    foreach($namePart in $checks.Keys){
        $xpath=switch($namePart){'Description'{'/t:Task/t:RegistrationInfo/t:Description'};'Command'{'/t:Task/t:Actions/t:Exec/t:Command'};default{'/t:Task/t:Principals/t:Principal/t:'+$namePart}}
        $nodes=$doc.SelectNodes($xpath,$ns)
        if($nodes.Count -ne 1 -or $nodes[0].InnerText.Trim() -ine $checks[$namePart]){throw ('Task ownership mismatch: '+$namePart)}
    }
    $arguments=$doc.SelectNodes('/t:Task/t:Actions/t:Exec/t:Arguments',$ns)
    if($arguments.Count -gt 1 -or ($arguments.Count -eq 0 -and $expectedArgs -ne '') -or ($arguments.Count -eq 1 -and $arguments[0].InnerText.Trim() -cne $expectedArgs)){throw 'Unexpected recovery task arguments.'}
    $work=$doc.SelectNodes('//t:WorkingDirectory',$ns)
    if($work.Count -gt 1 -or ($work.Count -eq 1 -and $work[0].InnerText.Trim() -ne '')){throw 'Unexpected task working directory.'}
}
function Get-RecoveryHosts([byte[]]$Bytes) {
    if($Bytes.Length -gt 4194304){throw 'Hosts file exceeds recovery limits.'}
    $encoding=[Text.UTF8Encoding]::new($false,$true)
    $text=$encoding.GetString($Bytes)
    $bar=([char]0x2500).ToString()*4
    $begin=[regex]::Matches($text,('(?m)^'+[regex]::Escape('# '+$bar+' BEGIN FOCUSER BLOCK '+$bar)+'\r?$'))
    $end=[regex]::Matches($text,('(?m)^'+[regex]::Escape('# '+$bar+' END FOCUSER BLOCK '+$bar)+'\r?$'))
    if($begin.Count -eq 0 -and $end.Count -eq 0){return ,$Bytes}
    if($begin.Count -ne 1 -or $end.Count -ne 1 -or $end[0].Index -le $begin[0].Index){throw 'Malformed/duplicate Focuser hosts markers; no cleanup was authorized.'}
    $stop=$end[0].Index+$end[0].Length
    if($stop -lt $text.Length -and $text[$stop] -eq "`n"){$stop++}
    return ,$encoding.GetBytes($text.Remove($begin[0].Index,$stop-$begin[0].Index))
}
function Invoke-ColdInspection([string]$Helper,[string]$Database) {
    $info=[Diagnostics.ProcessStartInfo]::new($Helper,('recovery-inspect --database "'+$Database+'"'))
    $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    $info.StandardOutputEncoding=[Text.Encoding]::UTF8;$info.StandardErrorEncoding=[Text.Encoding]::UTF8
    $process=[Diagnostics.Process]::Start($info)
    try {
        $output=$process.StandardOutput.ReadToEndAsync();$errors=$process.StandardError.ReadToEndAsync()
        if(-not $process.WaitForExit(15000)){$process.Kill();throw 'Read-only recovery inspection timed out.'}
        if($process.ExitCode -ne 0){throw ('Recovery refused: '+$errors.Result)}
        $result=$output.Result | ConvertFrom-Json
        if(-not $result.decision.allowed -or $result.installation -notmatch '^[0-9a-f-]{36}$' -or $result.database_sha256 -notmatch '^[0-9A-F]{64}$'){throw 'Recovery inspection returned an invalid decision.'}
        return $result
    }finally{$process.Dispose()}
}
