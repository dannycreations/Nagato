$ErrorActionPreference = 'Stop'

$Owner = "dannycreations"
$Repo = "nagato"
$AssetName = "nagato-windows-amd64.exe"
$BinaryName = "nagato.exe"

$Arch = if ($Is64BitProcess -or $env:PROCESSOR_ARCHITECTURE -eq 'AMD64') { "amd64" } else { "386" }
if ($Arch -ne "amd64") {
  Write-Error "Unsupported Architecture: $Arch. Only amd64 is supported for Windows."
  exit 1
}

$Version = if ($args[0]) { $args[0] } elseif ($env:NAGATO_VERSION) { $env:NAGATO_VERSION } else { "latest" }
$Url = if ($Version -eq "latest") {
  "https://api.github.com/repos/$Owner/$Repo/releases/latest"
}
else {
  "https://api.github.com/repos/$Owner/$Repo/releases/tags/$Version"
}

Write-Host "Fetching $Version release info..."

try {
  $Headers = @{ "User-Agent" = "nagato-installer" }
  $ReleaseInfo = Invoke-RestMethod -Uri $Url -Headers $Headers -UseBasicParsing
  $DownloadUrl = ($ReleaseInfo.assets | Where-Object { $_.name -eq $AssetName }).browser_download_url
}
catch {
  Write-Error $_.Exception.Message
  exit 1
}

if (-not $DownloadUrl) {
  Write-Error "Could not find binary for Windows-amd64 in $Version release."
  exit 1
}

$InstallDir = Join-Path $HOME ".local\bin"
if (-not (Test-Path $InstallDir)) {
  New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$DestPath = Join-Path $InstallDir $BinaryName

Write-Host "Downloading $AssetName..."
Invoke-WebRequest -Uri $DownloadUrl -OutFile $DestPath -UseBasicParsing

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
  Write-Host "Adding $InstallDir to User PATH..."
  [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
  $env:Path = "$env:Path;$InstallDir"
  Write-Host "Please restart your terminal to use '$BinaryName'."
}

Write-Host "Nagato installed to $DestPath"
