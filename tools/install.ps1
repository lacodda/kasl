# kasl installer for Windows:
#   irm https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.ps1 | iex
$ErrorActionPreference = "Stop"

$repo = "lacodda/kasl"

# The tag comes from the /releases/latest redirect rather than the REST API:
# unauthenticated API calls are capped at 60 per hour per IP, and an installer
# that fails because someone else on the same address ran it is no installer.
# $env:KASL_VERSION pins a specific release.
$tag = $env:KASL_VERSION
if (-not $tag) {
    $request = [Net.HttpWebRequest]::Create("https://github.com/$repo/releases/latest")
    $request.AllowAutoRedirect = $false
    $request.UserAgent = "kasl-installer"
    try {
        $response = $request.GetResponse()
        $tag = ($response.Headers["Location"] -split "/")[-1]
        $response.Close()
    } catch {
        throw "Cannot resolve the latest release of ${repo}: $($_.Exception.Message)"
    }
}
if (-not $tag -or $tag -notmatch '^v\d') {
    throw "Cannot resolve the latest release of $repo - set `$env:KASL_VERSION to a release tag (vX.Y.Z)"
}

$name = "kasl-$tag-x86_64-pc-windows-msvc"
$url = "https://github.com/$repo/releases/download/$tag/$name.tar.gz"
$dir = if ($env:KASL_INSTALL_DIR) { $env:KASL_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\kasl" }
$tmp = Join-Path ([IO.Path]::GetTempPath()) "kasl-install-$([guid]::NewGuid())"
New-Item -ItemType Directory -Force $tmp | Out-Null

try {
    Write-Host "Downloading $url"
    $archive = Join-Path $tmp "kasl.tar.gz"
    Invoke-WebRequest $url -OutFile $archive
    # Windows ships bsdtar since 10 1803. Call it by full path: a Git Bash
    # install puts GNU tar first in PATH, and that one chokes on `C:\` paths.
    $tar = Join-Path $env:SystemRoot "System32\tar.exe"
    if (-not (Test-Path $tar)) { $tar = "tar" }
    & $tar -xzf $archive -C $tmp
    if ($LASTEXITCODE -ne 0) { throw "Cannot unpack $archive" }
    $binary = Get-ChildItem -Path $tmp -Filter "kasl.exe" -Recurse | Select-Object -First 1
    if (-not $binary) { throw "The archive did not contain kasl.exe" }
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item $binary.FullName $dir -Force

    # Short alias `ka` as a hard link, not a copy: a copy doubles the install
    # for no new code and goes stale the moment self-update replaces the
    # binary. A symlink would need elevation on Windows; a hard link does not,
    # as long as both names are on one volume - and they are, since the alias
    # lands beside the binary. Skipped when another `ka` already answers in
    # PATH; $env:KASL_NO_ALIAS=1 opts out.
    if (-not $env:KASL_NO_ALIAS) {
        $alias = Join-Path $dir "ka.exe"
        $existing = Get-Command ka -ErrorAction SilentlyContinue
        if (-not $existing -or $existing.Source -eq $alias) {
            # A link cannot be created over an existing name, and earlier
            # installs left a full second binary sitting there.
            Remove-Item $alias -Force -ErrorAction SilentlyContinue
            try {
                New-Item -ItemType HardLink -Path $alias -Target (Join-Path $dir "kasl.exe") -ErrorAction Stop | Out-Null
                Write-Host "Alias ka -> kasl"
            } catch {
                # A different volume, or a filesystem without hard links: a
                # copy still works, it just has to be refreshed by the
                # installer.
                Copy-Item (Join-Path $dir "kasl.exe") $alias -Force
                Write-Host "Alias ka -> kasl (copied - this filesystem has no hard links)"
            }
        } else {
            Write-Host "Note: 'ka' already resolves to $($existing.Source) - alias skipped."
        }
    }
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# Binaries left behind by updates: the replaced executable is kept as `.bak`
# and nothing ever came back for it.
Remove-Item (Join-Path $dir "kasl.bak") -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $dir "ka.bak") -Force -ErrorAction SilentlyContinue

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $dir) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
    Write-Host "Added $dir to your user PATH - restart the terminal to pick it up."
}
Write-Host "Installed kasl $tag to $dir\kasl.exe"

# `init` is an interactive wizard, so it cannot run from here: this script is
# usually piped into iex, which leaves no terminal for prompts.
Write-Host "Next: run 'kasl setup' to set up monitoring, integrations and credentials."
