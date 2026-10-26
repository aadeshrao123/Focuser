param([Parameter(Mandatory=$true)][string]$Path)
$ErrorActionPreference='Stop'
# Certificate-store access only. Never accept/export a private key in build args.
if($env:FOCUSER_SIGN_CERT_SHA1 -notmatch '^[A-Fa-f0-9]{40}$'){throw 'A code-signing certificate store thumbprint is required'}
if($env:FOCUSER_AUTHENTICODE_SHA256 -notmatch '^[A-Fa-f0-9]{64}$'){throw 'Configure the SHA-256 publisher certificate pin before signing'}
$certificate=$null
foreach($store in @('Cert:\CurrentUser\My','Cert:\LocalMachine\My')){
    $candidate=Get-Item -LiteralPath (Join-Path $store $env:FOCUSER_SIGN_CERT_SHA1) -ErrorAction SilentlyContinue
    if($candidate -and $candidate.HasPrivateKey){$certificate=$candidate;break}
}
if(-not $certificate -or $certificate.NotAfter -le [DateTime]::Now -or $certificate.NotBefore -gt [DateTime]::Now){throw 'No valid private code-signing certificate is available'}
if(-not @($certificate.EnhancedKeyUsageList | Where-Object {$_.ObjectId.Value -eq '1.3.6.1.5.5.7.3.3'}).Count){throw 'Certificate must allow code signing'}
$timestamp=$null
if(-not [Uri]::TryCreate($env:FOCUSER_SIGN_TIMESTAMP,[UriKind]::Absolute,[ref]$timestamp) -or $timestamp.Scheme -ne 'https'){throw 'Configure an HTTPS timestamp server using FOCUSER_SIGN_TIMESTAMP'}
$publisher=[BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($certificate.RawData)).Replace('-','').ToLowerInvariant()
if($publisher -ne $env:FOCUSER_AUTHENTICODE_SHA256.ToLowerInvariant()){throw 'Signing certificate does not match the compiled publisher pin'}
$file=Get-Item -LiteralPath $Path
if($file.PSIsContainer -or $file.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Refusing to sign a directory or reparse point'}
$signature=Set-AuthenticodeSignature -LiteralPath $file.FullName -Certificate $certificate -HashAlgorithm SHA256 -TimestampServer $timestamp.AbsoluteUri
if($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate){throw "Code signing/timestamp validation failed: $($file.Name)"}
$verified=Get-AuthenticodeSignature -LiteralPath $file.FullName
if($verified.Status -ne 'Valid' -or $verified.SignerCertificate.Thumbprint -ne $certificate.Thumbprint){throw 'Post-signature verification failed'}
