$ErrorActionPreference = "Stop"

$baseUrl = $env:TRADER_BASE_URL
if (-not $baseUrl) {
    $baseUrl = "http://127.0.0.1:8080"
}

Invoke-RestMethod "$baseUrl/api/v1/health" | Out-Null
$paper = Invoke-RestMethod -Method Post "$baseUrl/api/v1/paper-runs"
if ($paper.status -ne "running") { throw "expected paper run to start as running" }

$status = $null
for ($i = 0; $i -lt 80; $i++) {
    Start-Sleep -Milliseconds 250
    $status = Invoke-RestMethod "$baseUrl/api/v1/runs/$($paper.run_id)/status"
    if ($status.status -eq "completed") { break }
}
if ($status.status -ne "completed") { throw "expected paper run to complete" }

$fills = Invoke-RestMethod "$baseUrl/api/v1/fills"
$balances = Invoke-RestMethod "$baseUrl/api/v1/account-balances"
$snapshots = Invoke-RestMethod "$baseUrl/api/v1/portfolio/snapshots"
$metrics = Invoke-RestMethod "$baseUrl/api/v1/metrics"
$replay = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replays"
$replayPause = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/pause"
$replaySeek = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/seek/2"
$replaySpeed = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/speed/25"
$replayResume = Invoke-RestMethod -Method Post "$baseUrl/api/v1/replay/$($paper.run_id)/resume"
$events = Invoke-RestMethod "$baseUrl/api/v1/events"
$runEvents = Invoke-RestMethod "$baseUrl/api/v1/runs/$($paper.run_id)/events"

if (@($fills).Count -lt 1) { throw "expected at least one fill" }
if (@($balances).Count -lt 1) { throw "expected at least one account balance" }
if (@($snapshots).Count -lt 1) { throw "expected at least one portfolio snapshot" }
if ($metrics.fill_count -lt 1) { throw "expected metrics fill_count >= 1" }
if ($replay.bars -lt 1) { throw "expected replay bars >= 1" }
if ($replayPause.status -ne "paused") { throw "expected replay pause status paused" }
if ($replaySeek.offset -ne 2) { throw "expected replay seek offset 2" }
if ($replaySpeed.speed -ne 25) { throw "expected replay speed 25" }
if ($replayResume.status -ne "running") { throw "expected replay resume status running" }
if (@($events).Count -lt 1) { throw "expected at least one event" }
if (@($runEvents).Count -lt 1) { throw "expected at least one run event" }

[pscustomobject]@{
    run_id = $paper.run_id
    status = $status.status
    fills = @($fills).Count
    balances = @($balances).Count
    snapshots = @($snapshots).Count
    total_return = $metrics.total_return
    replay_bars = $replay.bars
    replay_status = $replayResume.status
    replay_offset = $replaySeek.offset
    replay_speed = $replaySpeed.speed
    events = @($events).Count
    run_events = @($runEvents).Count
}
