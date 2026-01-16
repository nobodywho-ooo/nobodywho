param(
    [Parameter(Mandatory=$true)]
    [string]$ScriptPath,
    
    [Parameter()]
    [int]$Iterations = 100,
    
    [Parameter(ValueFromRemainingArguments=$true)]
    [string[]]$ScriptArgs = @()
)

# PowerShell script to run a Python script N times and count non-zero exit codes
# Usage: .\test_reliability.ps1 [-Iterations N] path\to\script.py [args...]

# Validate iterations
if ($Iterations -le 0) {
    Write-Host "Error: Iterations must be a positive integer, got '$Iterations'" -ForegroundColor Red
    exit 1
}

# Check if script exists
if (-not (Test-Path $ScriptPath)) {
    Write-Host "Error: Script '$ScriptPath' not found" -ForegroundColor Red
    exit 1
}

# Initialize counters
$TotalRuns = $Iterations
$SuccessCount = 0
$FailureCount = 0
$Failures = @()

Write-Host "Running '$ScriptPath' $TotalRuns times..." -ForegroundColor Green
if ($ScriptArgs.Count -gt 0) {
    Write-Host "Arguments: $($ScriptArgs -join ' ')" -ForegroundColor Yellow
}
Write-Host ""

# Function to show progress bar
function Show-Progress {
    param(
        [int]$Current,
        [int]$Total
    )
    $Percentage = [math]::Round(($Current / $Total) * 100)
    $Width = 50
    $Filled = [math]::Round(($Current / $Total) * $Width)
    
    $ProgressBar = "[" + ("=" * $Filled) + ("-" * ($Width - $Filled)) + "]"
    Write-Host "`rProgress: $ProgressBar $Percentage% ($Current/$Total)" -NoNewline
}

# Run the script 100 times
for ($i = 1; $i -le $TotalRuns; $i++) {
    Show-Progress -Current $i -Total $TotalRuns
    
    # Run the Python script and capture both output and exit code
    $ArgumentList = @($ScriptPath) + $ScriptArgs
    try {
        $Output = & python @ArgumentList 2>&1 | Out-String
        $ExitCode = $LASTEXITCODE
    } catch {
        $Output = $_.Exception.Message
        $ExitCode = 1
    }

    if ($ExitCode -eq 0) {
        $SuccessCount++
    } else {
        $FailureCount++
        $ErrorInfo = @{
            Run = $i
            ExitCode = $ExitCode
            Output = $Output.Trim()
        }
        $Failures += $ErrorInfo
    }
}

Write-Host ""
Write-Host ""
Write-Host "=== RESULTS ===" -ForegroundColor Cyan
Write-Host "Total runs: $TotalRuns"
Write-Host "Successes: $SuccessCount" -ForegroundColor Green
Write-Host "Failures: $FailureCount" -ForegroundColor Red
Write-Host "Success rate: $([math]::Round(($SuccessCount / $TotalRuns) * 100))%"
Write-Host "Failure rate: $([math]::Round(($FailureCount / $TotalRuns) * 100))%"

if ($FailureCount -gt 0) {
    Write-Host ""
    Write-Host "=== FAILURE DETAILS ===" -ForegroundColor Red
    foreach ($failure in $Failures) {
        Write-Host ""
        Write-Host "Run $($failure.Run): exit code $($failure.ExitCode)" -ForegroundColor Red
        if ($failure.Output) {
            Write-Host "Output:" -ForegroundColor Yellow
            Write-Host $failure.Output -ForegroundColor DarkGray
        }
    }
}

# Exit with failure count as exit code (capped at 255)
$ExitCode = if ($FailureCount -gt 255) { 255 } else { $FailureCount }
exit $ExitCode