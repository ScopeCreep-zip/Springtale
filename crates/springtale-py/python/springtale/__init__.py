"""Springtale's cooperation primitives, for Python.

The compiled extension provides every class; this package exists so the
wheel can ship type stubs beside it (see ``__init__.pyi``). Importing
from here and from the extension is the same thing.
"""

from .springtale import Formation, FormationId, Intent, MomentumTier, __version__

__all__ = ["Formation", "FormationId", "Intent", "MomentumTier", "__version__"]
