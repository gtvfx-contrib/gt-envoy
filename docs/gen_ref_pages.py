"""Generate API reference pages for mkdocs, consumed by mkdocs-literate-nav.

Runs automatically as part of ``properdocs build`` via the
``mkdocs-gen-files`` plugin (see ``properdocs.yml``). For each package in
:data:`PACKAGES` this:

- Writes a reference page for the package itself (``::: package``).
- Writes a reference page for every public submodule listed in that
  package's ``__all__`` (e.g. ``envoy.proc``, ``envoy.testing``,
  ``envoy.exceptions``).

New public submodules are picked up automatically the next time the docs
are built — adding one to a package's ``__all__`` is enough; no manual
nav or ``properdocs.yml`` edits are required.
"""

from __future__ import annotations

import importlib
import types

import mkdocs_gen_files

PACKAGES = ["envoy", "engit"]

nav = mkdocs_gen_files.Nav()


def _writeModulePage(doc_path: str, module_name: str) -> None:
    """Write a single reference page containing a mkdocstrings directive.

    Args:
        doc_path: Virtual path (relative to the docs dir) to write to.
        module_name: Fully-qualified module name to document.

    """
    with mkdocs_gen_files.open(doc_path, "w") as doc_file:
        doc_file.write(f"# `{module_name}`\n\n::: {module_name}\n")


for package_name in PACKAGES:
    package = importlib.import_module(package_name)

    doc_path = f"reference/{package_name}.md"
    _writeModulePage(doc_path, package_name)
    nav[(package_name,)] = f"{package_name}.md"

    for attr_name in sorted(getattr(package, "__all__", [])):
        attr = getattr(package, attr_name, None)
        if not isinstance(attr, types.ModuleType):
            continue

        submodule_name = f"{package_name}.{attr_name}"
        doc_path = f"reference/{submodule_name}.md"
        _writeModulePage(doc_path, submodule_name)
        nav[(package_name, attr_name)] = f"{submodule_name}.md"

with mkdocs_gen_files.open("reference/SUMMARY.md", "w") as nav_file:
    nav_file.writelines(nav.build_literate_nav())
