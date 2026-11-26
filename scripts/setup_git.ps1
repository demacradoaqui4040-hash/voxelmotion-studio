# Configure local Git conventions, hooks, and commit template for VoxelMotion Studio
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $repoRoot
try {
    $hooksDir = Join-Path $repoRoot ".git\hooks"
    New-Item -ItemType Directory -Force -Path $hooksDir | Out-Null
    Copy-Item -Force (Join-Path $repoRoot "scripts\hooks\commit-msg") (Join-Path $hooksDir "commit-msg")
    git config --local commit.template ".gitmessage"
    Write-Host "VoxelMotion Studio Git conventions configured (.gitmessage + commit-msg hook)."
}
finally {
    Pop-Location
}
