$ErrorActionPreference = "Stop"

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    throw "Python 3.11 or 3.12 is required. Install it, reopen PowerShell, and rerun this script."
}

try {
    $version = & python -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')"
} catch {
    throw "The Windows Store Python alias is present but Python is not installed. Install Python 3.11 or 3.12 first."
}
if ($LASTEXITCODE -ne 0) {
    throw "Python could not start. Install Python 3.11 or 3.12 first."
}

Write-Host "Using Python $version"
python -m venv .venv
& .\.venv\Scripts\python.exe -m pip install --upgrade pip
& .\.venv\Scripts\python.exe -m pip install -e ".[dev]"

if (-not (Test-Path .env)) {
    Copy-Item .env.example .env
}

Write-Host "MonkeyForge is ready. Run: .venv\Scripts\python.exe -m uvicorn monkeyforge.api:app --reload"
