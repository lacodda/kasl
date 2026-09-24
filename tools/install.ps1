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
$defaultDir = Join-Path $env:LOCALAPPDATA "Programs\kasl"

# Upgrade in place. A kasl already on PATH is the one the user runs, the one
# autostart names and the one self-update replaces; installing a second copy
# next to it leaves the old one answering and the old watcher running. Found
# in the field: two copies, each started at login, each toasting.
$copies = @(Get-Command kasl -CommandType Application -All -ErrorAction SilentlyContinue |
    ForEach-Object { $_.Source } | Where-Object { $_ -like "*.exe" } | Select-Object -Unique)
$dir = if ($env:KASL_INSTALL_DIR) {
    $env:KASL_INSTALL_DIR
} elseif ($copies.Count -gt 0) {
    Split-Path $copies[0]
} else {
    $defaultDir
}
$target = Join-Path $dir "kasl.exe"

# A running watcher holds its binary open, so it is stopped before the copy -
# every watcher, from any copy: after this install exactly one is started
# again, from the binary just installed.
$watchers = @(Get-CimInstance Win32_Process -Filter "Name='kasl.exe' OR Name='ka.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -match '--daemon-run|\swatch(\s|$)' -and $_.CommandLine -notmatch '--stop' })
foreach ($watcher in $watchers) {
    Stop-Process -Id $watcher.ProcessId -Force -ErrorAction SilentlyContinue
}
if ($watchers.Count -gt 0) {
    Write-Host "Stopped $($watchers.Count) running watcher(s) for the upgrade"
    # The OS lets go of the binary a moment after the process ends.
    Start-Sleep -Milliseconds 500
}
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

# Add the directory to the user PATH in the registry, keeping the value's
# type. PATH is almost always REG_EXPAND_SZ, with entries like %JAVA_HOME%\bin
# stored unexpanded; the .NET environment API rewrites it as a plain string
# and silently breaks every such entry. So: read the raw value, compare
# case-insensitively without a trailing slash, write it back as an expandable
# string, and tell running shells about it. A PATH failure must not fail the
# install.
try {
    $key = Get-Item "HKCU:\Environment"
    $raw = [string]$key.GetValue("Path", "", "DoNotExpandEnvironmentNames")
    $entries = @($raw -split ";" | Where-Object { $_ })
    $wanted = $dir.TrimEnd("\")
    $present = $entries | Where-Object { $_.TrimEnd("\") -ieq $wanted }
    # A directory some copy already ran from is on PATH already, often the
    # machine PATH; a user entry would only duplicate it.
    if (-not $present -and -not ($copies -contains $target)) {
        $value = if ($entries.Count -gt 0) { ($entries + $wanted) -join ";" } else { $wanted }
        Set-ItemProperty -Path "HKCU:\Environment" -Name Path -Value $value -Type ExpandString
        if (-not ("KaslInstall.Env" -as [type])) {
            Add-Type -Namespace KaslInstall -Name Env -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr SendMessageTimeout(System.IntPtr hWnd, uint Msg, System.UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out System.UIntPtr lpdwResult);
'@
        }
        $result = [System.UIntPtr]::Zero
        # HWND_BROADCAST = 0xffff, WM_SETTINGCHANGE = 0x1A, SMTO_ABORTIFHUNG = 0x2
        [KaslInstall.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, "Environment", 0x2, 5000, [ref]$result) | Out-Null
        Write-Host "Added $dir to your user PATH - open a new terminal to pick it up."
    }
} catch {
    Write-Host "Note: could not update the user PATH ($($_.Exception.Message)); add $dir to it yourself."
}
Write-Host "Installed kasl $tag to $target"

# Other copies on PATH. One this installer put in its default directory
# before it knew to upgrade in place is its own leftover and goes; anything
# else was put there some other way (cargo, npm, by hand) and is only named.
foreach ($copy in $copies) {
    $copyDir = Split-Path $copy
    if ($copyDir.TrimEnd("\") -ieq $dir.TrimEnd("\")) { continue }
    if ($copyDir.TrimEnd("\") -ieq $defaultDir.TrimEnd("\")) {
        Remove-Item (Join-Path $copyDir "kasl.exe"), (Join-Path $copyDir "ka.exe") -Force -ErrorAction SilentlyContinue
        try {
            $key = Get-Item "HKCU:\Environment"
            $raw = [string]$key.GetValue("Path", "", "DoNotExpandEnvironmentNames")
            $kept = @($raw -split ";" | Where-Object { $_ -and $_.TrimEnd("\") -ine $copyDir.TrimEnd("\") })
            Set-ItemProperty -Path "HKCU:\Environment" -Name Path -Value ($kept -join ";") -Type ExpandString
        } catch {
            Write-Host "Note: remove $copyDir from your user PATH yourself."
        }
        Write-Host "Removed the second copy an earlier install left in $copyDir"
    } else {
        Write-Host "Note: another kasl is at $copy - remove it, or it may run instead of $target."
    }
}

# Autostart names a binary by its full path, so an entry written by another
# copy would start that copy at every login.
$wantedRun = "`"$target`" watch"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$run = (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).Kasl
if ($run -and $run -ne $wantedRun) {
    Set-ItemProperty -Path $runKey -Name Kasl -Value $wantedRun
    Write-Host "Autostart now starts $target"
}
# Through cmd: Windows PowerShell turns a native command's stderr into an
# error, and under "Stop" a missing task would end the install.
$task = cmd /c "schtasks /Query /TN KaslAutostart /XML 2>nul"
if ($LASTEXITCODE -eq 0 -and ($task -join "`n") -match "<Command>([^<]+)</Command>") {
    $taskExe = $Matches[1].Trim('"')
    if ($taskExe -ine $target) {
        # Changing a scheduled task can ask for a password, which a piped
        # install cannot answer; the fix is one command in an elevated shell.
        Write-Host "Note: the KaslAutostart task starts $taskExe - run 'kasl autostart enable' in an elevated terminal to point it here."
    }
}

if ($watchers.Count -gt 0) {
    & $target watch
}

# `init` is an interactive wizard, so it cannot run from here: this script is
# usually piped into iex, which leaves no terminal for prompts.
Write-Host "Next: run 'kasl setup' to set up monitoring, integrations and credentials."
