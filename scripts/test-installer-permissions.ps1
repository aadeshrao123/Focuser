param([Parameter(Mandatory=$true)][string]$GeneratedPath)
$ErrorActionPreference='Stop'
# Exercise the real rendered permission commands on owner-controlled scratch
# files. Never change Program Files, SCM, tasks, production storage or ownership.
$scratch=Join-Path ([IO.Path]::GetTempPath()) ('Focuser-Installer-Permissions-'+[guid]::NewGuid())
$probe=Join-Path $scratch 'helper.exe'
$nested=Join-Path $scratch 'resources'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$icacls=Join-Path $env:WINDIR 'System32\icacls.exe'
function Invoke-Icacls([string]$Arguments){
    $info=[Diagnostics.ProcessStartInfo]::new($icacls,$Arguments)
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    $process=[Diagnostics.Process]::Start($info)
    $process.WaitForExit()
    if($process.ExitCode -ne 0){throw "Permission command failed: $Arguments"}
    $process.Dispose()
}
function Assert-ReadableProtectedFile([string]$Path){
    if([IO.File]::ReadAllText($Path) -ne 'test payload'){throw 'Installed payload is unreadable or changed'}
    $acl=Get-Acl -LiteralPath $Path
    if($acl.AreAccessRulesProtected){throw 'Extracted file did not inherit the protected directory ACL'}
    $rules=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
    $users=@($rules | Where-Object {$_.IdentityReference.Value -eq 'S-1-5-32-545'})
    if($rules.Count -eq 0 -or $users.Count -eq 0){throw 'Empty effective file permissions'}
    $rights=[int64]0
    foreach($rule in $users){
        if($rule.AccessControlType -ne 'Allow' -or $rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly){throw 'Unexpected Users file permission'}
        $rights=$rights -bor [int64]$rule.FileSystemRights
    }
    $rx=[int64][Security.AccessControl.FileSystemRights]::ReadAndExecute
    if(($rights -band $rx) -ne $rx -or ($rights -band 0xD0156) -ne 0){throw 'Users require read/execute without write/delete/ACL rights'}
}
try {
    New-Item -ItemType Directory -Path $nested -Force | Out-Null
    [IO.File]::WriteAllText($probe,'test payload')
    [IO.File]::WriteAllText((Join-Path $nested 'resource.dll'),'test payload')
    $commands=@()
    foreach($line in Get-Content -LiteralPath $GeneratedPath){
        if($line -match '^\s*nsExec::ExecToLog /TIMEOUT=120000 ''"\$SYSDIR\\icacls\.exe" (?<args>.*)''$'){
            $arguments=$Matches.args
            if($arguments.Contains('/grant:r') -or $arguments.Contains('/reset')){$commands+= $arguments.Replace('$INSTDIR',$scratch)}
        }
    }
    if($commands.Count -ne 2){throw 'Expected one directory grant and one inherited-child reset in the generated installer'}
    foreach($command in $commands){Invoke-Icacls $command}
    Assert-ReadableProtectedFile $probe
    Assert-ReadableProtectedFile (Join-Path $nested 'resource.dll')
    Write-Output 'Rendered installer permission commands passed file and nested-resource ACL checks.'
} finally {
    if(Test-Path -LiteralPath $scratch){
        # Restore only this exact owner-controlled fixture, then remove it.
        Invoke-Icacls ('"'+$scratch+'" /grant:r *'+$sid+':(OI)(CI)F /Q')
        Invoke-Icacls ('"'+$scratch+'\*" /reset /T /Q')
        $resolved=(Resolve-Path -LiteralPath $scratch).Path
        $expectedParent=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
        if([IO.Path]::GetDirectoryName($resolved) -ne $expectedParent -or [IO.Path]::GetFileName($resolved) -notlike 'Focuser-Installer-Permissions-*'){throw 'Refusing unexpected fixture cleanup path'}
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
