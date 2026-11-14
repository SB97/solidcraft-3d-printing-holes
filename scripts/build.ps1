# Builds the plugin package:
#   dist/printing-holes/plugin.json + plugin.wasm          (folder for plugin.inspect / plugin.run)
#   dist/printing-holes.zip                                 (for plugin.install / Preferences > Plugins)
# Not 'Stop': Windows PowerShell 5.1 turns a native tool's stderr (cargo's progress) into errors.
# Each native step checks $LASTEXITCODE instead.
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

cargo build --release --target wasm32-unknown-unknown
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$out = Join-Path $root 'dist\printing-holes'
New-Item -ItemType Directory -Force $out | Out-Null
$core = Join-Path $root 'target\wasm32-unknown-unknown\release\printing_holes.wasm'
$component = Join-Path $out 'plugin.wasm'

# Wrap the core module (with its embedded WIT metadata) as a component. No WASI adapter:
# the host links no WASI, so the component must import only craft:plugin interfaces.
wasm-tools component new $core -o $component
if ($LASTEXITCODE -ne 0) { throw "wasm-tools component new failed" }
wasm-tools validate $component
if ($LASTEXITCODE -ne 0) { throw "component does not validate" }

Copy-Item (Join-Path $root 'plugin\plugin.json') $out -Force

$zip = Join-Path $root 'dist\printing-holes.zip'
if (Test-Path $zip) { Remove-Item $zip -Force }
# Not Compress-Archive: Windows PowerShell 5.1 writes backslashes into entry names, which
# plugin.install refuses. Windows' own bsdtar writes a standard zip with forward slashes.
& "$env:SystemRoot\System32\tar.exe" -a -c -f $zip -C (Join-Path $root 'dist') 'printing-holes'
if ($LASTEXITCODE -ne 0) { throw "zip failed" }

"Imports of the component:"
wasm-tools component wit $component | Select-String -Pattern '^\s*(import|export)'
"Built $component ($((Get-Item $component).Length) bytes) and $zip"
