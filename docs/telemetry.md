# Telemetry

Envoy can export a record of every command it runs -- for your own
diagnostics, or aggregated fleet-wide into one shared, studio-hosted
Grafana dashboard. Disabled by default, and best-effort: a misconfigured
or unreachable destination is visible under `--diagnose` but never
affects a command's own behavior or exit code.

## Enabling it

Set `ENVOY_TELEMETRY_ENDPOINT` -- either in your own shell/session, or
(more commonly) baked into your studio's own stack `global_env.json` so
it applies fleet-wide automatically:

- A value starting with `http://` or `https://` selects **direct
  OTLP/HTTP export** to an always-on Collector.
- Any other value (a UNC path, mapped drive, or mount point) selects
  **file-drop export**: each record is serialized as an OTLP-JSON payload
  and written atomically under that path -- no listening service required
  there.
- Standard `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` / `OTEL_EXPORTER_OTLP_ENDPOINT`
  are honored as an **endpoint fallback**, used only when
  `ENVOY_TELEMETRY_ENDPOINT` itself is unset.
- `OTEL_EXPORTER_OTLP_HEADERS` / `OTEL_EXPORTER_OTLP_TIMEOUT` /
  `OTEL_SERVICE_NAME` / `OTEL_RESOURCE_ATTRIBUTES`, by contrast, always
  apply whenever they're set -- regardless of whether
  `ENVOY_TELEMETRY_ENDPOINT` or the endpoint fallback resolved the
  destination. In particular, `OTEL_EXPORTER_OTLP_HEADERS` is how a
  shared Collector bearer token reaches the exporter even when
  `ENVOY_TELEMETRY_ENDPOINT` (not an `OTEL_*` var) is what selected the
  destination.
- `ENVOY_TELEMETRY_ENABLED=false` always disables collection, regardless
  of any other setting -- your own opt-out always wins over a
  studio-wide default.

Resolution checks envoy-cli's own process environment first, then the
selected stack's merged bundle environment -- so a personal override
always beats a fleet-wide default.

Check the current state any time with `envoy --diagnose` (see the
[CLI reference](cli-reference/envoy.md#--diagnose-command)).

## What gets recorded

One `envoy.command.run` record per invocation -- built-ins (`--list`,
`--diagnose`, `--which`, `--trace`, `--docs`, config commands, help),
resolution failures, and the managed-command path all get their own
record, not just successful command runs. Each includes real start/end
timestamps (not just a duration attribute), stack/team/bundle context,
envoy version, a pseudonymous per-workstation installation ID (never a
username or hostname), the complete argv (both as originally typed and
alias-expanded), success/exit code, and which transport delivered it.

### Redaction

Secret-looking values are redacted **before** a record is ever
constructed -- nothing sensitive is written to disk or sent anywhere, in
two independent, always-on layers (neither can be disabled by a bundle):

1. **Flag-name-based**: `--token value`, `--api-key=value`, and other
   sensitive-looking flag names (extend the built-in list via
   `ENVOY_TELEMETRY_REDACT_ARGS`).
2. **Pattern-based**: every value is also scanned regardless of which
   flag (if any) it followed -- JWT-shaped strings, bearer-token headers,
   embedded URL/connection-string credentials, and long high-entropy
   tokens. Tuned to leave ordinary long identifiers (bundle IDs, UUIDs,
   content hashes) alone.

### Delivery resilience

If delivery fails (share unreachable, collector unreachable), the
already-redacted record is queued in a small local spool under
`~/.envoy/telemetry/spool/` (bounded by both count and size, oldest-first
eviction) and retried -- one bounded, time-limited attempt per subsequent
invocation -- until it succeeds. A command is never blocked on network or
file I/O.

## Version compatibility

The client (every workstation's `envoy` binary) and the shared server
bundle (Collector/Tempo/Grafana, in the separate
[`telemetry`](https://github.com/gtvfx-envoy/telemetry) bundle) are
released and updated independently -- the client is expected to update far
more often than the server. This is safe by design:

- Record attributes are **additive by default**: an older
  Collector/Tempo/dashboard simply stores or ignores attributes it
  doesn't recognize, and a newer dashboard tolerates older clients missing
  newer attributes.
- The schema-version attribute (`envoy.schema_version`, see
  `envoy-core::telemetry::schema`) is bumped **only** on a breaking change
  (an attribute rename, removal, or semantic change) -- never for
  additive changes.

| envoy release line | minimum telemetry-bundle schema version |
|---|---|
| current | 1 |

(Table intentionally starts at one row -- extend it if/when a future
breaking change ships, on either side.)

## The shared studio dashboard

Aggregating usage across every user's workstation into one dashboard is a
studio policy decision, not just a technical one -- confirm with whoever
owns employee-monitoring/privacy policy before a fleet-wide rollout. Once
approved, see the [`telemetry` bundle's own
README](https://github.com/gtvfx-envoy/telemetry) for server setup,
authentication, retention, and a tour of the provisioned "Envoy Command
Telemetry" dashboard.
