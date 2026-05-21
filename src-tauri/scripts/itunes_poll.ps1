# itunes_poll.ps1 — invoked by Hum's Rust backend.
#
# Polls iTunes via its COM automation interface and writes one JSON line per
# second to stdout. Each line contains the current track + state + position.
# Hum reads stdout and emits Tauri events.
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

# Cap matches smtc.rs::MAX_THUMBNAIL_BYTES so iTunes art behaves the same as
# SMTC art on the consumer side.
$MaxArtworkBytes = 10 * 1024 * 1024

function Get-ArtworkDataUrl($track) {
  try {
    if ($null -eq $track.Artwork -or $track.Artwork.Count -lt 1) { return $null }
    $art = $track.Artwork.Item(1)
    $tmp = [System.IO.Path]::GetTempFileName()
    try {
      $art.SaveArtworkToFile($tmp)
      if (-not (Test-Path $tmp)) { return $null }
      $size = (Get-Item $tmp).Length
      if ($size -le 0 -or $size -gt $MaxArtworkBytes) { return $null }
      $bytes = [System.IO.File]::ReadAllBytes($tmp)
      $b64 = [Convert]::ToBase64String($bytes)
      # iTunes COM ITArtworkFormat: 1=jpeg, 2=png, 3=bmp.
      $mime = switch ([int]$art.Format) {
        1 { "image/jpeg" }
        2 { "image/png" }
        3 { "image/bmp" }
        default { "image/jpeg" }
      }
      return "data:$mime;base64,$b64"
    } finally {
      Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    }
  } catch {
    return $null
  }
}

$itunes = $null
$lastTrackKey = ""

while ($true) {
  if (-not $itunes) {
    $itunes = Get-ITunes
    if (-not $itunes) {
      Emit @{ source = "itunes"; present = $false }
      $lastTrackKey = ""
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
      $lastTrackKey = ""
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

      $payload = @{
        source = "itunes"; present = $true
        state = $stateName
        title = $title
        artist = $artist
        album = $album
        duration_ms = [int64]([math]::Round($duration * 1000))
        position_ms = [int64]([math]::Round($position * 1000))
      }

      # Only re-extract artwork when the track itself changes. Each save+read
      # is a 100-500KB temp-file round-trip; doing it every poll would burn
      # disk and stdin bandwidth for no user-visible benefit.
      $trackKey = "$title|$artist|$album"
      if ($trackKey -ne $lastTrackKey) {
        $artUrl = Get-ArtworkDataUrl $track
        if ($null -ne $artUrl) {
          $payload.art_data_url = $artUrl
        }
        $lastTrackKey = $trackKey
      }

      Emit $payload
    }
  } catch {
    # COM call failed — iTunes likely closed or RPC server unavailable.
    # Drop the ref and try to reconnect.
    $itunes = $null
    $lastTrackKey = ""
    Emit @{ source = "itunes"; present = $false; error = $_.Exception.Message }
    Start-Sleep -Seconds 2
    continue
  }

  Start-Sleep -Seconds 1
}
