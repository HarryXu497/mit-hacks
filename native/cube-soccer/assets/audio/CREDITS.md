# Audio credits

## Music — third party, CC0

Both tracks are CC0 (public domain dedication) from OpenGameArt. No attribution
is legally required; it is given because it is the decent thing to do.

| File | Track | Author | Source |
|---|---|---|---|
| `music-lobby.ogg` | Feel Good Island Loop | antumdeluge | https://opengameart.org/content/feel-good-island-loop |
| `music-match.ogg` | Jungle Battle Loop | omfgdude | https://opengameart.org/content/jungle-battle-loop |

They replaced two synthesised tracks. Writing listenable music from oscillators
turned out to be beyond what this could do well, and a real recording under a
free licence beats a synthetic one that is merely correct.

## Sound effects — rendered here

The nine effects are synthesised by `tools/compose.py` and are original to this
project. Short, synthetic sounds -- a whistle, a thud, a menu blip -- are a much
easier target than music, and being generated means the whole set can be
re-voiced by changing constants.

`compose.py` still contains the two music routines. They are unused, kept as a
fallback if the CC0 tracks ever need replacing.
