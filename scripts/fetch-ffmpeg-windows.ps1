# Download a GPL shared build of ffmpeg for Windows (BtbN/FFmpeg-Builds) into
# dist\windows\ffmpeg, for the installer to place next to retype.exe.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root "dist\windows\ffmpeg"
$zip = Join-Path $env:TEMP "ffmpeg-win64-gpl-shared.zip"
$url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip"
Invoke-WebRequest -Uri $url -OutFile $zip
$tmp = Join-Path $env:TEMP "ffmpeg-extract"
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
Expand-Archive -Path $zip -DestinationPath $tmp
$bin = Get-ChildItem -Path $tmp -Recurse -Directory -Filter bin | Select-Object -First 1
New-Item -ItemType Directory -Force -Path $out | Out-Null
Copy-Item -Path (Join-Path $bin.FullName "*") -Destination $out -Recurse -Force
Remove-Item -Force (Join-Path $out "ffplay.exe") -ErrorAction SilentlyContinue
$lic = Get-ChildItem -Path $tmp -Recurse -File -Filter LICENSE.txt | Select-Object -First 1
if ($lic) { Copy-Item $lic.FullName (Join-Path $out "FFMPEG-LICENSE.txt") }
Write-Host "ffmpeg in $out"
& (Join-Path $out "ffmpeg.exe") -hide_banner -version | Select-Object -First 1
