# Authenticode-sign one file via Azure Artifact Signing (Trusted Signing).
# Called by Tauri as: pwsh ... sign-windows.ps1 <path>
# Requires prior azure/login (or AZURE_CLIENT_* env) and the TrustedSigning module.

param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$FilePath
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $FilePath)) {
    throw "File to sign not found: $FilePath"
}

$endpoint = $env:AZURE_TRUSTED_SIGNING_ENDPOINT
$account = $env:AZURE_TRUSTED_SIGNING_ACCOUNT
$profile = $env:AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE

if (-not $endpoint -or -not $account -or -not $profile) {
    throw @"
Missing Artifact Signing settings. Set:
  AZURE_TRUSTED_SIGNING_ENDPOINT
  AZURE_TRUSTED_SIGNING_ACCOUNT
  AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE
"@
}

if (-not (Get-Module -ListAvailable -Name TrustedSigning)) {
    Install-Module -Name TrustedSigning -Force -Scope CurrentUser -Repository PSGallery -AllowClobber
}
Import-Module TrustedSigning -Force

Write-Host "Signing $FilePath"
Write-Host "  endpoint=$endpoint account=$account profile=$profile"

Invoke-TrustedSigning `
    -Endpoint $endpoint `
    -CodeSigningAccountName $account `
    -CertificateProfileName $profile `
    -Files $FilePath `
    -FileDigest SHA256 `
    -TimestampRfc3161 "http://timestamp.acs.microsoft.com" `
    -TimestampDigest SHA256

Write-Host "Signed OK: $FilePath"
