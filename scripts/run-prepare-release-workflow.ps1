param(
    [Parameter(Position=0, mandatory=$true)]
    [string]$Version
)


gh workflow run prepare-release.yml `
    --repo gtvfx-envoy/envoy `
    --ref main `
    -f version=$Version
