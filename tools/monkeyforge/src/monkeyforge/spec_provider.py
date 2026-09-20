import re
from pathlib import Path
from typing import Protocol

from monkeyforge.models import (
    AbilityId,
    AbilitySpec,
    AccessorySpec,
    Archetype,
    BodyMorphs,
    CharacterRequest,
    CharacterSpec,
    Palette,
    Socket,
)
from monkeyforge.skin import extract_skin


class SpecProvider(Protocol):
    async def create_spec(self, request: CharacterRequest, sketch_path: str) -> CharacterSpec: ...


class RuleBasedSpecProvider:
    """Deterministic development provider; replace with a schema-constrained VLM."""

    async def create_spec(self, request: CharacterRequest, sketch_path: str) -> CharacterSpec:
        text = request.description.lower()
        archetype = self._archetype(text)
        ability = self._ability(text, archetype)
        slot, kind = self._accessory(text)
        name = self._name(request.description)

        return CharacterSpec(
            name=name,
            archetype=archetype,
            body_morphs=self._morphs(archetype),
            palette=self._resolve_palette(text, sketch_path),
            accessories=[
                AccessorySpec(
                    id="primary_accessory",
                    slot=slot,
                    kind=kind,
                    description=request.description,
                    target_triangles=2500,
                    part_schema=self._parts(kind),
                )
            ],
            ability=ability,
        )

    @staticmethod
    def _archetype(text: str) -> Archetype:
        if any(word in text for word in ("goalie", "goalkeeper", "keeper")):
            return Archetype.GOALKEEPER
        if any(word in text for word in ("fast", "speed", "runner", "wing")):
            return Archetype.RUNNER
        if any(word in text for word in ("defender", "armor", "shield", "strong")):
            return Archetype.DEFENDER
        if any(word in text for word in ("pass", "coach", "whistle", "playmaker")):
            return Archetype.PLAYMAKER
        return Archetype.BALANCED

    @staticmethod
    def _ability(text: str, archetype: Archetype) -> AbilitySpec:
        if "curve" in text or "spin" in text:
            return AbilitySpec(
                id=AbilityId.CURVE_SHOT,
                magnitude=0.20,
                duration_seconds=1.5,
                cooldown_seconds=14,
            )
        if archetype is Archetype.GOALKEEPER:
            return AbilitySpec(
                id=AbilityId.GOALIE_FOCUS,
                magnitude=0.20,
                duration_seconds=4,
                cooldown_seconds=18,
            )
        if archetype is Archetype.RUNNER:
            return AbilitySpec(
                id=AbilityId.TAILWIND,
                magnitude=0.18,
                duration_seconds=2.5,
                cooldown_seconds=14,
            )
        if archetype is Archetype.DEFENDER:
            return AbilitySpec(
                id=AbilityId.SHIELD_WALL,
                magnitude=0.16,
                duration_seconds=4,
                cooldown_seconds=20,
            )
        if archetype is Archetype.PLAYMAKER:
            return AbilitySpec(
                id=AbilityId.RALLY,
                magnitude=0.14,
                duration_seconds=4,
                cooldown_seconds=18,
            )
        return AbilitySpec()

    @staticmethod
    def _accessory(text: str) -> tuple[Socket, str]:
        if any(word in text for word in ("backpack", "jetpack", "cape")):
            return Socket.BACK, "backpack"
        if any(word in text for word in ("goggle", "glasses", "mask", "visor")):
            return Socket.FACE, "facewear"
        if any(word in text for word in ("staff", "wand", "whistle", "tool")):
            return Socket.HAND_RIGHT, "held_prop"
        return Socket.HEAD_TOP, "hat"

    @staticmethod
    def _name(description: str) -> str:
        words = re.findall(r"[A-Za-z]+", description)
        return (words[0].title() if words else "Monkey")[:40]

    @staticmethod
    def _morphs(archetype: Archetype) -> BodyMorphs:
        presets = {
            Archetype.RUNNER: BodyMorphs(torso_width=0.35, leg_length=0.72),
            Archetype.DEFENDER: BodyMorphs(torso_width=0.78, hand_size=0.66),
            Archetype.GOALKEEPER: BodyMorphs(head_size=0.60, hand_size=0.82),
            Archetype.PLAYMAKER: BodyMorphs(head_size=0.65, ear_size=0.62),
        }
        return presets.get(archetype, BodyMorphs())

    @classmethod
    def _resolve_palette(cls, text: str, sketch_path: str) -> Palette:
        """Colours come from the drawing first, the caption second, defaults last.

        The drawing is the stronger signal: someone who coloured a jersey green meant green, whether
        or not they typed the word. Keyword colours remain the fallback so a caption-only request
        still produces something deliberate.
        """
        keyword = cls._palette_from_text(text)
        reading = extract_skin(Path(sketch_path), fallback=keyword)
        return reading.palette if reading.confident else keyword

    @staticmethod
    def _palette_from_text(text: str) -> Palette:
        if "red" in text:
            return Palette(jersey="#D9493F", accent="#F6C84A")
        if "green" in text:
            return Palette(jersey="#45A65A", accent="#D8ED75")
        if "purple" in text:
            return Palette(jersey="#704CC7", accent="#62D5C9")
        return Palette()

    @staticmethod
    def _parts(kind: str) -> list[str]:
        return {
            "hat": ["fitted shell", "top ornament"],
            "facewear": ["frame", "left lens", "right lens", "strap"],
            "backpack": ["mounting plate", "body", "accent"],
            "held_prop": ["standard grip", "prop body", "accent"],
        }.get(kind, ["body"])

