"""envoy -- Environment orchestration for managed application execution.

Phase-0 scaffolding placeholder for the PyO3-backed `envoy` package. Real
public API surface (Bundle, BundleConfig, ApplicationWrapper, proc, testing,
exceptions, etc. -- see py/envoy/__init__.py for the full contract to
replicate) is ported in Phases 2-4 of the Rust migration plan.

Once ported, this file becomes the thin Python-side aggregator that
re-exports symbols from the compiled `envoy._envoy` extension module,
mirroring today's `py/envoy/__init__.py` structure so `import envoy` keeps
working unchanged for existing consumers.
"""

from __future__ import annotations

from ._envoy import _core_version

__all__ = ["_core_version"]
