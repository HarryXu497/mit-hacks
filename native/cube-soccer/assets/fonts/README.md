# Fonts

**M PLUS Rounded 1c** — Black (titles) and Bold (UI). SIL Open Font License 1.1, see `OFL.txt`.

Chosen as the open equivalent of FOT-Rodin, the rounded gothic Fontworks typeface
Animal Crossing uses for its wordmark and dialogue. Same plump strokes and round
terminals; no reserved font name, so it can be used and modified freely.

## These files are subsets

Upstream ships a full Japanese family at ~3.6 MB per weight. Only the Latin range
is needed here, so each face was cut down to ~68 KB with:

    pip install fonttools
    python -m fontTools.subset MPLUSRounded1c-Black.ttf \
      --unicodes="U+0020-007E,U+00A0-00FF,U+0100-017F,U+2010-2015,U+2018-201F,\
U+2022,U+2026,U+2030,U+2039-203A,U+20AC,U+2122,U+2190-2193,U+00D7,U+00F7" \
      --layout-features='*' --name-IDs='*' \
      --output-file=MPLUSRounded1c-Black.ttf

That covers Latin-1, Latin Extended-A, curly quotes, dashes, arrows and currency.
Add ranges to the list and re-run if a language or symbol turns up missing.

## Keep the faces static

Text rendering goes through `ab_glyph`, which reads a single static instance. A
variable font will load without complaint and then render at its default weight,
which is how a title ends up mysteriously thin. Most rounded families on Google
Fonts now ship variable-only; instance them first:

    python -c "from fontTools.ttLib import TTFont; from fontTools.varLib import instancer; \
    f=TTFont('Src[wght].ttf'); instancer.instantiateVariableFont(f,{'wght':900},inplace=True); \
    f.save('Src-Black.ttf')"
