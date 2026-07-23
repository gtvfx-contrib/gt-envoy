"""Shared fixtures for the ``envoy-py`` wheel Python contract tests.

Autouse-clears ``ENVOY_BNDL_ROOTS`` for every test in this directory. Several
tests build an ``Environment``/``ApplicationWrapper`` from an explicit
``commands_file``, expecting bundle auto-discovery to find nothing so the
commands-file fallback kicks in (see ``envoy_core::runtime::load_registry``'s
documented discovery priority: explicit ``bundle_roots`` > ``ENVOY_BNDL_ROOTS``
auto-discovery > ``commands_file`` > upward CWD search). If a developer
machine has ``ENVOY_BNDL_ROOTS`` set in its real environment (as is common
for local envoy development), auto-discovery would find real bundles first
and shadow the test's own commands.json -- this is a pre-existing behavior
confirmed identical against the pure-Python ``py/envoy`` implementation, not
a wheel-specific regression, but it makes these tests flaky depending on the
host machine's environment. Clearing it here keeps the tests deterministic.
"""

import pytest


@pytest.fixture(autouse=True)
def _clear_envoy_bndl_roots(monkeypatch):
    monkeypatch.delenv("ENVOY_BNDL_ROOTS", raising=False)
