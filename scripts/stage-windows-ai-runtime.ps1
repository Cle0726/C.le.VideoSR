param(
  [string]$ManifestPath = "runtime/sources.json",
  [string]$Platform = "windows-x64"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Find-FileByName {
  param([string]$Root, [string]$Name)
  return Get-ChildItem -Path $Root -Recurse -File | Where-Object { $_.Name -eq $Name } | Select-Object -First 1
}

function Find-DirectoryByName {
  param([string]$Root, [string]$Name)
  return Get-ChildItem -Path $Root -Recurse -Directory | Where-Object { $_.Name -eq $Name } | Select-Object -First 1
}

function Copy-DirectoryContents {
  param([string]$Source, [string]$Destination)
  New-Item -ItemType Directory -Path $Destination -Force | Out-Null
  Get-ChildItem -Path $Source -Force | ForEach-Object {
    Copy-Item -Path $_.FullName -Destination $Destination -Recurse -Force
  }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifest = Get-Content -Raw -Encoding UTF8 (Join-Path $repoRoot $ManifestPath) | ConvertFrom-Json
$config = $manifest.platforms.$Platform
if ($null -eq $config) { throw "Unknown platform $Platform" }

$runtimeRoot = Join-Path $repoRoot "runtime"
$binRoot = Join-Path $runtimeRoot "bin"
$modelRoot = Join-Path $runtimeRoot "models"
$licenseRoot = Join-Path $runtimeRoot "licenses"

Remove-Item $binRoot -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $modelRoot -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $licenseRoot -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $binRoot, $modelRoot, $licenseRoot -Force | Out-Null

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("c-le-videosr-stage-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
$staged = @()

try {
  foreach ($component in $config.components) {
    if ($component.redistribution_status -ne "approved_with_notice") {
      Write-Host "SKIP $($component.id): $($component.redistribution_status)"
      continue
    }
    if ([string]::IsNullOrWhiteSpace([string]$component.archive_url) -or [string]::IsNullOrWhiteSpace([string]$component.sha256)) {
      throw "$($component.id) must have a pinned archive URL and SHA-256 before staging"
    }
    if ([string]::IsNullOrWhiteSpace([string]$component.license_raw_url)) {
      throw "$($component.id) must have a pinned engine license notice URL before staging"
    }

    $archive = Join-Path $tempRoot ("$($component.id).zip")
    $extract = Join-Path $tempRoot $component.id
    New-Item -ItemType Directory -Path $extract | Out-Null

    Write-Host "Stage $($component.id) $($component.version)"
    Invoke-WebRequest -Uri $component.archive_url -OutFile $archive
    $actualHash = (Get-FileHash -Path $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne ([string]$component.sha256).ToLowerInvariant()) {
      throw "$($component.id) SHA-256 mismatch"
    }
    Expand-Archive -Path $archive -DestinationPath $extract -Force

    $binary = Find-FileByName -Root $extract -Name $component.binary
    if ($null -eq $binary) { throw "$($component.id) binary $($component.binary) was not found" }
    Copy-Item $binary.FullName (Join-Path $binRoot $component.binary) -Force

    Get-ChildItem -Path $binary.Directory.FullName -File -Filter "*.dll" | ForEach-Object {
      Copy-Item $_.FullName (Join-Path $binRoot $_.Name) -Force
    }

    $noticeDir = Join-Path $licenseRoot $component.id
    New-Item -ItemType Directory -Path $noticeDir -Force | Out-Null
    Invoke-WebRequest -Uri $component.license_raw_url -OutFile (Join-Path $noticeDir "ENGINE_LICENSE.txt")
    if ($component.PSObject.Properties.Name -contains "model_license_raw_url" -and -not [string]::IsNullOrWhiteSpace([string]$component.model_license_raw_url)) {
      Invoke-WebRequest -Uri $component.model_license_raw_url -OutFile (Join-Path $noticeDir "MODEL_LICENSE.txt")
    }
    $readmeFile = Get-ChildItem -Path $extract -Recurse -File | Where-Object { $_.Name -match '^README(\..*)?$' } | Select-Object -First 1
    if ($null -ne $readmeFile) {
      Copy-Item $readmeFile.FullName (Join-Path $noticeDir "README-upstream.md") -Force
    }

    switch ($component.id) {
      "realesrgan-ncnn-vulkan" {
        $marker = Find-FileByName -Root $extract -Name "realesrgan-x4plus.param"
        if ($null -eq $marker) { throw "Real-ESRGAN models directory not found" }
        $sourceModels = $marker.Directory.FullName
        $stems = @(
          "realesrgan-x4plus",
          "realesrgan-x4plus-anime",
          "realesr-animevideov3-x2",
          "realesr-animevideov3-x4"
        )
        foreach ($stem in $stems) {
          foreach ($extension in @("param", "bin")) {
            $source = Join-Path $sourceModels "$stem.$extension"
            if (-not (Test-Path $source)) { throw "Missing Real-ESRGAN payload $stem.$extension" }
            Copy-Item $source (Join-Path $modelRoot "$stem.$extension") -Force
          }
        }
      }
      "realcugan-ncnn-vulkan" {
        $sourceModels = Find-DirectoryByName -Root $extract -Name "models-se"
        if ($null -eq $sourceModels) { throw "Real-CUGAN models-se directory not found" }
        Copy-DirectoryContents -Source $sourceModels.FullName -Destination (Join-Path $modelRoot "models-se")
      }
      "rife-ncnn-vulkan" {
        foreach ($name in @("rife-v4.6", "rife-anime")) {
          $sourceModels = Find-DirectoryByName -Root $extract -Name $name
          if ($null -eq $sourceModels) { throw "RIFE model directory $name not found" }
          Copy-DirectoryContents -Source $sourceModels.FullName -Destination (Join-Path $modelRoot $name)
        }
      }
      default { throw "No staging layout rule for $($component.id)" }
    }

    $modelLicense = $null
    if ($component.PSObject.Properties.Name -contains "model_license") { $modelLicense = $component.model_license }
    $staged += [PSCustomObject]@{
      component = $component.id
      version = $component.version
      sha256 = $actualHash
      source = $component.archive_url
      engine_license = $component.license
      model_license = $modelLicense
    }
  }

  $manifestOut = [PSCustomObject]@{
    schema_version = 1
    platform = $Platform
    generated_utc = [DateTime]::UtcNow.ToString("o")
    components = $staged
  }
  $manifestOut | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $runtimeRoot "staged-ai-runtime.json") -Encoding UTF8

  Write-Host "Staged AI runtime files:"
  Get-ChildItem -Path $runtimeRoot -Recurse -File | ForEach-Object {
    Write-Host "  $($_.FullName.Substring($runtimeRoot.Length).TrimStart('\', '/'))"
  }
} finally {
  Remove-Item $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
