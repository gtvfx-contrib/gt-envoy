param(
    [Parameter(Position=0, mandatory=$true)]
    [string]$Version
)


gh workflow run prepare-release.yml `
    --repo gtvfx-contrib/gt-envoy `
    --ref main `
    -f version=$Version
