#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Replay the Elite Dangerous carrier journal into the API so the frontend can be
  watched live.

.DESCRIPTION
  Truncates `carriers` and `carrier_events` (history cascades), then POSTs every
  carrier event from the journal in chronological order. Events are paced so the
  UI visibly steps through them.

  The backend must be running. The database is truncated through the compose
  service, so Docker must be up too.

.PARAMETER Url
  API base URL. Default http://127.0.0.1:8080

.PARAMETER JournalDir
  Folder holding Journal*.log. Defaults to the standard ED location.

.PARAMETER DelayMs
  Milliseconds between events. Lower = faster. Default 400. When -DelayMax is
  set, this becomes the *minimum* gap.

.PARAMETER DelayMax
  If greater than DelayMs, each gap is picked at random from [DelayMs, DelayMax]
  so the pacing varies instead of ticking metronomically. 0 = fixed DelayMs.
  Useful for watching how the page handles irregular updates.

.PARAMETER Limit
  Replay only the first N events. 0 = all. Default 0.

.PARAMETER Loop
  Repeat forever (truncate + replay) until Ctrl+C. Handy for watching the UI.

.PARAMETER SkipTruncate
  Append to the existing data instead of wiping it first.

.EXAMPLE
  ./scripts/reseed.ps1
.EXAMPLE
  ./scripts/reseed.ps1 -DelayMs 120
.EXAMPLE
  ./scripts/reseed.ps1 -DelayMs 600 -DelayMax 2500
.EXAMPLE
  ./scripts/reseed.ps1 -DelayMs 800 -Loop
.EXAMPLE
  ./scripts/reseed.ps1 -Limit 30 -DelayMs 1000
#>
[CmdletBinding()]
param(
	[string]$Url = 'http://127.0.0.1:8080',
	[string]$JournalDir = (Join-Path $env:USERPROFILE 'Saved Games\Frontier Developments\Elite Dangerous'),
	[int]$DelayMs = 400,
	[int]$DelayMax = 0,
	[int]$Limit = 0,
	[switch]$Loop,
	[switch]$SkipTruncate,
	[string]$DbService = 'db',
	[string]$DbUser = 'ed',
	[string]$DbName = 'ed_commander'
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$composeFile = Join-Path $repoRoot 'compose.yaml'

function Assert-Api {
	try {
		$r = Invoke-WebRequest -Uri "$Url/api/healthz" -UseBasicParsing -TimeoutSec 3
		if ($r.StatusCode -ne 200) { throw "unexpected status $($r.StatusCode)" }
	} catch {
		throw "API not reachable at $Url - start the backend first (cd backend; cargo run). $_"
	}
}

function Get-CarrierEvents {
	if (-not (Test-Path $JournalDir)) { throw "Journal directory not found: $JournalDir" }

	$files = Get-ChildItem $JournalDir -Filter 'Journal*.log' | Sort-Object Name
	if (-not $files) { throw "No Journal*.log files in $JournalDir" }

	$events = [System.Collections.Generic.List[string]]::new()
	foreach ($f in $files) {
		foreach ($line in [System.IO.File]::ReadLines($f.FullName)) {
			if ($line -match '"event":"Carrier') { $events.Add($line) }
		}
	}
	return $events
}

function Reset-Database {
	Write-Host '  truncating carriers + carrier_events ...' -ForegroundColor DarkGray
	$sql = 'TRUNCATE carriers, carrier_events RESTART IDENTITY CASCADE;'
	$out = docker compose -f $composeFile exec -T $DbService psql -U $DbUser -d $DbName -c $sql 2>&1
	if ($LASTEXITCODE -ne 0) { throw "truncate failed: $out" }
}

function Invoke-Replay {
	param(
		[System.Collections.Generic.List[string]]$Events,
		[int]$DelayMs,
		[int]$DelayMax,
		[int]$Limit
	)

	$toSend = if ($Limit -gt 0) { [Math]::Min($Limit, $Events.Count) } else { $Events.Count }

	$client = [System.Net.Http.HttpClient]::new()
	$client.Timeout = [TimeSpan]::FromSeconds(30)

	$ok = 0; $fail = 0
	$sw = [System.Diagnostics.Stopwatch]::StartNew()

	try {
		for ($i = 0; $i -lt $toSend; $i++) {
			$line = $Events[$i]
			$name = if ($line -match '"event":"(Carrier[A-Za-z]+)"') { $Matches[1] } else { '?' }

			$content = [System.Net.Http.StringContent]::new(
				$line, [System.Text.Encoding]::UTF8, 'application/json')

			try {
				$res = $client.PostAsync("$Url/api/carrier/event", $content).GetAwaiter().GetResult()
				if ($res.IsSuccessStatusCode) {
					$ok++
					Write-Host ("  [{0,3}/{1}] {2}" -f ($i + 1), $toSend, $name) -ForegroundColor Green
				} else {
					$fail++
					$body = $res.Content.ReadAsStringAsync().GetAwaiter().GetResult()
					Write-Host ("  [{0,3}/{1}] {2} -> HTTP {3}: {4}" -f ($i + 1), $toSend, $name, [int]$res.StatusCode, $body) -ForegroundColor Red
				}
			} catch {
				$fail++
				Write-Host ("  [{0,3}/{1}] {2} -> {3}" -f ($i + 1), $toSend, $name, $_.Exception.Message) -ForegroundColor Red
			} finally {
				$content.Dispose()
			}

			if ($i -lt $toSend - 1) {
				$gap = if ($DelayMax -gt $DelayMs) {
					Get-Random -Minimum $DelayMs -Maximum ($DelayMax + 1)
				} else {
					$DelayMs
				}
				if ($gap -gt 0) { Start-Sleep -Milliseconds $gap }
			}
		}
	} finally {
		$client.Dispose()
	}

	$sw.Stop()
	Write-Host ("  done: ok={0} fail={1} in {2}s" -f $ok, $fail, [math]::Round($sw.Elapsed.TotalSeconds, 1)) -ForegroundColor Cyan
	return ($fail -eq 0)
}

# ---------------------------------------------------------------------------

Assert-Api
$events = Get-CarrierEvents
Write-Host "journal: $($events.Count) carrier events from $JournalDir" -ForegroundColor Cyan
$pacing = if ($DelayMax -gt $DelayMs) { "$DelayMs-$DelayMax ms (random)" } else { "${DelayMs}ms" }
Write-Host "target : $Url   delay: $pacing$(if ($Loop) { '   loop: on' })" -ForegroundColor Cyan

$pass = 0
do {
	$pass++
	if ($Loop) { Write-Host "`n=== pass $pass ===" -ForegroundColor Yellow }

	if (-not $SkipTruncate) { Reset-Database }
	$clean = Invoke-Replay -Events $events -DelayMs $DelayMs -DelayMax $DelayMax -Limit $Limit

	if ($SkipTruncate) { break }
	if ($Limit -gt 0) {
		Write-Host '  (partial replay - state is mid-journal)' -ForegroundColor DarkGray
		break
	}
} while ($Loop)

if ($clean) { Write-Host 're-seed complete.' -ForegroundColor Green }
else { Write-Host 're-seed finished with failures.' -ForegroundColor Red; exit 1 }
