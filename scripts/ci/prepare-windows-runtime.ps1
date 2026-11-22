$ErrorActionPreference = 'Stop'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path $vswhere)) { throw 'Visual Studio Installer/vswhere.exe is required' }
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'Visual C++ x64 build tools were not found' }
$redist = Join-Path $installation 'VC/Redist/MSVC'
$directories = Get-ChildItem $redist -Directory |
  Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
  Sort-Object { [version]$_.Name } -Descending
$dlls = @('MSVCP140.dll', 'MSVCP140_1.dll', 'VCRUNTIME140.dll', 'VCRUNTIME140_1.dll')
$minimum = [version]'14.44.35211.0'
$source = $null
foreach ($directory in $directories) {
  foreach ($crt in (Get-ChildItem (Join-Path $directory.FullName 'x64') -Directory -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue)) {
    $valid = $true
    foreach ($dll in $dlls) {
      $file = Join-Path $crt.FullName $dll
      if (-not (Test-Path $file)) { $valid = $false; break }
      $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($file)
      $version = [version]::new($info.FileMajorPart, $info.FileMinorPart, $info.FileBuildPart, $info.FilePrivatePart)
      if ($version -lt $minimum) { $valid = $false; break }
    }
    if ($valid) { $source = $crt.FullName; break }
  }
  if ($source) { break }
}
if (-not $source) { throw "A redistributable x64 VC++ runtime >= $minimum is required" }
$output = Join-Path $PSScriptRoot '../../.local/windows-runtime'
New-Item -ItemType Directory -Path $output -Force | Out-Null
foreach ($dll in $dlls) { Copy-Item (Join-Path $source $dll) (Join-Path $output $dll) -Force }
Write-Host 'Prepared app-local Microsoft VC++ runtime DLLs.'
