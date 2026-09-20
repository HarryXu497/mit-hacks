from monkeyforge.models import Socket
from monkeyforge.wardrobe import Construction, Wardrobe, WornItem


def test_a_cap_becomes_geometry_on_the_head_socket() -> None:
    # A cap is silhouette. Painting it onto a head texture gives a hat-coloured head, so it must
    # become a mesh -- this is the routing that makes the socket system and the generator worth
    # having at all.
    cap = WornItem(kind="cap", colour="red", text="MIT")
    assert cap.construction is Construction.GEOMETRY
    assert cap.socket is Socket.HEAD_TOP


def test_a_jersey_becomes_a_texture_not_geometry() -> None:
    # A jersey lies flat against the torso and has no silhouette of its own; generating a mesh for
    # it would fight the base body.
    jersey = WornItem(kind="jersey", colour="cardinal red", text="MIT")
    assert jersey.construction is Construction.TEXTURE
    assert jersey.socket is None
    assert jersey.region == "torso"


def test_accessories_route_to_their_own_sockets() -> None:
    assert WornItem(kind="backpack").socket is Socket.BACK
    assert WornItem(kind="goggles").socket is Socket.FACE
    assert WornItem(kind="shield").socket is Socket.HAND_LEFT
    assert WornItem(kind="belt").socket is Socket.WAIST


def test_kind_matching_tolerates_spacing_and_case() -> None:
    assert WornItem(kind="  Back-Pack ").socket is Socket.BACK
    assert WornItem(kind="T Shirt").construction is Construction.TEXTURE


def test_unknown_items_default_to_texture_rather_than_geometry() -> None:
    # Generating geometry for something unrecognised risks a floating blob on the character;
    # defaulting to a torso texture fails quietly instead.
    assert WornItem(kind="poncho").construction is Construction.TEXTURE


def test_wardrobe_splits_the_two_paths() -> None:
    wardrobe = Wardrobe(items=[
        WornItem(kind="cap", colour="red", text="MIT"),
        WornItem(kind="jersey", colour="cardinal", text="MIT"),
        WornItem(kind="backpack", colour="green"),
    ])
    assert [i.kind for i in wardrobe.geometry] == ["cap", "backpack"]
    assert [i.kind for i in wardrobe.textures] == ["jersey"]


def test_describe_is_shared_by_both_paths() -> None:
    # Both halves prompt from the same description so a generated cap and a painted jersey do not
    # drift apart in colour or style.
    item = WornItem(kind="cap", colour="red", secondary_colour="grey")
    assert "red cap" in item.describe()
    assert "grey trim" in item.describe()
