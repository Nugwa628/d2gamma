# node-ctl.ps1: runs a D2RUST game server (d2rust-node.exe) on Windows, hidden, and swaps it to a new
# version during an upgrade. Windows PowerShell 5.1 or later; nothing to install.
#
#   node-ctl.ps1 setup [ID]       once, from an elevated PowerShell: the folders, the current version (the
#                                 folder's d2rust-node.exe), the firewall rule for its port and the scheduled
#                                 task "D2RUST game server <ID>" (ID: default the folder's name)
#   node-ctl.ps1 start | stop | restart | status
#   node-ctl.ps1 fetch <version>  download and check that version's signed package into downloads\ (the game
#                                 server's updater runs it during an upgrade's countdown)
#   node-ctl.ps1 rollback         switch back to the version before the last swap, and restart
#   node-ctl.ps1 supervise        internal: the task's restart loop
#   node-ctl.ps1 selftest         a fake game server and fake signed releases in a temp folder
#
# The folder is the one this script is in ($env:D2RUST_NODE_HOME overrides it):
#
#   node-ctl.ps1   d2rust-node.cfg   its bundle and identity   release-signers   release.token (optional)
#   bin\d2rust-node-<version>.exe    current.txt (the one that runs)   previous.txt (the one before)
#   downloads\<version>\             a fetched, checked package until it is swapped in
#   logs\  status.json (the health check), console.log, supervise.log
#   run\   supervisor.pid, node.pid, swap, trial, stop, stopped
#
# The task runs as the user who ran setup, "whether logged on or not": it starts at boot, runs in no
# desktop session and can never show a window. Its action is `supervise`, which starts d2rust-node.exe
# with no window and restarts it when it exits. With `update_command = powershell.exe -NoProfile
# -NonInteractive -ExecutionPolicy Bypass -File node-ctl.ps1` in d2rust-node.cfg, an upgrade's countdown
# runs `fetch V`; at zero the game server saves its games, writes run\swap and exits 75, and the
# supervisor swaps in the staged version on trial: one that doesn't report healthy (logs\status.json
# with its version) in 45 s is stopped and rolled back. `stop` writes run\stop, which the game server
# watches (it has no console for a Ctrl+Break): every game saves and it exits.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version 2

$Self = $PSCommandPath
if ($env:D2RUST_NODE_HOME) { $NodeHome = $env:D2RUST_NODE_HOME } else { $NodeHome = $PSScriptRoot }
$Program = 'd2rust-node'
$Cfg = Join-Path $NodeHome "$Program.cfg"
$Bin = Join-Path $NodeHome 'bin'
$Run = Join-Path $NodeHome 'run'
$Downloads = Join-Path $NodeHome 'downloads'
$CurrentFile = Join-Path $NodeHome 'current.txt'
$PreviousFile = Join-Path $NodeHome 'previous.txt'
$IdFile = Join-Path $NodeHome 'node-id.txt'
$Signers = Join-Path $NodeHome 'release-signers'
$TokenFile = Join-Path $NodeHome 'release.token'
$SwapExit = 75
$StartWait = 45
if ($env:D2RUST_SERVER_START_WAIT) { $StartWait = [int]$env:D2RUST_SERVER_START_WAIT }
$StopWait = 60
if ($env:D2RUST_NODE_STOP_WAIT) { $StopWait = [int]$env:D2RUST_NODE_STOP_WAIT }
# The release download base: $env:D2RUST_RELEASE_BASE, else release-base.txt in the node folder.
# There is no default; fetching stops until one is set.
$ReleaseBase = ''
if ($env:D2RUST_RELEASE_BASE) { $ReleaseBase = $env:D2RUST_RELEASE_BASE }
$ReleaseTag = 'server-'
$ReleaseNamespace = 'd2rust-release'

function Say([string]$m) { Write-Output $m }
function Die([string]$m) { [Console]::Error.WriteLine("node-ctl: $m"); exit 1 }
function Stamp { (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ') }
function UnixNow { [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() }
function Read-Text([string]$p) {
    if (Test-Path -LiteralPath $p) { return ([IO.File]::ReadAllText($p)).Trim() }
    return ''
}
function Write-Text([string]$p, [string]$s) { [IO.File]::WriteAllText($p, "$s`n", [Text.Encoding]::ASCII) }
function Remove-IfThere([string]$p) {
    if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Recurse -Force }
}
function Test-Version([string]$v) { return $v -match '^[0-9][0-9.]*$' }

# A value of d2rust-node.cfg (`key = value  # comment`), or $default.
function Get-CfgValue([string]$key, [string]$default) {
    if (Test-Path -LiteralPath $Cfg) {
        foreach ($line in [IO.File]::ReadAllLines($Cfg)) {
            $l = ($line -replace '#.*$', '').Trim()
            if ($l -match "^$key\s*=\s*(.+)$") { return $Matches[1].Trim() }
        }
    }
    return $default
}
$Logs = Get-CfgValue 'logs' 'logs'
if (-not [IO.Path]::IsPathRooted($Logs)) { $Logs = Join-Path $NodeHome $Logs }
$StatusJson = Join-Path $Logs 'status.json'
$ConsoleLog = Join-Path $Logs 'console.log'
$SuperviseLog = Join-Path $Logs 'supervise.log'

function Get-NodeId {
    $id = Read-Text $IdFile
    if ($id) { return $id }
    return (Split-Path -Leaf $NodeHome)
}
function Get-TaskName { return "D2RUST game server $(Get-NodeId)" }
function Get-Task {
    return Get-ScheduledTask -TaskName (Get-TaskName) -ErrorAction SilentlyContinue
}

# A running process of a pid file, or $null.
function Get-PidProcess([string]$file) {
    $id = Read-Text $file
    if (-not $id) { return $null }
    return Get-Process -Id ([int]$id) -ErrorAction SilentlyContinue
}

# Runs a program with no window, its standard input from $stdin (bytes, exactly), and gives back its exit
# code and output. Arguments are one string, quoted by the caller.
function Invoke-Hidden([string]$file, [string]$arguments, [byte[]]$stdin) {
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $file
    $psi.Arguments = $arguments
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $p = [Diagnostics.Process]::Start($psi)
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    if ($stdin) { $p.StandardInput.BaseStream.Write($stdin, 0, $stdin.Length) }
    $p.StandardInput.Close()
    $p.WaitForExit()
    return @{ Code = $p.ExitCode; Out = $out.Result; Err = $err.Result }
}

function Get-SshKeygen {
    $k = Join-Path $env:SystemRoot 'System32\OpenSSH\ssh-keygen.exe'
    if (Test-Path -LiteralPath $k) { return $k }
    $c = Get-Command ssh-keygen.exe -ErrorAction SilentlyContinue
    if ($c) { return $c.Source }
    Die "fetch needs ssh-keygen (Windows' OpenSSH client: Settings > System > Optional features) to check the signature"
}

# The version an exe says it is (`d2rust-node.exe --version`: `d2rust-node V`).
function Get-ExeVersion([string]$exe) {
    $r = Invoke-Hidden $exe '--version' $null
    if ($r.Code -ne 0) { return '' }
    $w = ($r.Out.Trim() -split '\s+')
    if ($w.Count -ge 2) { return $w[1] }
    return ''
}

function Get-Current { return Read-Text $CurrentFile }
function Get-CurrentExe {
    $c = Get-Current
    if (-not $c) { return '' }
    return Join-Path $Bin $c
}
function Get-VersionOfName([string]$name) { return ($name -replace "^$Program-", '' -replace '\.exe$', '') }

# Whether status.json names version $want and was written at or after $since (Unix seconds); waits up to
# $wait s.
function Test-Healthy([string]$want, [long]$since, [int]$wait) {
    $until = (UnixNow) + $wait
    while ($true) {
        if (Test-Path -LiteralPath $StatusJson) {
            $t = ([DateTimeOffset](Get-Item -LiteralPath $StatusJson).LastWriteTimeUtc).ToUnixTimeSeconds()
            if ($t -ge $since) {
                try {
                    $j = [IO.File]::ReadAllText($StatusJson) | ConvertFrom-Json
                    if ($j.version -eq $want) { return $true }
                } catch { }
            }
        }
        if ((UnixNow) -ge $until) { return $false }
        Start-Sleep -Milliseconds 500
    }
}

# --- fetch -------------------------------------------------------------------------------------------

# Downloads file $name of release $ver to $dest: from the release downloads (with release.token when there is one),
# or, for a test, from a folder ($ReleaseBase\server-<ver>\<name>).
function Get-ReleaseFile([string]$ver, [string]$name, [string]$dest) {
    if (-not $ReleaseBase) { $ReleaseBase = Read-Text (Join-Path $NodeHome 'release-base.txt') }
    if (-not $ReleaseBase) { Die "no release download base: set D2RUST_RELEASE_BASE or write it into release-base.txt" }
    if ($ReleaseBase -match '^https?://') {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        $h = @{}
        $tok = Read-Text $TokenFile
        if ($tok) { $h['Authorization'] = "token $tok" }
        Invoke-WebRequest -UseBasicParsing -Uri "$ReleaseBase/$ReleaseTag$ver/$name" -OutFile $dest -Headers $h
    } else {
        Copy-Item -LiteralPath (Join-Path (Join-Path $ReleaseBase "$ReleaseTag$ver") $name) -Destination $dest
    }
}

# Downloads release server-<ver> into downloads\<ver> and checks it: the signature of SHA256SUMS.windows
# first (nothing of the package is read before it checks out), the zip's sum, its version.json. The
# checked package is downloads\<ver>\d2rust-node-<ver>; anything that fails leaves nothing behind.
function Invoke-Stage([string]$ver) {
    if (-not (Test-Path -LiteralPath $Signers) -or (Get-Item -LiteralPath $Signers).Length -eq 0) {
        Die "no $Signers`: the admin adds the release key once (tools/server/README.md)"
    }
    $keygen = Get-SshKeygen
    $name = "$Program-$ver"
    $zip = "$name-windows-x86_64.zip"
    $dl = Join-Path $Downloads $ver
    Remove-IfThere $dl
    New-Item -ItemType Directory -Force -Path $dl | Out-Null
    $refuse = { param($m) Remove-IfThere $dl; Die $m }
    foreach ($f in @('SHA256SUMS.windows', 'SHA256SUMS.windows.sig', $zip)) {
        try { Get-ReleaseFile $ver $f (Join-Path $dl $f) } catch {
            & $refuse "downloading $f of release $ReleaseTag$ver failed: $($_.Exception.Message)"
        }
    }
    # ssh-keygen reads release-signers as ASCII lines; one written by Windows PowerShell's `>` is UTF-16.
    $signersCopy = Join-Path $dl 'release-signers'
    [IO.File]::WriteAllText($signersCopy, ((Get-Content -LiteralPath $Signers) -join "`n") + "`n", [Text.Encoding]::ASCII)
    $sums = Join-Path $dl 'SHA256SUMS.windows'
    $sig = Join-Path $dl 'SHA256SUMS.windows.sig'
    $r = Invoke-Hidden $keygen "-Y verify -f `"$signersCopy`" -I $ReleaseNamespace -n $ReleaseNamespace -s `"$sig`"" ([IO.File]::ReadAllBytes($sums))
    if ($r.Code -ne 0) { & $refuse "the signature of SHA256SUMS.windows does not check out: nothing staged" }
    $lines = @([IO.File]::ReadAllLines($sums) | Where-Object {
        $w = $_ -split '\s+', 2
        $w.Count -eq 2 -and ($w[1] -eq $zip -or $w[1] -eq "*$zip")
    })
    if ($lines.Count -ne 1) { & $refuse "SHA256SUMS.windows does not list $zip once: nothing staged" }
    $want = ($lines[0] -split '\s+', 2)[0]
    $have = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $dl $zip)).Hash
    if ($have -ne $want.ToUpperInvariant()) { & $refuse "$zip does not match the signed SHA256SUMS.windows: nothing staged" }
    try { Expand-Archive -LiteralPath (Join-Path $dl $zip) -DestinationPath $dl -Force } catch {
        & $refuse "unpacking $zip failed: $($_.Exception.Message)"
    }
    $pkg = Join-Path $dl $name
    $pv = ''
    try { $pv = ([IO.File]::ReadAllText((Join-Path $pkg 'version.json')) | ConvertFrom-Json).version } catch { }
    if ($pv -ne $ver) { & $refuse "$zip is not version $ver`: nothing staged" }
    if (-not (Test-Path -LiteralPath (Join-Path $pkg "$Program.exe"))) { & $refuse "$zip has no $Program.exe: nothing staged" }
    Remove-Item -LiteralPath (Join-Path $dl $zip), $signersCopy -Force
}

function Invoke-Fetch([string]$ver) {
    if (-not (Test-Version $ver)) { Die 'usage: node-ctl.ps1 fetch <version>' }
    Invoke-Stage $ver
    Say "staged $ver"
}

# --- the swap ----------------------------------------------------------------------------------------

# Before every start: a version on trial that never became healthy is rolled back; a swap marker
# (run\swap, the version) swaps in that staged version, on trial.
function Invoke-PreStart {
    New-Item -ItemType Directory -Force -Path $Run, $Bin | Out-Null
    $trial = Join-Path $Run 'trial'
    $swap = Join-Path $Run 'swap'
    if (Test-Path -LiteralPath $trial) {
        $t = (Read-Text $trial) -split ' '
        $prev = ''
        if ($t.Count -ge 3) { $prev = $t[2] }
        if (Test-Healthy $t[0] ([long]$t[1]) 0) {
            Remove-IfThere $trial
        } else {
            $to = 'none'
            if ($prev) { $to = $prev }
            Note "$Program $($t[0]) never reported healthy; rolling back to $to"
            if ($prev -and (Test-Path -LiteralPath (Join-Path $Bin $prev))) { Write-Text $CurrentFile $prev }
            Remove-IfThere $trial
            Remove-IfThere $swap
            return
        }
    }
    if (-not (Test-Path -LiteralPath $swap)) { return }
    $v = Read-Text $swap
    Remove-IfThere $swap
    $staged = Join-Path $Downloads "$v\$Program-$v\$Program.exe"
    if (-not (Test-Version $v) -or -not (Test-Path -LiteralPath $staged)) {
        Note "swap to $v`: nothing staged; staying on $(Get-Current)"
        return
    }
    $newName = "$Program-$v.exe"
    $dest = Join-Path $Bin $newName
    try {
        Copy-Item -LiteralPath $staged -Destination "$dest.tmp" -Force
        Move-Item -LiteralPath "$dest.tmp" -Destination $dest -Force
    } catch {
        Note "swap to $v`: copying $staged failed: $($_.Exception.Message)"
        return
    }
    $old = Get-Current
    if ($old -and $old -ne $newName) { Write-Text $PreviousFile $old }
    Write-Text $CurrentFile $newName
    Write-Text $trial "$v $(UnixNow) $old"
    Note "swapped to $Program $v (on trial for $StartWait s)"
}

# A version on trial that is healthy: this script and the config example come from its package, and
# old versions go.
function Complete-Trial([string]$v) {
    Remove-IfThere (Join-Path $Run 'trial')
    $pkg = Join-Path $Downloads "$v\$Program-$v"
    $script = Join-Path $pkg 'node-ctl.ps1'
    if (Test-Path -LiteralPath $script) {
        Copy-Item -LiteralPath $script -Destination "$Self.new" -Force
        Move-Item -LiteralPath "$Self.new" -Destination $Self -Force
    }
    Get-ChildItem -LiteralPath $pkg -Filter '*.cfg.example' -ErrorAction SilentlyContinue |
        ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $NodeHome -Force }
    Remove-IfThere (Join-Path $Downloads $v)
    $keep = @((Get-Current), (Read-Text $PreviousFile))
    Get-ChildItem -LiteralPath $Bin -Filter "$Program-*.exe" | Where-Object { $keep -notcontains $_.Name } |
        ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }
    Note "$Program $v is healthy"
}

# --- supervise ---------------------------------------------------------------------------------------

function Note([string]$m) {
    $line = "$(Stamp) $m"
    try { New-Item -ItemType Directory -Force -Path $Logs | Out-Null; Add-Content -LiteralPath $SuperviseLog -Value $line } catch { }
}

# The game server, started with no window; its output goes to logs\console.log. cmd.exe does the
# redirection and gives back its exit code.
function Start-Node([string]$exe) {
    New-Item -ItemType Directory -Force -Path $Logs | Out-Null
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $env:ComSpec
    $psi.Arguments = "/d /c `"`"$exe`" --config `"$Cfg`" >>`"$ConsoleLog`" 2>&1`""
    $psi.WorkingDirectory = $NodeHome
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    return [Diagnostics.Process]::Start($psi)
}

function Stop-Tree([int]$id) {
    & "$env:SystemRoot\System32\taskkill.exe" /PID $id /T /F 2>&1 | Out-Null
}

function Invoke-Supervise {
    New-Item -ItemType Directory -Force -Path $Run, $Logs | Out-Null
    try {
        $lock = [IO.File]::Open((Join-Path $Run 'supervisor.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    } catch { exit 0 }
    Write-Text (Join-Path $Run 'supervisor.pid') $PID
    $stopped = Join-Path $Run 'stopped'
    $stopMarker = Join-Path $Run 'stop'
    $nodePid = Join-Path $Run 'node.pid'
    $backoff = 1
    while (-not (Test-Path -LiteralPath $stopped)) {
        try { Invoke-PreStart } catch { Note "pre-start: $($_.Exception.Message)" }
        $exe = Get-CurrentExe
        if (-not $exe -or -not (Test-Path -LiteralPath $exe)) {
            Note "no current version (current.txt); nothing to start"
            break
        }
        Remove-IfThere $stopMarker
        if (Test-Path -LiteralPath $stopped) { break }
        $started = UnixNow
        $p = Start-Node $exe
        Write-Text $nodePid $p.Id
        $trialFile = Join-Path $Run 'trial'
        $trial = $null
        if (Test-Path -LiteralPath $trialFile) { $trial = (Read-Text $trialFile) -split ' ' }
        $stopAsked = 0
        while (-not $p.WaitForExit(1000)) {
            $now = UnixNow
            if ($trial) {
                if (Test-Healthy $trial[0] ([long]$trial[1]) 0) {
                    try { Complete-Trial $trial[0] } catch { Note "after the trial: $($_.Exception.Message)" }
                    $trial = $null
                } elseif ($now - $started -ge $StartWait -and -not $stopAsked) {
                    # Stopped, it is rolled back by the next pre-start.
                    Note "$Program $($trial[0]) did not report healthy in $StartWait s; stopping it"
                    Write-Text $stopMarker 'trial'
                    $stopAsked = $now
                }
            }
            if (-not $stopAsked -and (Test-Path -LiteralPath $stopped)) { $stopAsked = $now }
            if ($stopAsked -and $now - $stopAsked -ge $StopWait) {
                Note "$Program did not stop in $StopWait s; ending it"
                Stop-Tree $p.Id
                $stopAsked = $now
            }
        }
        $rc = $p.ExitCode
        Remove-IfThere $nodePid
        if (Test-Path -LiteralPath $stopped) { break }
        if ($rc -eq $SwapExit) {
            Note "$Program exited with $rc`: an upgrade is staged; starting again"
            $backoff = 1
            continue
        }
        Note "$Program exited with $rc; restarting in $backoff s"
        # A game server that ran ten minutes was healthy: start over with a short wait.
        if ((UnixNow) - $started -ge 600) { $backoff = 1 }
        Start-Sleep -Seconds $backoff
        $backoff = [Math]::Min($backoff * 2, 60)
    }
    Remove-IfThere (Join-Path $Run 'supervisor.pid')
    $lock.Close()
}

# --- the commands ------------------------------------------------------------------------------------

function Test-Admin {
    $me = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    return $me.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# The folder's d2rust-node.exe becomes bin\d2rust-node-<version>.exe and the current version (it is
# copied: a game server running from it now keeps running).
function Add-FolderExe {
    if (Get-Current) { return }
    $exe = Join-Path $NodeHome "$Program.exe"
    if (-not (Test-Path -LiteralPath $exe)) { Die "no $exe and no current.txt: put the game server's $Program.exe here first" }
    $v = Get-ExeVersion $exe
    if (-not (Test-Version $v)) { Die "$exe --version did not say its version" }
    New-Item -ItemType Directory -Force -Path $Bin | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $Bin "$Program-$v.exe") -Force
    Write-Text $CurrentFile "$Program-$v.exe"
    Say "current version: $v (bin\$Program-$v.exe)"
}

function Invoke-Setup([string]$id) {
    if (-not (Test-Admin)) { Die 'setup registers a task that starts at boot: run it once from an elevated PowerShell (Run as administrator)' }
    if (-not (Test-Path -LiteralPath $Cfg)) { Die "no $Cfg`: write it first (d2rust-node.cfg.example)" }
    if ($id) { Write-Text $IdFile $id }
    New-Item -ItemType Directory -Force -Path $Bin, $Run, $Downloads, $Logs | Out-Null
    Add-FolderExe
    # The rule is for the port, not the program: every version runs from its own bin\ file.
    $listen = Get-CfgValue 'listen' '0.0.0.0:6113'
    $port = ($listen -split ':')[-1]
    $rule = "D2RUST game server $(Get-NodeId) (UDP $port)"
    if (-not (Get-NetFirewallRule -DisplayName $rule -ErrorAction SilentlyContinue)) {
        New-NetFirewallRule -DisplayName $rule -Direction Inbound -Protocol UDP -LocalPort $port -Action Allow | Out-Null
        Say "firewall: $rule"
    }
    $ps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $action = New-ScheduledTaskAction -Execute $ps -WorkingDirectory $NodeHome `
        -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$Self`" supervise"
    $trigger = New-ScheduledTaskTrigger -AtStartup
    $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType S4U -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew -Hidden `
        -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1)
    Register-ScheduledTask -TaskName (Get-TaskName) -Action $action -Trigger $trigger -Principal $principal `
        -Settings $settings -Description "D2RUST game server in $NodeHome (node-ctl.ps1)" -Force | Out-Null
    Say "task: $(Get-TaskName) (at boot, whether logged on or not, hidden)"
    Say "next: stop a game server you started by hand from this folder, then: node-ctl.ps1 start"
}

function Invoke-Start {
    Remove-IfThere (Join-Path $Run 'stopped')
    if (Get-PidProcess (Join-Path $Run 'supervisor.pid')) { Say 'running'; return }
    if (-not (Get-Task)) { Die "no task $(Get-TaskName): run setup first" }
    Start-ScheduledTask -TaskName (Get-TaskName)
    Say 'started'
}

function Invoke-Stop {
    New-Item -ItemType Directory -Force -Path $Run | Out-Null
    Write-Text (Join-Path $Run 'stopped') (Stamp)
    $sup = Get-PidProcess (Join-Path $Run 'supervisor.pid')
    $node = Get-PidProcess (Join-Path $Run 'node.pid')
    if ($node) { Write-Text (Join-Path $Run 'stop') 'stop' }
    $until = (UnixNow) + $StopWait + 15
    while ((UnixNow) -lt $until) {
        $sup = Get-PidProcess (Join-Path $Run 'supervisor.pid')
        $node = Get-PidProcess (Join-Path $Run 'node.pid')
        if (-not $sup -and -not $node) { break }
        Start-Sleep -Milliseconds 500
    }
    if ($node) { Stop-Tree $node.Id; Remove-IfThere (Join-Path $Run 'node.pid') }
    if ($sup) { Stop-Process -Id $sup.Id -Force -ErrorAction SilentlyContinue; Remove-IfThere (Join-Path $Run 'supervisor.pid') }
    Remove-IfThere (Join-Path $Run 'stop')
    Say 'stopped'
}

function Invoke-Rollback {
    $prev = Read-Text $PreviousFile
    if (-not $prev) { Die 'no previous version' }
    if (-not (Test-Path -LiteralPath (Join-Path $Bin $prev))) { Die "previous version $prev is gone" }
    $cur = Get-Current
    Write-Text $CurrentFile $prev
    Write-Text $PreviousFile $cur
    Remove-IfThere (Join-Path $Run 'trial')
    # The supervisor starts the game server again, now as $prev.
    if (Get-PidProcess (Join-Path $Run 'node.pid')) { Write-Text (Join-Path $Run 'stop') 'rollback' }
    Say "switched back to $prev"
}

function Invoke-Status {
    $t = Get-Task
    if ($t) { Say "task:       $(Get-TaskName) ($($t.State))" } else { Say 'task:       none (run setup)' }
    $sup = Get-PidProcess (Join-Path $Run 'supervisor.pid')
    $node = Get-PidProcess (Join-Path $Run 'node.pid')
    $supId = 'none'
    if ($sup) { $supId = $sup.Id }
    if ($node) { Say "running:    yes (supervisor $supId, game server $($node.Id))" }
    elseif ($sup) { Say "running:    starting (supervisor $supId)" }
    elseif (Test-Path -LiteralPath (Join-Path $Run 'stopped')) { Say 'running:    no (stopped)' }
    else { Say 'running:    no' }
    Say "current:    $(Get-VersionOfName (Get-Current))"
    $prev = Read-Text $PreviousFile
    if ($prev) { Say "previous:   $(Get-VersionOfName $prev)" }
    if (Test-Path -LiteralPath (Join-Path $Run 'trial')) { Say "on trial:   $(((Read-Text (Join-Path $Run 'trial')) -split ' ')[0])" }
    if (Test-Path -LiteralPath $Downloads) {
        $staged = @(Get-ChildItem -LiteralPath $Downloads -Directory | ForEach-Object { $_.Name })
        if ($staged.Count) { Say "staged:     $($staged -join ' ')" }
    }
    if (Test-Path -LiteralPath $StatusJson) {
        $age = [int]((Get-Date).ToUniversalTime() - (Get-Item -LiteralPath $StatusJson).LastWriteTimeUtc).TotalSeconds
        try {
            $j = [IO.File]::ReadAllText($StatusJson) | ConvertFrom-Json
            Say "status:     version $($j.version), $($j.games) games, $($j.players) players ($age s ago)"
        } catch { Say "status:     unreadable ($age s ago)" }
    }
}

# --- selftest ----------------------------------------------------------------------------------------

function Invoke-Selftest {
    $ps = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
    $keygen = Get-SshKeygen
    $T = Join-Path ([IO.Path]::GetTempPath()) "node-ctl-selftest-$PID"
    $t0 = Get-Date
    Remove-IfThere $T
    $H = Join-Path $T 'node'
    $rel = Join-Path $T 'releases'
    $work = Join-Path $T 'work'
    New-Item -ItemType Directory -Force -Path $H, $rel, $work | Out-Null
    $script:fails = 0
    $script:checks = 0
    function Check([string]$what, [bool]$ok) {
        $script:checks++
        if ($ok) { Write-Output "ok - $what" } else { $script:fails++; Write-Output "FAIL - $what" }
    }
    function Wait-Until([scriptblock]$cond, [int]$secs) {
        $until = (Get-Date).AddSeconds($secs)
        while ((Get-Date) -lt $until) { if (& $cond) { return $true }; Start-Sleep -Milliseconds 250 }
        return [bool](& $cond)
    }
    # A fake d2rust-node.exe: says its version, writes status.json if healthy, stops on run\stop, and
    # on run\fake-swap (standing in for the main server's restart) writes run\swap and exits 75.
    function New-FakeExe([string]$v, [bool]$healthy, [string]$out) {
        $cls = 'Fake' + ($v -replace '\.', '_')
        $h = 'false'
        if ($healthy) { $h = 'true' }
        $src = @"
using System; using System.IO; using System.Threading;
public static class $cls {
    static bool Healthy = $h;
    public static int Main(string[] a) {
        if (a.Length > 0 && a[0] == "--version") { Console.WriteLine("d2rust-node $v"); return 0; }
        Directory.CreateDirectory("run"); Directory.CreateDirectory("logs");
        File.AppendAllText(Path.Combine("logs", "fake.log"), "$v started\n");
        while (true) {
            if (Healthy) {
                try { File.WriteAllText(Path.Combine("logs", "status.json"), "{\"version\":\"$v\",\"games\":0,\"players\":0}"); } catch (IOException) { }
            }
            string stop = Path.Combine("run", "stop");
            if (File.Exists(stop)) { File.Delete(stop); return 0; }
            string ask = Path.Combine("run", "fake-swap");
            if (File.Exists(ask)) {
                string to = File.ReadAllText(ask).Trim();
                File.Delete(ask);
                File.WriteAllText(Path.Combine("run", "swap"), to + "\n");
                return 75;
            }
            Thread.Sleep(300);
        }
    }
}
"@
        Add-Type -TypeDefinition $src -OutputAssembly $out -OutputType ConsoleApplication
    }
    $sign = { param($key, $file)
        Remove-IfThere "$file.sig"
        $r = Invoke-Hidden $keygen "-q -Y sign -f `"$key`" -n $ReleaseNamespace `"$file`"" $null
        if ($r.Code -ne 0) { throw "signing failed: $($r.Err)" }
    }
    # Release server-<v> in $rel: the zip (the fake exe, version.json, this script), its sums, signed.
    $release = { param($v, $healthy, $key)
        $pkg = Join-Path $work "$Program-$v"
        New-Item -ItemType Directory -Force -Path $pkg | Out-Null
        New-FakeExe $v $healthy (Join-Path $pkg "$Program.exe")
        [IO.File]::WriteAllText((Join-Path $pkg 'version.json'), "{`"version`":`"$v`",`"programs`":[`"$Program.exe`"]}`n")
        [IO.File]::WriteAllText((Join-Path $pkg 'node-ctl.ps1'), [IO.File]::ReadAllText($Self) + "`n# selftest package $v`n")
        $d = Join-Path $rel "$ReleaseTag$v"
        New-Item -ItemType Directory -Force -Path $d | Out-Null
        $zip = "$Program-$v-windows-x86_64.zip"
        Compress-Archive -LiteralPath $pkg -DestinationPath (Join-Path $d $zip) -Force
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $d $zip)).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText((Join-Path $d 'SHA256SUMS.windows'), "$hash  $zip`n")
        & $sign $key (Join-Path $d 'SHA256SUMS.windows')
    }
    $ctl = Join-Path $H 'node-ctl.ps1'
    $run = { param([string[]]$a)
        $q = ($a | ForEach-Object { "`"$_`"" }) -join ' '
        return Invoke-Hidden $ps "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$ctl`" $q" $null
    }
    $sup = $null
    $saved = @{ base = $env:D2RUST_RELEASE_BASE; wait = $env:D2RUST_SERVER_START_WAIT; home = $env:D2RUST_NODE_HOME }
    try {
        $env:D2RUST_RELEASE_BASE = $rel
        $env:D2RUST_SERVER_START_WAIT = '6'
        $env:D2RUST_NODE_HOME = ''
        Copy-Item -LiteralPath $Self -Destination $ctl
        [IO.File]::WriteAllText((Join-Path $H "$Program.cfg"), "logs = logs`n")
        $key = Join-Path $T 'relkey'
        $other = Join-Path $T 'otherkey'
        foreach ($k in @($key, $other)) {
            $r = Invoke-Hidden $keygen "-q -t ed25519 -N `"`" -C selftest -f `"$k`"" $null
            if ($r.Code -ne 0) { throw "ssh-keygen: $($r.Err)" }
        }
        # Written the way an admin would in Windows PowerShell (UTF-16): fetch copes.
        "$ReleaseNamespace $([IO.File]::ReadAllText("$key.pub").Trim())" | Out-File -LiteralPath (Join-Path $H 'release-signers') -Encoding Unicode
        New-FakeExe '1.0.0' $true (Join-Path $H "$Program.exe")
        & $release '2.0.0' $true $key
        & $release '3.0.0' $false $key
        & $release '2.0.1' $true $other
        & $release '2.0.2' $true $key
        # 2.0.2's zip changes after it was signed.
        Add-Content -LiteralPath (Join-Path $rel "$($ReleaseTag)2.0.2\$Program-2.0.2-windows-x86_64.zip") -Value 'x'

        # The folder's exe becomes the current version (setup's first step; setup itself needs an
        # elevated PowerShell and registers a real task, so it is not run here).
        & { $script:NodeHome = $H; $script:Bin = Join-Path $H 'bin'; $script:CurrentFile = Join-Path $H 'current.txt'; Add-FolderExe } | Out-Null
        Check 'the folder exe is adopted as 1.0.0' ((Read-Text (Join-Path $H 'current.txt')) -eq "$Program-1.0.0.exe" -and (Test-Path (Join-Path $H "bin\$Program-1.0.0.exe")))

        $r = & $run @('fetch', '2.0.1')
        Check 'fetch refuses a release signed by another key' ($r.Code -ne 0 -and $r.Err -match 'signature' -and -not (Test-Path (Join-Path $H 'downloads\2.0.1')))
        $r = & $run @('fetch', '2.0.2')
        Check 'fetch refuses a zip changed after signing' ($r.Code -ne 0 -and $r.Err -match 'does not match' -and -not (Test-Path (Join-Path $H 'downloads\2.0.2')))
        $r = & $run @('fetch', '9.9.9')
        Check 'fetch refuses a missing release' ($r.Code -ne 0 -and $r.Err -match 'downloading')
        $r = & $run @('fetch', '2.0.0')
        Check 'fetch stages a good release, nothing switched' ($r.Code -eq 0 -and $r.Out -match 'staged 2.0.0' -and (Test-Path (Join-Path $H "downloads\2.0.0\$Program-2.0.0\$Program.exe")) -and (Read-Text (Join-Path $H 'current.txt')) -eq "$Program-1.0.0.exe")
        $r = & $run @('fetch', '3.0.0')
        Check 'fetch stages the unhealthy one too' ($r.Code -eq 0)

        $psi = New-Object Diagnostics.ProcessStartInfo
        $psi.FileName = $ps
        $psi.Arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$ctl`" supervise"
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $sup = [Diagnostics.Process]::Start($psi)
        $status = Join-Path $H 'logs\status.json'
        $ver = { try { ([IO.File]::ReadAllText($status) | ConvertFrom-Json).version } catch { '' } }
        Check 'supervise starts 1.0.0' (Wait-Until { (& $ver) -eq '1.0.0' } 20)
        $r = Invoke-Hidden $ps "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$ctl`" supervise" $null
        Check 'a second supervisor leaves at once' ($r.Code -eq 0 -and -not $sup.HasExited)

        Write-Text (Join-Path $H 'run\fake-swap') '2.0.0'
        Check 'exit 75 swaps in the staged 2.0.0' (Wait-Until { (& $ver) -eq '2.0.0' } 20)
        Check 'its trial passes: current 2.0.0, previous 1.0.0, the package adopted' (Wait-Until {
            -not (Test-Path (Join-Path $H 'run\trial')) -and -not (Test-Path (Join-Path $H 'downloads\2.0.0')) -and
            (Read-Text (Join-Path $H 'current.txt')) -eq "$Program-2.0.0.exe" -and (Read-Text (Join-Path $H 'previous.txt')) -eq "$Program-1.0.0.exe" -and
            ([IO.File]::ReadAllText($ctl) -match 'selftest package 2.0.0') } 15)

        Write-Text (Join-Path $H 'run\fake-swap') '7.7.7'
        Start-Sleep -Seconds 2
        Check 'a swap with nothing staged keeps 2.0.0' (Wait-Until { (& $ver) -eq '2.0.0' -and (Read-Text (Join-Path $H 'current.txt')) -eq "$Program-2.0.0.exe" -and (Get-Content (Join-Path $H 'logs\supervise.log') -Raw) -match 'swap to 7.7.7: nothing staged' } 15)

        $r = & $run @('rollback')
        Check 'rollback switches to 1.0.0 and the supervisor restarts it' ($r.Code -eq 0 -and (Wait-Until { (& $ver) -eq '1.0.0' } 20))
        $r = & $run @('rollback')
        Check 'rollback again: 2.0.0' ($r.Code -eq 0 -and (Wait-Until { (& $ver) -eq '2.0.0' } 20))

        Write-Text (Join-Path $H 'run\fake-swap') '3.0.0'
        Check 'an unhealthy 3.0.0 is swapped in on trial' (Wait-Until { (Read-Text (Join-Path $H 'current.txt')) -eq "$Program-3.0.0.exe" } 20)
        Check 'it is stopped and rolled back to 2.0.0' (Wait-Until { (& $ver) -eq '2.0.0' -and (Read-Text (Join-Path $H 'current.txt')) -eq "$Program-2.0.0.exe" } 40)
        $log = Get-Content (Join-Path $H 'logs\supervise.log') -Raw
        Check 'the log says why' ($log -match 'did not report healthy' -and $log -match '3.0.0 never reported healthy; rolling back')

        $r = & $run @('status')
        Check 'status shows the version and that it runs' ($r.Code -eq 0 -and $r.Out -match 'current:\s+2.0.0' -and $r.Out -match 'running:\s+yes')

        $r = & $run @('stop')
        Check 'stop stops the game server (run\stop) and the supervisor' ($r.Code -eq 0 -and (Wait-Until { $sup.HasExited } 10) -and -not (Test-Path (Join-Path $H 'run\node.pid')))
        $fl = Get-Content (Join-Path $H 'logs\fake.log')
        Check 'every start was logged by the fake' ($fl.Count -ge 6)
    } catch {
        Check "selftest error: $($_.Exception.Message)" $false
    } finally {
        # Only what this selftest started: its supervisor, and a game server it left running.
        if ($sup -and -not $sup.HasExited) { Stop-Tree $sup.Id }
        $np = Get-PidProcess (Join-Path $H 'run\node.pid')
        if ($np -and $np.StartTime -ge $t0) { Stop-Tree $np.Id }
        $env:D2RUST_RELEASE_BASE = $saved.base
        $env:D2RUST_SERVER_START_WAIT = $saved.wait
        $env:D2RUST_NODE_HOME = $saved.home
        Start-Sleep -Milliseconds 500
        Remove-Item -LiteralPath $T -Recurse -Force -ErrorAction SilentlyContinue
    }
    Write-Output "$($script:checks - $script:fails) ok, $($script:fails) failed"
    if ($script:fails) { exit 1 }
}

$cmd = ''
if ($args.Count -ge 1) { $cmd = $args[0] }
$arg1 = ''
if ($args.Count -ge 2) { $arg1 = $args[1] }
switch ($cmd) {
    'setup' { Invoke-Setup $arg1 }
    'start' { Invoke-Start }
    'stop' { Invoke-Stop }
    'restart' { Invoke-Stop; Invoke-Start }
    'status' { Invoke-Status }
    'fetch' { Invoke-Fetch $arg1 }
    'rollback' { Invoke-Rollback }
    'supervise' { Invoke-Supervise }
    'selftest' { Invoke-Selftest }
    default { Die 'usage: node-ctl.ps1 setup [ID] | start | stop | restart | status | fetch <version> | rollback | selftest' }
}
