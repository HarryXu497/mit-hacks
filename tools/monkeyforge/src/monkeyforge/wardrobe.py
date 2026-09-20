"""Decide how each drawn item becomes part of the character.

A drawing contains two fundamentally different kinds of thing, and they need different machinery:

  CONFORMING garments -- a jersey, a shirt, a suit -- lie flat against the body. They have no
  silhouette of their own, so generating geometry for them is wasted work and would fight the
  base mesh. These become a texture layer painted into the torso's UV region.

  ACCESSORIES -- a cap, a backpack, a shield -- stick out. Their whole point is silhouette, which a
  texture cannot express: a cap painted onto a head is a hat-coloured head. These become real
  geometry and attach to a socket.

Getting this split wrong is what makes generated characters look flat or, worse, makes the pipeline
regenerate a monkey that already exists. The base body is never generated; only what is worn.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum

from monkeyforge.models import Socket


class Construction(StrEnum):
    """How a worn item is realised."""

    TEXTURE = "texture"      # painted into the base's UV map
    GEOMETRY = "geometry"    # generated as a mesh and socketed


#: Items that lie against the body. Texture is the right and cheapest representation.
CONFORMING: dict[str, str] = {
    "jersey": "torso",
    "shirt": "torso",
    "top": "torso",
    "tshirt": "torso",
    "vest": "torso",
    "suit": "torso",
    "armour": "torso",
    "armor": "torso",
    "uniform": "torso",
    "robe": "torso",
    "hoodie": "torso",
    "jacket": "torso",
    "blazer": "torso",
    # A tie hangs flat down the chest. It reads as a shape, which tempts a mesh, but it has no
    # silhouette off the body -- generating one would be a flat plane floating a millimetre in
    # front of a shirt, with a seam where it meets the collar.
    "tie": "torso",
    "necktie": "torso",
    "bowtie": "torso",
    "scarf": "torso",
    "shorts": "legs",
    "trousers": "legs",
    "pants": "legs",
    "jeans": "legs",
}

#: Items with their own silhouette, and the socket each belongs on.
ACCESSORIES: dict[str, Socket] = {
    "cap": Socket.HEAD_TOP,
    "hat": Socket.HEAD_TOP,
    "helmet": Socket.HEAD_TOP,
    "crown": Socket.HEAD_TOP,
    "headband": Socket.HEAD_TOP,
    "goggles": Socket.FACE,
    "sunglasses": Socket.FACE,
    "shades": Socket.FACE,
    "glasses": Socket.FACE,
    "mask": Socket.FACE,
    "visor": Socket.FACE,
    "backpack": Socket.BACK,
    "jetpack": Socket.BACK,
    "cape": Socket.BACK,
    "wings": Socket.BACK,
    "belt": Socket.WAIST,
    "shield": Socket.HAND_LEFT,
    "sword": Socket.HAND_RIGHT,
    "staff": Socket.HAND_RIGHT,
    "weapon": Socket.HAND_RIGHT,
    "dart": Socket.HAND_RIGHT,
}


@dataclass
class WornItem:
    """One thing the monkey is wearing, as read from the drawing."""

    kind: str
    colour: str = "red"
    secondary_colour: str | None = None
    text: str | None = None

    @property
    def normalised(self) -> str:
        return self.kind.lower().strip().replace(" ", "").replace("-", "")

    @property
    def construction(self) -> Construction:
        if self.normalised in ACCESSORIES:
            return Construction.GEOMETRY
        return Construction.TEXTURE

    @property
    def socket(self) -> Socket | None:
        return ACCESSORIES.get(self.normalised)

    @property
    def region(self) -> str:
        return CONFORMING.get(self.normalised, "torso")

    def describe(self) -> str:
        """A prompt fragment, shared by both paths so the two halves stay visually consistent."""
        parts = [f"{self.colour} {self.kind}"]
        if self.secondary_colour:
            parts.append(f"with {self.secondary_colour} trim")
        return " ".join(parts)


@dataclass
class Wardrobe:
    """Everything read from one drawing, split by how it will be built."""

    items: list[WornItem] = field(default_factory=list)
    raw: str = ""
    source: str = "vlm"

    @property
    def textures(self) -> list[WornItem]:
        return [i for i in self.items if i.construction is Construction.TEXTURE]

    @property
    def geometry(self) -> list[WornItem]:
        return [i for i in self.items if i.construction is Construction.GEOMETRY]

    def summary(self) -> str:
        if not self.items:
            return "nothing worn identified"
        lines = []
        for item in self.items:
            if item.construction is Construction.GEOMETRY:
                lines.append(f"  {item.kind}: GEOMETRY -> {item.socket.value} socket")
            else:
                lines.append(f"  {item.kind}: TEXTURE -> {item.region}")
        return "\n".join(lines)
