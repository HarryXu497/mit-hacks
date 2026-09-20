import pytest
from pydantic import ValidationError

from monkeyforge.models import AbilitySpec, BodyMorphs, Palette


def test_body_morphs_are_bounded() -> None:
    with pytest.raises(ValidationError):
        BodyMorphs(head_size=1.1)


def test_palette_normalizes_hex_case() -> None:
    assert Palette(fur="#aa22cc").fur == "#AA22CC"


def test_ability_parameters_are_bounded() -> None:
    with pytest.raises(ValidationError):
        AbilitySpec(magnitude=0.8)

