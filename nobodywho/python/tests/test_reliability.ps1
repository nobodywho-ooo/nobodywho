[CmdletBinding(PositionalBinding=$false)]
param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$ScriptPath,
    [int]$Iterations = 100,
    [double]$Timeout = 120,
    [string]$LogDir = "smoke-test-logs",
    [Parameter(Position=1, ValueFromRemainingArguments=$true)]
    [string[]]$ScriptArgs = @()
)

$RunnerPath = Join-Path $PSScriptRoot "reliability_runner.py"
& python $RunnerPath --iterations $Iterations --timeout $Timeout --log-dir $LogDir $ScriptPath @ScriptArgs
exit $LASTEXITCODE
