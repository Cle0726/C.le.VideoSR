param(
  [string]$ManifestPath = "runtime/sources.json",
  [string]$Platform = "windows-x64"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Resolve-PairByStem {
  param([string]$Root, [string]$Stem)

  $paramFile = Get-ChildItem -Path $Root -Recurse -File -Filter "$Stem.param" | Select-Object -First 1
  $binFile = Get-ChildItem -Path $Root -Recurse -File -Filter "$Stem.bin" | Select-Object -First 1
  return ($null -ne $paramFile -and $null -ne $binFile)
}

function Resolve-PairInDirectory {
  param([string]$Root, [string]$DirectoryName)

  $directories = Get-ChildItem -Path $Root -Recurse -Directory | Where-Object { $_.Name -eq $DirectoryName }
  foreach ($directory in $directories) {
    $params = @{}
    Get-ChildItem -Path $directory.FullName -Recurse -File -Filter "*.param" | ForEach-Object {
      $params[$_.BaseName] = $true
    }
    foreach ($bin in (Get-ChildItem -Path $directory.FullName -Recurse -File -Filter "*.bin")) {
      if ($params.ContainsKey($bin.BaseName)) {
        return $true
      }
    }
  }
  return $false
}

function Show-ModelInventory {
  param([string]$Root, [string]$ComponentId)

  $params = @(Get-ChildItem -Path $Root -Recurse -File -Filter "*.param" | Sort-Object FullName)
  Write-Host "MODEL INVENTORY $ComponentId ($($params.Count) .param files)"
  foreach ($file in ($params | Select-Object -First 80)) {
    $relative = $file.FullName.Substring($Root.Length).TrimStart('\', '/')
    $paired = Test-Path ([System.IO.Path]::ChangeExtension($file.FullName, ".bin"))
    Write-Host "  $relative paired=$paired"
  }
  if ($params.Count -gt 80) {
    Write-Host "  ... truncated $($params.Count - 80) additional .param files"
  }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifestFullPath = Join-Path $repoRoot $ManifestPath
$manifest = Get-Content -Raw -Encoding UTF8 $manifestFullPath | ConvertFrom-Json
$platformConfig = $manifest.platforms.$Platform
if ($null -eq $platformConfig) {
  throw "Unknown runtime source platform: $Platform"
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("c-le-videosr-runtime-audit-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
$results = @()

try {
  foreach ($component in $platformConfig.components) {
    if ($component.redistribution_status -ne "approved_with_notice") {
      Write-Host "SKIP $($component.id): $($component.redistribution_status)"
      continue
    }

    if ([string]::IsNullOrWhiteSpace([string]$component.archive_url)) {
      throw "$($component.id) is approved for redistribution but has no archive_url"
    }

    $componentRoot = Join-Path $tempRoot $component.id
    $archivePath = Join-Path $tempRoot ("$($component.id).zip")
    New-Item -ItemType Directory -Path $componentRoot | Out-Null

    Write-Host "Downloading $($component.id) $($component.version)"
    Invoke-WebRequest -Uri $component.archive_url -OutFile $archivePath

    $actualHash = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $expectedHash = [string]$component.sha256
    if (-not [string]::IsNullOrWhiteSpace($expectedHash)) {
      if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
        throw "$($component.id) SHA-256 mismatch. expected=$expectedHash actual=$actualHash"
      }
      Write-Host "HASH OK $($component.id) $actualHash"
    } else {
      Write-Host "PIN $($component.id) sha256=$actualHash"
      if ($env:GITHUB_STEP_SUMMARY) {
        Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value "- ``$($component.id)`` SHA-256: ``$actualHash``"
      }
    }

    Expand-Archive -Path $archivePath -DestinationPath $componentRoot -Force

    $binary = Get-ChildItem -Path $componentRoot -Recurse -File | Where-Object { $_.Name -eq $component.binary } | Select-Object -First 1
    if ($null -eq $binary) {
      throw "$($component.id) archive does not contain expected binary $($component.binary)"
    }

    Show-ModelInventory -Root $componentRoot -ComponentId $component.id

    foreach ($check in @($component.model_checks)) {
      if ($null -eq $check) { continue }
      switch ($check.kind) {
        "exact_pair" {
          if (-not (Resolve-PairByStem -Root $componentRoot -Stem $check.stem)) {
            throw "$($component.id) is missing model pair $($check.stem).param/.bin"
          }
        }
        "directory_pair" {
          if (-not (Resolve-PairInDirectory -Root $componentRoot -DirectoryName $check.directory)) {
            throw "$($component.id) is missing a .param/.bin pair under $($check.directory)"
          }
        }
        default {
          throw "$($component.id) has unsupported model check kind: $($check.kind)"
        }
      }
    }

    $results += [PSCustomObject]@{
      component = $component.id
      version = $component.version
      sha256 = $actualHash
      binary = $binary.FullName.Substring($componentRoot.Length).TrimStart('\', '/')
      archive_bytes = (Get-Item $archivePath).Length
    }
  }

  Write-Host "Runtime source audit passed."
  $results | Format-Table -AutoSize
  $jsonResult = $results | ConvertTo-Json -Depth 5
  $resultPath = Join-Path $repoRoot "runtime-audit-hashes.json"
  Set-Content -Path $resultPath -Value $jsonResult -Encoding UTF8
} finally {
  Remove-Item -Path $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
