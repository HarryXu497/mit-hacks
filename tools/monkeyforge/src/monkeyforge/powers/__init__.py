"""Sketch -> superpower -> BTD6-style badge icon.

Two separate problems, deliberately kept apart (see `docs/SUPERPOWER-ICONS.md`):

* `registry` + `classify` map a drawing to one of the four powers the runtime
  implements. This is retrieval against a fixed list, not learning.
* `badge` composites a subject into a procedurally drawn frame, so every icon
  shares one rim and only the subject can go wrong.
"""

from monkeyforge.powers.registry import POWERS, Power, PowerId, by_id, by_onehot

__all__ = ["POWERS", "Power", "PowerId", "by_id", "by_onehot"]
