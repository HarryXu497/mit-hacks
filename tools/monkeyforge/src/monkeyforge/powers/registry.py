"""The four superpowers the runtime implements, as data.

The authoritative definition is `src/systems/superpowers.rs` in `HarryXu497/mit-hacks`:

    pub enum SuperpowerKind { BeamBlast, FreezeRay, Boost, Slow }

`onehot_index` there (blast=0, freeze=1, boost=2, slow=3) is written into the RL
observation vector, so it is a compile-time contract with every trained PPO
checkpoint. Reordering this table would silently invalidate them; the indices are
therefore declared explicitly and asserted at import rather than inferred from
position. `cooldown_seconds` mirrors `src/game/config.rs` and is informational here
-- the Rust constants remain authoritative.

`motifs` is the part that does real work. CLIP is far more accurate against an
ensemble of phrasings than against one label, so each power carries the things a
player actually *draws* for it rather than its engineering name. Nobody sketches
"BeamBlast"; they sketch a lightning bolt, a snowflake, a clock.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum


class PowerId(StrEnum):
    BEAM_BLAST = "beam_blast"
    FREEZE_RAY = "freeze_ray"
    BOOST = "boost"
    SLOW = "slow"


@dataclass(frozen=True)
class Motif:
    """One drawable thing, and the colours that thing naturally has.

    `palette` exists because SDXL renders line art as grey extruded plastic unless the
    prompt names colours, but the colours must belong to *the drawn object*, not to the
    power. A spring that maps to Boost is still steel, not gold: the power decides what
    the ability does and what the badge behind it looks like, never what the subject is
    made of. Two players who draw different things for the same power must get
    different artwork, which is the whole point of drawing it.
    """

    phrase: str
    palette: str


@dataclass(frozen=True)
class Power:
    """One runtime superpower and everything the icon pipeline needs to know about it."""

    id: PowerId
    rust_name: str
    onehot_index: int
    display_name: str
    effect: str
    cooldown_seconds: float
    # Drives the badge backdrop only -- how a player tells Boost from Slow at a glance.
    accent: str
    motifs: tuple[Motif, ...]

    @property
    def label(self) -> str:
        """Short human-facing name, for 'did you mean ...?' prompts."""
        return self.display_name


POWERS: tuple[Power, ...] = (
    Power(
        id=PowerId.BEAM_BLAST,
        rust_name="BeamBlast",
        onehot_index=0,
        display_name="Beam Blast",
        effect="Shoves every opponent in a 30-degree, 8 m cone away from you.",
        cooldown_seconds=5.0,
        accent="#F0483C",
        motifs=(
            Motif("a laser beam firing forward", "vivid red with a white hot core"),
            Motif("an energy blast", "bright orange and yellow energy"),
            Motif("a shockwave pushing outwards", "pale blue and white"),
            Motif("a cone of force", "translucent cyan and white"),
            Motif("an explosion", "orange, yellow and red flame"),
            Motif("a ray gun", "gunmetal grey with red accents"),
            Motif("a glowing fist punching", "warm tan skin with a golden glow"),
            Motif("concentric blast rings", "bright orange and white"),
            Motif("a cannon firing", "dark iron black with a muzzle flash"),
            Motif("a starburst impact", "brilliant white and yellow"),
        ),
    ),
    Power(
        id=PowerId.FREEZE_RAY,
        rust_name="FreezeRay",
        onehot_index=1,
        display_name="Freeze Ray",
        effect="Freezes the nearest opponent in a narrow 10 m cone solid for 2 s.",
        cooldown_seconds=8.0,
        accent="#3FC6F0",
        motifs=(
            Motif("a snowflake", "white and pale ice blue"),
            Motif("an ice crystal", "translucent cyan and white"),
            Motif("icicles hanging down", "clear pale blue ice"),
            Motif("a frozen ice cube", "clear glassy blue ice"),
            Motif("a freeze ray beam", "bright cyan with a white core"),
            Motif("a block of ice", "translucent pale blue"),
            Motif("frost spreading", "white and silver frost"),
            Motif("a snowman", "white snow with an orange carrot and black coal"),
            Motif("a thermometer reading cold", "clear glass with blue liquid and silver"),
            Motif("a blue ice shard", "deep blue and white"),
        ),
    ),
    Power(
        id=PowerId.BOOST,
        rust_name="Boost",
        onehot_index=2,
        display_name="Boost",
        effect="Multiplies your own speed and acceleration by 1.5x for 2 s.",
        cooldown_seconds=10.0,
        accent="#F5C518",
        motifs=(
            Motif("a lightning bolt", "bright yellow and white"),
            # The case that named the rule: a spring is steel, not Boost-gold.
            Motif("a coiled spring", "polished steel and chrome"),
            Motif("a rocket taking off", "a red and white hull with an orange flame"),
            Motif("speed lines trailing behind", "bright cyan and white"),
            Motif("a winged running shoe", "a red shoe with white wings"),
            Motif("an upward arrow", "bright green and white"),
            Motif("a flame trail", "orange, yellow and red fire"),
            Motif("a sprinting figure", "a blue athletic kit"),
            Motif("a speedometer at maximum", "a black dial with a red needle and chrome"),
            Motif("a turbo booster", "gunmetal grey with a blue flame"),
        ),
    ),
    Power(
        id=PowerId.SLOW,
        rust_name="Slow",
        onehot_index=3,
        display_name="Slow",
        effect="Drags the nearest opponent within 12 m down to 0.4x speed for 3 s.",
        cooldown_seconds=8.0,
        accent="#8A5BD6",
        motifs=(
            Motif("an hourglass", "a warm wood frame with golden sand"),
            Motif("a clock face", "a white dial with black hands and a brass rim"),
            Motif("a snail", "a spiral brown shell with a soft green body"),
            Motif("a tortoise", "a green shell with olive skin"),
            Motif("sticky tar or slime", "glossy black tar"),
            Motif("a heavy anchor", "rusted iron grey"),
            Motif("a ball and chain", "dark iron grey"),
            Motif("a downward arrow", "deep blue and white"),
            Motif("thick dripping goo", "glossy green slime"),
            Motif("a stopwatch", "polished chrome with a white dial"),
        ),
    ),
)

_BY_ID: dict[PowerId, Power] = {power.id: power for power in POWERS}
_BY_ONEHOT: dict[int, Power] = {power.onehot_index: power for power in POWERS}


def by_id(power_id: PowerId | str) -> Power:
    """Look a power up by its stable string id."""
    try:
        return _BY_ID[PowerId(power_id)]
    except (KeyError, ValueError) as exc:
        known = ", ".join(sorted(p.id for p in POWERS))
        raise KeyError(f"unknown power {power_id!r}; known powers are {known}") from exc


def by_onehot(index: int) -> Power:
    """Look a power up by the observation-vector index the Rust enum assigns it."""
    try:
        return _BY_ONEHOT[index]
    except KeyError as exc:
        raise KeyError(f"no power has one-hot index {index}; expected 0..3") from exc


_BY_PHRASE: dict[str, Motif] = {m.phrase: m for power in POWERS for m in power.motifs}


def motif_by_phrase(phrase: str) -> Motif:
    """Recover a motif (and so its palette) from the phrase the classifier returned."""
    try:
        return _BY_PHRASE[phrase]
    except KeyError as exc:
        raise KeyError(f"unknown motif {phrase!r}") from exc


def onehot(power_id: PowerId | str) -> list[float]:
    """The 4-wide one-hot slice the runtime appends to its observation vector."""
    hot = by_id(power_id).onehot_index
    return [1.0 if i == hot else 0.0 for i in range(len(POWERS))]


def _check_table() -> None:
    """Fail at import if the table drifts from the Rust contract it mirrors."""
    if len(_BY_ID) != len(POWERS):
        raise RuntimeError("duplicate PowerId in POWERS")
    phrases = [motif.phrase for power in POWERS for motif in power.motifs]
    if len(set(phrases)) != len(phrases):
        raise RuntimeError("a motif phrase is claimed by two powers; prototypes would collide")
    if sorted(_BY_ONEHOT) != list(range(len(POWERS))):
        raise RuntimeError(f"one-hot indices must be 0..{len(POWERS) - 1} with no gaps")
    expected = {0: "BeamBlast", 1: "FreezeRay", 2: "Boost", 3: "Slow"}
    actual = {power.onehot_index: power.rust_name for power in POWERS}
    if actual != expected:
        raise RuntimeError(
            "one-hot order no longer matches SuperpowerKind::onehot_index in mit-hacks; "
            f"expected {expected}, got {actual}. Every PPO checkpoint depends on this order."
        )


_check_table()
