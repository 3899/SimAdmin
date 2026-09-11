# SimAdmin Universal Startup Benchmark Wrapper (PowerShell)
param(
    [string]$Url = "http://192.168.68.1:3000",
    [switch]$Reboot,
    [switch]$Now,
    [string]$CompareA = "",
    [string]$CompareB = ""
)

$scriptPath = Join-Path $PSScriptRoot "benchmark-startup.mjs"

if ($CompareA -and $CompareB) {
    node $scriptPath --compare $CompareA $CompareB
    exit $LASTEXITCODE
}

$argsList = @($Url)
if ($Reboot) { $argsList += "--reboot" }
if ($Now) { $argsList += "--now" }

node $scriptPath @argsList
