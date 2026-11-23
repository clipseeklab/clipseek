$ErrorActionPreference = 'Stop'
if ($env:CI) { throw 'HEIF source builds are local Windows maintenance only.' }
$project = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$cache = Join-Path $project '.cache\heif-build'
$sdk = Join-Path $cache 'install'
$staging = Join-Path $project 'runtime'
New-Item -ItemType Directory -Force -Path $cache,$staging | Out-Null
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'HEIF helper builds require Visual Studio C++ x64 tools.' }
$vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'

if (-not (Test-Path -LiteralPath (Join-Path $sdk 'include\libheif\heif.h')) -or
    -not (Test-Path -LiteralPath (Join-Path $sdk 'lib\heif.lib'))) {
    # Rebuild the import SDK from pinned, unmodified upstream release sources.
    $archives = @(
        @{name='libde265';version='1.1.3';sha='554228bd17788c99a7e63b37ab5634722190e6e2bf60c1dcb01cef328e133905'},
        @{name='libheif';version='1.23.5';sha='fd9036064c4432f0550d15072ddf34956a248279ee9aeaff0fba3fa0f77d8f1a'}
    )
    foreach ($archive in $archives) {
        $filename = "$($archive.name)-$($archive.version).tar.gz"
        $path = Join-Path $cache $filename
        if (-not (Test-Path -LiteralPath $path)) {
            Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/strukturag/$($archive.name)/releases/download/v$($archive.version)/$filename" -OutFile $path
        }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLower() -ne $archive.sha) {
            throw "Upstream source hash mismatch: $filename"
        }
        if (-not (Test-Path -LiteralPath (Join-Path $cache "$($archive.name)-$($archive.version)"))) {
            & tar -xf $path -C $cache
            if ($LASTEXITCODE -ne 0) { throw "Cannot extract $filename" }
        }
    }
    $cmake = Join-Path $env:ProgramFiles 'CMake\bin\cmake.exe'
    $ninja = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe'
    $sdkCommands = @(
        "call `"$vcvars`" >nul",
        "`"$cmake`" -S `"$cache\libde265-1.1.3`" -B `"$cache\libde265-ninja`" -G Ninja -DCMAKE_MAKE_PROGRAM=`"$ninja`" -DCMAKE_BUILD_TYPE=Release -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DCMAKE_INSTALL_PREFIX=`"$sdk`" -DENABLE_DECODER=OFF -DENABLE_ENCODER=OFF -DENABLE_SDL=OFF -DENABLE_AVX512=OFF -DBUILD_SHARED_LIBS=ON",
        "`"$cmake`" --build `"$cache\libde265-ninja`" --parallel 2",
        "`"$cmake`" --install `"$cache\libde265-ninja`"",
        "`"$cmake`" -S `"$cache\libheif-1.23.5`" -B `"$cache\libheif-ninja`" -G Ninja -DCMAKE_MAKE_PROGRAM=`"$ninja`" -DCMAKE_BUILD_TYPE=Release -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DCMAKE_INSTALL_PREFIX=`"$sdk`" -DCMAKE_PREFIX_PATH=`"$sdk`" -DBUILD_SHARED_LIBS=ON -DENABLE_PLUGIN_LOADING=OFF -DWITH_LIBDE265=ON -DWITH_X265=OFF -DWITH_X264=OFF -DWITH_AOM_DECODER=OFF -DWITH_AOM_ENCODER=OFF -DWITH_OpenH264_DECODER=OFF -DWITH_JPEG_DECODER=OFF -DWITH_JPEG_ENCODER=OFF -DWITH_LIBSHARPYUV=OFF -DWITH_UNCOMPRESSED_CODEC=ON -DWITH_EXAMPLES=OFF -DBUILD_TESTING=OFF -DBUILD_DOCUMENTATION=OFF",
        "`"$cmake`" --build `"$cache\libheif-ninja`" --parallel 2",
        "`"$cmake`" --install `"$cache\libheif-ninja`""
    )
    $sdkScript = Join-Path $cache 'clipseek-build-sdk.cmd'
    $sdkLines = @('@echo off', 'chcp 65001 >nul') + @($sdkCommands | ForEach-Object { $_; 'if errorlevel 1 exit /b %errorlevel%' })
    [IO.File]::WriteAllLines($sdkScript, $sdkLines, [Text.UTF8Encoding]::new($false))
    & cmd /d /c $sdkScript
    if ($LASTEXITCODE -ne 0) { throw 'HEIF import SDK build failed.' }
}
$source = Join-Path $project 'src-tauri\windows\heif-decoder.cpp'
$helper = Join-Path $staging 'heif-decoder.exe'
$script = Join-Path $cache 'clipseek-build-helper.cmd'
$helperLines = @("@echo off", 'chcp 65001 >nul', "call `"$vcvars`" >nul", 'if errorlevel 1 exit /b %errorlevel%',
  "cl /nologo /O2 /MT /EHsc /std:c++17 /utf-8 /I`"$sdk\include`" `"$source`" /Fo`"$cache\clipseek-heif-decoder.obj`" /link /LIBPATH:`"$sdk\lib`" heif.lib /OUT:`"$helper`"")
[IO.File]::WriteAllLines($script, $helperLines, [Text.UTF8Encoding]::new($false))
& cmd /d /c $script
if ($LASTEXITCODE -ne 0) { throw 'HEIF helper compilation failed.' }
foreach ($name in @('heif.dll', 'libde265.dll')) {
    Copy-Item -LiteralPath (Join-Path $sdk "bin\$name") -Destination (Join-Path $staging $name) -Force
}
foreach ($name in @('libheif', 'libde265')) {
    $version = if ($name -eq 'libheif') { '1.23.5' } else { '1.1.3' }
    Copy-Item (Join-Path $cache "$name-$version\COPYING") (Join-Path $staging "heif-$name-COPYING") -Force
}
$version = & $helper --version
if ($LASTEXITCODE -ne 0 -or $version -notmatch 'libheif 1\.23\.5$') { throw 'Unexpected HEIF helper version.' }
Write-Output "Prepared $version"
