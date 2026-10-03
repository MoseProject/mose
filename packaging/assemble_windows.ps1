param(
    [Parameter(Mandatory = $true)][string]$BuildDir,
    [Parameter(Mandatory = $true)][string]$FfmpegDir,
    [Parameter(Mandatory = $true)][string]$OutputDir
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
if (Test-Path $OutputDir) { Remove-Item -Recurse -Force $OutputDir }
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$Resources = Join-Path $OutputDir "resources"
$Runtime = Join-Path $Resources "runtime"
New-Item -ItemType Directory -Force -Path (Join-Path $Runtime "bin") | Out-Null

Copy-Item (Join-Path $BuildDir "oca-desktop.exe") $OutputDir
Copy-Item (Join-Path $BuildDir "ytbridge.exe") $OutputDir
Copy-Item (Join-Path $BuildDir "deno.exe") $OutputDir
Copy-Item (Join-Path $BuildDir "espeak-ng-data") $OutputDir -Recurse
Copy-Item (Join-Path $BuildDir "python310.dll") $OutputDir
Copy-Item (Join-Path $BuildDir "DLLs") $OutputDir -Recurse
Copy-Item (Join-Path $BuildDir "Lib") $OutputDir -Recurse

# libav DLLs must be beside the desktop executable for the Windows loader. The CLI is kept in runtime/bin and
# added to PATH by configure_bundled_runtime() for yt-dlp merging/audio extraction.
Copy-Item (Join-Path $FfmpegDir "bin\*.dll") $OutputDir
Copy-Item (Join-Path $FfmpegDir "bin\ffmpeg.exe") (Join-Path $Runtime "bin")
python (Join-Path $PSScriptRoot "verify_ffmpeg_runtime.py") `
    (Join-Path $FfmpegDir "bin\ffmpeg.exe") --platform windows

$env:PATH = "$OutputDir;$env:PATH"
python (Join-Path $PSScriptRoot "verify_ffmpeg_runtime.py") `
    (Join-Path $Runtime "bin\ffmpeg.exe") --platform windows

python (Join-Path $PSScriptRoot "fetch_models.py") $Resources
Copy-Item (Join-Path $PSScriptRoot "bundle-manifest.json") $Resources
python (Join-Path $PSScriptRoot "generate_third_party_notices.py") `
    (Join-Path $Resources "licenses")
New-Item -ItemType Directory -Force -Path (Join-Path $Resources "licenses\fonts") | Out-Null
Copy-Item (Join-Path $RepoRoot "LICENSE") (Join-Path $Resources "licenses\oca.txt")
Get-ChildItem (Join-Path $RepoRoot "crates\core\assets\fonts") -Filter OFL.txt -Recurse | ForEach-Object {
    $family = $_.Directory.Name
    Copy-Item $_.FullName (Join-Path $Resources "licenses\fonts\$family-OFL.txt")
}
$env:PYTHONDONTWRITEBYTECODE = "1"
@(
    & (Join-Path $FfmpegDir "bin\ffmpeg.exe") -version | Select-Object -First 1
    & (Join-Path $RepoRoot "vendor\python-runtime\python.exe") --version 2>&1
    & (Join-Path $OutputDir "deno.exe") --version | Select-Object -First 1
    & (Join-Path $RepoRoot "vendor\python-runtime\python.exe") -c `
        "import importlib.metadata, yt_dlp.version; print('yt-dlp ' + yt_dlp.version.__version__ + '; yt-dlp-ejs ' + importlib.metadata.version('yt-dlp-ejs'))"
) | Set-Content (Join-Path $Resources "RUNTIME_VERSIONS.txt")
python (Join-Path $PSScriptRoot "generate_dependency_inventory.py") `
    (Join-Path $Resources "DEPENDENCIES.json") --bundle-root $OutputDir
python (Join-Path $PSScriptRoot "validate_bundle.py") $OutputDir --platform windows
