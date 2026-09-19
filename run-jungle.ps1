param([switch]$Capture)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path -LiteralPath $cargoPath)) {
    throw 'Install the Rust toolchain, then run this script again.'
}
$previousCapture = $env:CANOPY_CAPTURE
try {
    if ($Capture) { $env:CANOPY_CAPTURE = Join-Path $PSScriptRoot 'docs\jungle-preview.png' }
    & $cargoPath run --bin cube-soccer
    if ($LASTEXITCODE -ne 0) { throw "The game exited with code $LASTEXITCODE." }
} finally {
    $env:CANOPY_CAPTURE = $previousCapture
}
