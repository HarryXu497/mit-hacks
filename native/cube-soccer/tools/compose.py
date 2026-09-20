"""Renders the game's music and sound effects.

The score lives here as code rather than as recordings, so a change to the
arrangement is a diff. Run it to regenerate everything in `assets/audio`:

    pip install numpy
    python tools/compose.py

This is chip music: pulse, triangle and noise, the palette of a console sound
chip. That is a deliberate choice and it replaced an earlier attempt at an
acoustic jazz trio. Sine waves pretending to be an upright bass and a vibraphone
sound like neither -- the appeal of that style is the timbre of real instruments
and there is no way to fake it from first principles. A square wave is not
pretending to be anything, so it can simply sound good, and it suits a game made
of blocks.

Three things matter more than the waveforms, and all three were missing first
time round:

- A tune. Not chord tones in sequence, but a motif that states itself, answers
  itself and comes back. If you cannot hum it, it is wallpaper.
- Groove. Off-beats land late (swing) and no two notes share a velocity. Music
  quantised dead to the grid ticks rather than breathes.
- Arpeggios instead of pads. A held chord on a sound chip is three fast
  alternating notes. It is the sound of the format and it keeps the texture
  moving underneath a slow melody.

Everything is rendered at a whole number of bars and the decaying tail of the
final notes is wrapped back onto the head, so the loop has no click and no gap.

Mono at 32 kHz: the material is narrow-band by design, stereo would double the
bytes in the repository for very little, and WAV is the only format the engine
can read without another dependency.
"""

from __future__ import annotations

import pathlib
import wave

import numpy as np

RATE = 32_000
OUT = pathlib.Path(__file__).resolve().parent.parent / "assets" / "audio"

# Equal temperament from A4. Note names are parsed as "C#4", "Bb3", "F4".
STEPS = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}

# How late an off-beat lands, as a fraction of an eighth note. Straight is 0.5;
# this is the shuffle that stops the grid from ticking.
SWING = 0.58


def hz(note: str) -> float:
    name = note[0].upper()
    i = 1
    semitone = STEPS[name]
    while i < len(note) and note[i] in "#b":
        semitone += 1 if note[i] == "#" else -1
        i += 1
    octave = int(note[i:])
    return 440.0 * 2 ** ((semitone + (octave - 4) * 12 - 9) / 12)


def _edges(x: np.ndarray, rise: int = 24, fall: int = 120) -> np.ndarray:
    """Takes the click off the start and end of a clip."""
    if x.size > rise + fall:
        x[:rise] *= np.linspace(0.0, 1.0, rise)
        x[-fall:] *= np.linspace(1.0, 0.0, fall)
    return x


def pulse(
    freq: float,
    dur: float,
    gain: float = 1.0,
    duty: float = 0.5,
    vibrato: float = 0.0,
    decay: float = 0.9,
) -> np.ndarray:
    """A pulse wave: the lead and harmony voice of every sound chip.

    Duty is what gives the channel its character -- a half cycle is hollow and
    square, an eighth is thin and reedy, and moving between them is most of how
    a chip tune gets more than one sound out of one oscillator.

    Vibrato is applied only after a note has had time to sound, which is how a
    player would do it and what keeps short notes crisp.
    """
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    if vibrato > 0.0:
        onset = np.clip((t - 0.12) * 5.0, 0.0, 1.0)
        bend = 1.0 + np.sin(2 * np.pi * 5.6 * t) * vibrato * onset
    else:
        bend = 1.0
    phase = np.cumsum(freq * bend) / RATE
    wave_ = np.where((phase % 1.0) < duty, 1.0, -1.0)
    # A chip envelope is a handful of volume steps, not a smooth curve.
    steps = np.array([1.0, 0.86, 0.74, 0.66, 0.6, 0.55])
    idx = np.minimum((t * 26).astype(int), steps.size - 1)
    shape = steps[idx] * np.exp(-decay * t)
    return _edges(wave_ * shape * gain * 0.32)


def triangle(freq: float, dur: float, gain: float = 1.0, decay: float = 1.4) -> np.ndarray:
    """Triangle wave: the bass voice. Rounder than a pulse and sits underneath."""
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    phase = (freq * t) % 1.0
    wave_ = 4.0 * np.abs(phase - 0.5) - 1.0
    return _edges(wave_ * np.exp(-decay * t) * gain * 0.5)


def noise(dur: float, gain: float = 1.0, tone: float = 1.0, decay: float = 28.0, seed: int = 0) -> np.ndarray:
    """The noise channel: hats, snares and anything that hisses.

    `tone` runs 0 (dark, snare body) to 1 (bright, closed hat), implemented by
    subtracting a running mean, which is a crude but adequate high pass.
    """
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    raw = np.random.default_rng(seed).normal(0, 1, n)
    k = max(2, int(2 + (1.0 - tone) * 26))
    smoothed = np.convolve(raw, np.ones(k) / k, mode="same")
    shaped = raw - smoothed if tone > 0.5 else smoothed
    return _edges(shaped * np.exp(-decay * t) * gain * 0.5)


def kick(dur: float = 0.22, gain: float = 1.0) -> np.ndarray:
    """Pitch drops fast from a click to a thump."""
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    sweep = 48.0 + 150.0 * np.exp(-34.0 * t)
    phase = 2 * np.pi * np.cumsum(sweep) / RATE
    return _edges((np.sin(phase) + np.sign(np.sin(phase)) * 0.25) * np.exp(-13.0 * t) * gain * 0.8)


def snare(dur: float = 0.19, gain: float = 1.0, seed: int = 4) -> np.ndarray:
    body = noise(dur, 1.0, tone=0.62, decay=26.0, seed=seed)
    n = body.size
    t = np.arange(n) / RATE
    ring = np.sin(2 * np.pi * 196.0 * t) * np.exp(-30.0 * t) * 0.35
    return _edges((body + ring) * gain * 0.7)


def echo(x: np.ndarray, delay: float = 0.19, feedback: float = 0.26, taps: int = 3) -> np.ndarray:
    """A tempo-ish slapback. Chip music uses delay where a band would use a room."""
    out = x.copy()
    for i in range(1, taps + 1):
        d = int(delay * i * RATE)
        if d < out.size:
            out[d:] += x[:-d] * (feedback**i)
    return out


class Track:
    """A bar grid, a swing feel, and a loop seam that actually joins."""

    def __init__(self, bpm: float, bars: int, beats_per_bar: int = 4, swing: float = SWING):
        self.beat = 60.0 / bpm
        self.bars = bars
        self.beats_per_bar = beats_per_bar
        self.swing = swing
        self.length = int(bars * beats_per_bar * self.beat * RATE)
        # Rendered long, then folded: a note struck in the last bar has to ring
        # over the seam into the first, or the loop ticks once a cycle.
        self.buffer = np.zeros(self.length + RATE * 3)

    def at(self, bar: float, beat: float) -> int:
        """Beat position in samples, with off-beat eighths pushed late."""
        whole = int(beat)
        frac = beat - whole
        if abs(frac - 0.5) < 1e-6:
            frac = self.swing
        return int((bar * self.beats_per_bar + whole + frac) * self.beat * RATE)

    def add(self, sample: np.ndarray, bar: float, beat: float = 0.0):
        start = self.at(bar, beat)
        end = min(start + sample.size, self.buffer.size)
        if end > start:
            self.buffer[start:end] += sample[: end - start]

    def finish(self, peak: float = 0.84) -> np.ndarray:
        body = self.buffer[: self.length].copy()
        tail = self.buffer[self.length :]
        body[: tail.size] += tail[: body.size]
        top = np.abs(body).max()
        return body * (peak / top) if top > 0 else body


def polish(x: np.ndarray, peak: float = 0.90) -> np.ndarray:
    """Normalise a one-shot and make sure it starts and ends at zero.

    Both halves matter. Synthesised transients overshoot and clip on the attack,
    which is heard as a crackle rather than as loudness; and a sound cut off
    while still ringing ends on a step, which is a click every time it plays.
    """
    out = x.astype(float).copy()
    top = np.abs(out).max()
    if top > 0:
        out *= peak / top
    rise = max(1, int(0.002 * RATE))
    fall = max(1, int(0.010 * RATE))
    out[:rise] *= np.linspace(0.0, 1.0, rise)
    out[-fall:] *= np.linspace(1.0, 0.0, fall)
    return out


def write(name: str, samples: np.ndarray):
    OUT.mkdir(parents=True, exist_ok=True)
    pcm = (np.clip(samples, -1.0, 1.0) * 32767).astype("<i2")
    path = OUT / name
    with wave.open(str(path), "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(RATE)
        f.writeframes(pcm.tobytes())
    print(f"{name:24s} {path.stat().st_size / 1024:7.0f} KB  {samples.size / RATE:5.1f}s")


# ---------------------------------------------------------------------------
# Shared arrangement helpers
# ---------------------------------------------------------------------------

# Velocities cycle rather than randomise, so the same bar sounds the same every
# time round the loop while no two adjacent notes match.
VELOCITY = [1.0, 0.82, 0.92, 0.76, 0.98, 0.84]


def arpeggio(t: Track, bar: int, notes: list[str], gain: float, duty: float = 0.5, rate: float = 0.25):
    """A chord, played as a sound chip plays one: fast alternating notes."""
    steps = int(t.beats_per_bar / rate)
    for i in range(steps):
        note = notes[i % len(notes)]
        t.add(
            pulse(hz(note), rate * t.beat * 1.05, gain * (0.85 if i % 2 else 1.0), duty, decay=5.0),
            bar,
            i * rate,
        )


def walking_bass(t: Track, bar: int, pattern: list[tuple[float, str]], gain: float = 0.9):
    for i, (beat, note) in enumerate(pattern):
        t.add(triangle(hz(note), t.beat * 0.55, gain * VELOCITY[i % len(VELOCITY)]), bar, beat)


def backbeat(t: Track, bar: int, hats: bool = True, fill: bool = False):
    t.add(kick(gain=0.95), bar, 0.0)
    t.add(kick(gain=0.72), bar, 2.5)
    t.add(snare(gain=0.62, seed=bar), bar, 1.0)
    t.add(snare(gain=0.66, seed=bar + 40), bar, 3.0)
    if hats:
        for i, beat in enumerate((0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5)):
            t.add(noise(0.055, 0.30 * VELOCITY[i % len(VELOCITY)], tone=0.95, decay=60.0, seed=bar * 8 + i), bar, beat)
    if fill:
        for i, beat in enumerate((3.0, 3.25, 3.5, 3.75)):
            t.add(snare(0.12, 0.5 + i * 0.12, seed=bar * 3 + i), bar, beat)


# ---------------------------------------------------------------------------
# The lobby theme
# ---------------------------------------------------------------------------

def lobby() -> np.ndarray:
    """Bright, mid-tempo, C major. Eight bars: a phrase, its answer, a lift, a
    close that lands on the dominant so bar one resolves it.

    The melody is written to be singable and to keep returning to the same
    three-note shape, which is what makes a loop feel like a tune rather than a
    sequence of correct notes.
    """
    t = Track(bpm=112, bars=8)

    progression = [
        ("C", ["C4", "E4", "G4"], [(0.0, "C2"), (1.0, "G2"), (2.0, "C3"), (3.0, "E3")]),
        ("Am", ["A3", "C4", "E4"], [(0.0, "A1"), (1.0, "E2"), (2.0, "A2"), (3.0, "C3")]),
        ("F", ["F3", "A3", "C4"], [(0.0, "F1"), (1.0, "C2"), (2.0, "F2"), (3.0, "A2")]),
        ("G", ["G3", "B3", "D4"], [(0.0, "G1"), (1.0, "D2"), (2.0, "G2"), (3.0, "B2")]),
        ("C", ["C4", "E4", "G4"], [(0.0, "C2"), (1.0, "G2"), (2.0, "C3"), (3.0, "E3")]),
        ("Am", ["A3", "C4", "E4"], [(0.0, "A1"), (1.0, "E2"), (2.0, "A2"), (3.0, "C3")]),
        ("F", ["F3", "A3", "C4"], [(0.0, "F1"), (1.0, "C2"), (2.0, "F2"), (3.0, "A2")]),
        ("G7", ["G3", "B3", "F4"], [(0.0, "G1"), (1.0, "D2"), (2.0, "F2"), (3.0, "G2")]),
    ]

    for bar, (_, chord, bass_line) in enumerate(progression):
        arpeggio(t, bar, chord, gain=0.30, duty=0.5, rate=0.25)
        walking_bass(t, bar, bass_line, gain=0.95)
        backbeat(t, bar, hats=True, fill=(bar == 7))

    # The tune. Bars 1-2 state it, 3-4 answer, 5-6 lift, 7-8 come home.
    melody = [
        (0, 0.0, "C5", 0.5), (0, 0.5, "E5", 0.5), (0, 1.0, "G5", 1.0),
        (0, 2.0, "E5", 0.5), (0, 2.5, "G5", 0.5), (0, 3.0, "A5", 1.0),
        (1, 0.0, "G5", 1.5), (1, 1.5, "E5", 0.5), (1, 2.0, "C5", 1.0), (1, 3.0, "D5", 1.0),
        (2, 0.0, "F5", 0.5), (2, 0.5, "A5", 0.5), (2, 1.0, "C6", 1.0),
        (2, 2.0, "A5", 0.5), (2, 2.5, "G5", 0.5), (2, 3.0, "F5", 1.0),
        (3, 0.0, "E5", 2.0), (3, 2.0, "D5", 1.0), (3, 3.0, "C5", 1.0),
        (4, 0.0, "E5", 0.5), (4, 0.5, "F5", 0.5), (4, 1.0, "G5", 1.0),
        (4, 2.0, "A5", 1.0), (4, 3.0, "G5", 1.0),
        (5, 0.0, "E5", 1.0), (5, 1.0, "C5", 1.0), (5, 2.0, "D5", 2.0),
        (6, 0.0, "F5", 0.5), (6, 0.5, "G5", 0.5), (6, 1.0, "A5", 1.0),
        (6, 2.0, "G5", 1.0), (6, 3.0, "F5", 1.0),
        (7, 0.0, "E5", 1.0), (7, 1.0, "D5", 1.0), (7, 2.0, "G4", 2.0),
    ]
    for i, (bar, beat, note, dur) in enumerate(melody):
        held = dur >= 1.5
        t.add(
            pulse(
                hz(note),
                dur * t.beat * 0.96,
                0.80 * VELOCITY[i % len(VELOCITY)],
                duty=0.25,
                vibrato=0.012 if held else 0.0,
                decay=0.7,
            ),
            bar,
            beat,
        )

    return echo(t.finish(0.82), delay=t.beat * 0.75, feedback=0.16, taps=2)


# ---------------------------------------------------------------------------
# The match theme
# ---------------------------------------------------------------------------

def match() -> np.ndarray:
    """A minor, fast, and syncopated. Same three-note shape as the lobby tune,
    turned minor and bitten off short, so kickoff is recognisably the same game
    at a higher gear.
    """
    t = Track(bpm=152, bars=8, swing=0.52)

    progression = [
        (["A3", "C4", "E4"], [(0.0, "A1"), (0.5, "A1"), (1.5, "E2"), (2.0, "A2"), (3.0, "G2")]),
        (["A3", "C4", "E4"], [(0.0, "A1"), (0.5, "A1"), (1.5, "C2"), (2.0, "E2"), (3.0, "A2")]),
        (["F3", "A3", "C4"], [(0.0, "F1"), (0.5, "F1"), (1.5, "C2"), (2.0, "F2"), (3.0, "A2")]),
        (["G3", "B3", "D4"], [(0.0, "G1"), (0.5, "G1"), (1.5, "D2"), (2.0, "G2"), (3.0, "B2")]),
        (["A3", "C4", "E4"], [(0.0, "A1"), (0.5, "A1"), (1.5, "E2"), (2.0, "A2"), (3.0, "G2")]),
        (["A3", "C4", "E4"], [(0.0, "A1"), (0.5, "A1"), (1.5, "C2"), (2.0, "E2"), (3.0, "A2")]),
        (["F3", "A3", "C4"], [(0.0, "F1"), (0.5, "F1"), (1.5, "C2"), (2.0, "F2"), (3.0, "A2")]),
        (["E3", "G#3", "D4"], [(0.0, "E2"), (0.5, "E2"), (1.5, "B1"), (2.0, "E2"), (3.0, "G#2")]),
    ]

    for bar, (chord, bass_line) in enumerate(progression):
        # Sixteenth arpeggios: the engine of the track.
        arpeggio(t, bar, chord, gain=0.26, duty=0.125, rate=0.25)
        walking_bass(t, bar, bass_line, gain=1.0)
        backbeat(t, bar, hats=True, fill=(bar % 4 == 3))

    melody = [
        (0, 0.0, "A5", 0.5), (0, 0.5, "C6", 0.5), (0, 1.0, "E6", 0.75),
        (0, 2.0, "C6", 0.5), (0, 2.5, "A5", 0.5), (0, 3.0, "B5", 1.0),
        (1, 0.0, "C6", 1.0), (1, 1.5, "A5", 0.5), (1, 2.0, "E5", 1.5),
        (2, 0.0, "F5", 0.5), (2, 0.5, "A5", 0.5), (2, 1.0, "C6", 0.75),
        (2, 2.0, "A5", 0.5), (2, 2.5, "F5", 0.5), (2, 3.0, "G5", 1.0),
        (3, 0.0, "B5", 1.0), (3, 1.5, "G5", 0.5), (3, 2.0, "D5", 1.5),
        (4, 0.0, "A5", 0.5), (4, 0.5, "C6", 0.5), (4, 1.0, "E6", 1.0),
        (4, 2.0, "D6", 0.5), (4, 2.5, "C6", 0.5), (4, 3.0, "B5", 1.0),
        (5, 0.0, "A5", 1.5), (5, 2.0, "C6", 0.5), (5, 2.5, "B5", 0.5), (5, 3.0, "A5", 1.0),
        (6, 0.0, "C6", 0.5), (6, 0.5, "A5", 0.5), (6, 1.0, "F5", 1.0),
        (6, 2.0, "G5", 0.5), (6, 2.5, "A5", 0.5), (6, 3.0, "C6", 1.0),
        (7, 0.0, "B5", 1.0), (7, 1.0, "G#5", 1.0), (7, 2.0, "E5", 2.0),
    ]
    for i, (bar, beat, note, dur) in enumerate(melody):
        t.add(
            pulse(
                hz(note),
                dur * t.beat * 0.92,
                0.78 * VELOCITY[i % len(VELOCITY)],
                duty=0.5,
                vibrato=0.014 if dur >= 1.5 else 0.0,
                decay=1.1,
            ),
            bar,
            beat,
        )

    return echo(t.finish(0.86), delay=t.beat * 0.5, feedback=0.14, taps=2)


# ---------------------------------------------------------------------------
# Sound effects
# ---------------------------------------------------------------------------

def sfx_move() -> np.ndarray:
    return pulse(hz("E5"), 0.07, 0.9, duty=0.25, decay=14.0)


def sfx_select() -> np.ndarray:
    out = np.zeros(int(0.30 * RATE))
    for i, note in enumerate(["C5", "E5", "G5", "C6"]):
        at = int(i * 0.045 * RATE)
        blip = pulse(hz(note), 0.14, 0.9, duty=0.25, decay=9.0)
        end = min(at + blip.size, out.size)
        out[at:end] += blip[: end - at]
    return out


def sfx_back() -> np.ndarray:
    out = np.zeros(int(0.22 * RATE))
    for i, note in enumerate(["G4", "D4"]):
        at = int(i * 0.055 * RATE)
        blip = pulse(hz(note), 0.13, 0.8, duty=0.5, decay=13.0)
        end = min(at + blip.size, out.size)
        out[at:end] += blip[: end - at]
    return out


def sfx_kick_ball() -> np.ndarray:
    """A short noise burst over a falling square: a chip's idea of an impact."""
    n = int(0.16 * RATE)
    t = np.arange(n) / RATE
    sweep = 90.0 + 420.0 * np.exp(-50.0 * t)
    phase = 2 * np.pi * np.cumsum(sweep) / RATE
    tone = np.sign(np.sin(phase)) * np.exp(-24.0 * t)
    return _edges(tone * 0.7 + noise(0.16, 0.8, tone=0.5, decay=46.0, seed=9))


def sfx_whistle() -> np.ndarray:
    """Two close pulses beating against each other, warbling like a pea whistle."""
    n = int(0.42 * RATE)
    t = np.arange(n) / RATE
    warble = np.sin(2 * np.pi * 24.0 * t) * 36.0
    a = np.sign(np.sin(2 * np.pi * np.cumsum(2_700 + warble) / RATE))
    b = np.sign(np.sin(2 * np.pi * np.cumsum(3_050 + warble) / RATE)) * 0.7
    shape = np.clip(t / 0.02, 0, 1) * np.clip((0.42 - t) / 0.08, 0, 1)
    return _edges((a + b) * shape * 0.3)


def sfx_goal() -> np.ndarray:
    """A rising fanfare over a drum roll. No crowd: filtered noise pretending to
    be a stadium was the weakest thing in the old set, and a chip tune would
    never have tried it."""
    n = int(1.5 * RATE)
    out = np.zeros(n)
    run = ["C5", "E5", "G5", "C6", "E6", "G6", "C7"]
    for i, note in enumerate(run):
        at = int((0.04 + i * 0.062) * RATE)
        blip = pulse(hz(note), 0.5, 0.85, duty=0.25, decay=4.0)
        end = min(at + blip.size, n)
        out[at:end] += blip[: end - at]
    # The chord it lands on, held.
    for note in ["C6", "E6", "G6"]:
        at = int(0.48 * RATE)
        held = pulse(hz(note), 0.95, 0.42, duty=0.5, vibrato=0.02, decay=1.6)
        end = min(at + held.size, n)
        out[at:end] += held[: end - at]
    for i in range(14):
        at = int(i * 0.031 * RATE)
        roll = snare(0.09, 0.28 + i * 0.03, seed=i)
        end = min(at + roll.size, n)
        out[at:end] += roll[: end - at]
    return echo(out, delay=0.14, feedback=0.22, taps=2)


def sfx_whoosh() -> np.ndarray:
    """The camera flight: noise swept from dark to bright and back."""
    n = int(0.9 * RATE)
    t = np.arange(n) / RATE
    raw = np.random.default_rng(33).normal(0, 1, n)
    out = np.zeros(n)
    chunk = n // 32
    for i in range(32):
        s, e = i * chunk, min((i + 1) * chunk, n)
        k = max(2, int(2 + 40 * abs(0.5 - i / 32) * 2))
        out[s:e] = np.convolve(raw[s:e], np.ones(k) / k, mode="same")
    return out * np.sin(np.pi * np.clip(t / (n / RATE), 0, 1)) ** 1.4 * 0.5


def main():
    # The two music tracks are third-party CC0 Vorbis files, not rendered here.
    # See assets/audio/CREDITS.md. `lobby()` and `match()` are kept because they
    # still render and are the fallback if those ever need replacing.
    write("sfx-menu-move.wav", polish(sfx_move()))
    write("sfx-menu-select.wav", polish(sfx_select()))
    write("sfx-menu-back.wav", polish(sfx_back()))
    write("sfx-kick.wav", polish(sfx_kick_ball()))
    write("sfx-whistle.wav", polish(sfx_whistle()))
    write("sfx-goal.wav", polish(sfx_goal()))
    write("sfx-whoosh.wav", polish(sfx_whoosh()))


if __name__ == "__main__":
    main()
