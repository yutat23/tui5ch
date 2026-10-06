param(
    [switch]$Demo,
    [switch]$Offline,
    [string]$Url,
    [string]$DataDir
)
$ErrorActionPreference = 'Stop'
$candidates = @(
    (Join-Path $PSScriptRoot 'target/release/tui5ch.exe'),
    (Join-Path $PSScriptRoot 'dist/tui5ch.exe')
)
$build = $candidates | Where-Object { Test-Path -LiteralPath $_ } |
    ForEach-Object { Get-Item -LiteralPath $_ } |
    Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
if ($null -eq $build) {
    throw 'cargo build --release --locked でビルドしてから起動してください。'
}
$executable = $build.FullName
$appArguments = @()
if ($Demo) { $appArguments += '--demo' }
if ($Offline) { $appArguments += '--offline' }
if ($DataDir) { $appArguments += @('--data-dir', $DataDir) }
if ($Url) { $appArguments += $Url }
& $executable @appArguments
exit $LASTEXITCODE
