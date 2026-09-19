[CmdletBinding()]
param(
    [string]$OutputRoot = "validation-logs"
)

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"

if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    $LogRoot = $OutputRoot
} else {
    $LogRoot = Join-Path $RepoRoot $OutputRoot
}

$RunDir = Join-Path $LogRoot $Timestamp
New-Item -ItemType Directory -Path $RunDir -Force | Out-Null

$SummaryPath = Join-Path $RunDir "validation-summary.log"
$CheckPath = Join-Path $RunDir "cargo-check.log"
$ClippyPath = Join-Path $RunDir "cargo-clippy.log"
$TestPath = Join-Path $RunDir "cargo-test.log"

$Results = @()

function Write-SummaryLine {
    param([string]$Text = "")
    $Text | Tee-Object -FilePath $SummaryPath -Append
}

function Invoke-ValidationStep {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string[]]$CargoArgs,

        [Parameter(Mandatory = $true)]
        [string]$LogPath
    )

    $CommandText = "cargo " + ($CargoArgs -join " ")
    $Started = Get-Date

    Write-Host ""
    Write-Host "============================================================"
    Write-Host $Name
    Write-Host $CommandText
    Write-Host "Log: $LogPath"
    Write-Host "============================================================"

    @(
        "============================================================"
        "$Name"
        "Command: $CommandText"
        "Started: $($Started.ToString("o"))"
        "============================================================"
    ) | Set-Content -Path $LogPath -Encoding utf8

    & cargo @CargoArgs 2>&1 | Tee-Object -FilePath $LogPath -Append

    $ExitCode = $LASTEXITCODE
    $Finished = Get-Date
    $Duration = $Finished - $Started
    $Status = if ($ExitCode -eq 0) { "PASS" } else { "FAIL" }

    @(
        ""
        "============================================================"
        "Result: $Status"
        "Exit code: $ExitCode"
        "Finished: $($Finished.ToString("o"))"
        "Duration: $($Duration.ToString())"
        "============================================================"
    ) | Add-Content -Path $LogPath -Encoding utf8

    Write-SummaryLine "$Name : $Status (exit $ExitCode, duration $($Duration.ToString()))"
    Write-SummaryLine "  Log: $LogPath"

    $script:Results += [PSCustomObject]@{
        Name = $Name
        Status = $Status
        ExitCode = $ExitCode
        Log = $LogPath
        Duration = $Duration
    }
}

Push-Location $RepoRoot
try {
    $StartedAt = Get-Date

    "" | Set-Content -Path $SummaryPath -Encoding utf8
    Write-SummaryLine "DragonForge Asset Vault - Validation Run"
    Write-SummaryLine "========================================"
    Write-SummaryLine "Started: $($StartedAt.ToString("o"))"
    Write-SummaryLine "Repository: $RepoRoot"
    Write-SummaryLine "Log directory: $RunDir"

    $GitCommit = (& git rev-parse HEAD 2>$null)
    if ($LASTEXITCODE -eq 0 -and $GitCommit) {
        Write-SummaryLine "Git commit: $GitCommit"
    } else {
        Write-SummaryLine "Git commit: unavailable"
    }

    $GitBranch = (& git branch --show-current 2>$null)
    if ($LASTEXITCODE -eq 0 -and $GitBranch) {
        Write-SummaryLine "Git branch: $GitBranch"
    }

    $CargoVersion = (& cargo --version 2>$null)
    if ($LASTEXITCODE -eq 0 -and $CargoVersion) {
        Write-SummaryLine "Cargo: $CargoVersion"
    }

    $RustcVersion = (& rustc --version 2>$null)
    if ($LASTEXITCODE -eq 0 -and $RustcVersion) {
        Write-SummaryLine "Rustc: $RustcVersion"
    }

    Write-SummaryLine ""

    Invoke-ValidationStep -Name "cargo check --all-targets" -CargoArgs @("check", "--all-targets") -LogPath $CheckPath
    Invoke-ValidationStep -Name "cargo clippy --all-targets" -CargoArgs @("clippy", "--all-targets") -LogPath $ClippyPath
    Invoke-ValidationStep -Name "cargo test --all-targets" -CargoArgs @("test", "--all-targets") -LogPath $TestPath

    $FinishedAt = Get-Date
    $TotalDuration = $FinishedAt - $StartedAt
    $Failed = @($Results | Where-Object { $_.ExitCode -ne 0 })

    Write-SummaryLine ""
    Write-SummaryLine "========================================"
    Write-SummaryLine "Final Summary"
    Write-SummaryLine "========================================"
    Write-SummaryLine "Finished: $($FinishedAt.ToString("o"))"
    Write-SummaryLine "Total duration: $($TotalDuration.ToString())"
    Write-SummaryLine "Passed: $(@($Results | Where-Object { $_.ExitCode -eq 0 }).Count)"
    Write-SummaryLine "Failed: $($Failed.Count)"
    Write-SummaryLine ""

    if ($Failed.Count -eq 0) {
        Write-SummaryLine "OVERALL RESULT: PASS"
        Write-Host ""
        Write-Host "Validation completed successfully."
        Write-Host "Upload this folder if you want the full validation results:"
        Write-Host "  $RunDir"
        exit 0
    }

    Write-SummaryLine "OVERALL RESULT: FAIL"
    Write-SummaryLine "Failed commands:"
    foreach ($Failure in $Failed) {
        Write-SummaryLine "  - $($Failure.Name) (exit $($Failure.ExitCode))"
    }

    Write-Host ""
    Write-Host "Validation completed with failures."
    Write-Host "Upload this folder, or at minimum validation-summary.log plus the failed command logs:"
    Write-Host "  $RunDir"
    exit 1
}
finally {
    Pop-Location
}
