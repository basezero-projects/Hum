# itunes_poll.ps1 — invoked by lyric-overlay's Rust backend.
#
# Polls iTunes via its COM automation interface and writes one JSON line per
# second to stdout. Each line contains the current track + state + position.
# Lyric Overlay reads stdout and emits Tauri events.
#
# Notes:
# - We never *launch* iTunes. We only attach when the iTunes process already
#   exists. If iTunes closes, we drop the COM ref and reconnect when it returns.
# - Apple's modern Microsoft Store "Apple Music" app has no COM API. Users on
#   that app are covered by SMTC instead.

$ErrorActionPreference = "Continue"
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

function Get-ITunes {
  $proc = Get-Process iTunes -ErrorAction SilentlyContinue
  if (-not $proc) { return $null }
  try {
    return New-Object -ComObject iTunes.Application
  } catch {
    return $null
  }
}

function Emit($obj) {
  $json = $obj | ConvertTo-Json -Compress -Depth 4
  [Console]::Out.WriteLine($json)
  [Console]::Out.Flush()
}

$itunes = $null

while ($true) {
  if (-not $itunes) {
    $itunes = Get-ITunes
    if (-not $itunes) {
      Emit @{ source = "itunes"; present = $false }
      Start-Sleep -Seconds 3
      continue
    }
    Emit @{ source = "itunes"; present = $true }
  }

  try {
    $rawState = $itunes.PlayerState  # 0=stopped/paused, 1=playing, 2=fwd, 3=rwd
    $track = $itunes.CurrentTrack

    if ($null -eq $track) {
      Emit @{
        source = "itunes"; present = $true
        state = "stopped"
        title = ""; artist = ""; album = ""
        duration_ms = 0; position_ms = 0
      }
    } else {
      $title    = if ($null -ne $track.Name)   { [string]$track.Name }   else { "" }
      $artist   = if ($null -ne $track.Artist) { [string]$track.Artist } else { "" }
      $album    = if ($null -ne $track.Album)  { [string]$track.Album }  else { "" }
      $duration = [double]$track.Duration            # seconds
      $position = [double]$itunes.PlayerPosition     # seconds

      # iTunes COM has no distinct paused state — PlayerState=0 with position>0
      # means paused mid-track; PlayerState=0 with position=0 means stopped.
      $stateName = if ($rawState -eq 0 -and $position -gt 0.05) {
        "paused"
      } elseif ($rawState -eq 0) {
        "stopped"
      } elseif ($rawState -ge 1 -and $rawState -le 3) {
        "playing"
      } else {
        "unknown"
      }

      Emit @{
        source = "itunes"; present = $true
        state = $stateName
        title = $title
        artist = $artist
        album = $album
        duration_ms = [int64]([math]::Round($duration * 1000))
        position_ms = [int64]([math]::Round($position * 1000))
      }
    }
  } catch {
    # COM call failed — iTunes likely closed or RPC server unavailable.
    # Drop the ref and try to reconnect.
    $itunes = $null
    Emit @{ source = "itunes"; present = $false; error = $_.Exception.Message }
    Start-Sleep -Seconds 2
    continue
  }

  Start-Sleep -Seconds 1
}
