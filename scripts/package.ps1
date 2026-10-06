param(
    [string]$OutputDirectory = 'dist'
)
$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$packageOutput = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $OutputDirectory))
Push-Location $repositoryRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Releaseビルドに失敗しました。' }
    $hostInfo = rustc -vV
    if ($LASTEXITCODE -ne 0) { throw 'Rustツールチェーンを確認できません。' }
    $hostTarget = ($hostInfo | Where-Object { $_ -like 'host: *' }) -replace '^host: ', ''
    if ($hostTarget -notmatch '^[a-zA-Z0-9_-]+-windows-[a-zA-Z0-9_-]+$') {
        throw 'このスクリプトはWindowsホスト向けです。'
    }
    $manifest = Get-Content -LiteralPath (Join-Path $repositoryRoot 'Cargo.toml') -Raw
    $packageVersion = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"\r\n]+)"').Groups[1].Value
    if ($packageVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][a-zA-Z0-9.-]+)?$') {
        throw 'パッケージのバージョンを読み取れません。'
    }
    New-Item -ItemType Directory -Force -Path $packageOutput | Out-Null
    # Keep an explicit allowlist and preserve documentation paths in the ZIP.
    $entries = @(
        @('target/release/tui5ch.exe', 'tui5ch.exe'),
        @('README.md', 'README.md'),
        @('LICENSE', 'LICENSE'),
        @('CONTRIBUTING.md', 'CONTRIBUTING.md'),
        @('CHANGELOG.md', 'CHANGELOG.md'),
        @('docs/usage.md', 'docs/usage.md'),
        @('docs/configuration.md', 'docs/configuration.md'),
        @('docs/demo-screen.txt', 'docs/demo-screen.txt')
    )
    $archive = Join-Path $packageOutput "tui5ch-$packageVersion-$hostTarget.zip"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archiveStream = [System.IO.File]::Open($archive, [System.IO.FileMode]::Create)
    $zip = [System.IO.Compression.ZipArchive]::new($archiveStream, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($entry in $entries) {
            $sourceFile = Join-Path $repositoryRoot $entry[0]
            [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $zip, $sourceFile, $entry[1], [System.IO.Compression.CompressionLevel]::Optimal
            ) | Out-Null
        }
    } finally {
        $zip.Dispose()
        $archiveStream.Dispose()
    }
    Write-Output $archive
} finally {
    Pop-Location
}
