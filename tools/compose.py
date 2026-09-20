"""Renders the game's music and sound effects.

The score lives here as code rather than as recordings, so a change to the
arrangement is a diff. Run it to regenerate everything in `assets/audio`:

    pip install numpy
    python tools/compose.py

Design notes, from how the games this one is aiming at handle music:

- A menu loop has to survive being heard for minutes at a time, so the lobby
  theme is written the way Animal Crossing writes: jazz harmony, ii-V-I, gentle
  tempo, and a last bar that lands on the dominant so the seam back to bar one
  is a resolution rather than a restart.
- Match music is the same harmony taken faster and given drums. Sharing the
  tonal centre with the lobby is what makes the drop into kickoff feel like the
  same place rather than a different soundtrack.
- Everything is rendered at a whole number of bars and the decaying tail of the
  final notes is wrapped back onto the head, so the loop has no click and no
  gap. This is the part that is easy to skip and immediately audible.

Mono at 32 kHz: the material is synthetic and narrow-band, stereo would double
the bytes in the repository for very little, and WAV is the only format the
engine can read without another dependency.
"""

from __future__ import annotations

import math
import pathlib
import struct
import wave

import numpy as np

RATE = 32_000
OUT = pathlib.Path(__file__).resolve().parent.parent / "assets" / "audio"

# Equal temperament from A4. Note names are parsed as "C#4", "Bb3", "F4".
STEPS = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def hz(note: str) -> float:
    name = note[0].upper()
    i = 1
    semitone = STEPS[name]
    while i < len(note) and note[i] in "#b":
        semitone += 1 if note[i] == "#" else -1
        i += 1
    octave = int(note[i:])
    return 440.0 * 2 ** ((semitone + (octave - 4) * 12 - 9) / 12)


def env(n: int, attack: float, decay: float, sustain: float = 0.0, release: float = 0.0) -> np.ndarray:
    """A four-stage envelope, in seconds, fitted into `n` samples."""
    a = max(1, int(attack * RATE))
    d = max(1, int(decay * RATE))
    r = max(1, int(release * RATE))
    s = max(0, n - a - d - r)
    out = np.concatenate(
        [
            np.linspace(0.0, 1.0, a),
            np.linspace(1.0, sustain, d),
            np.full(s, sustain),
            np.linspace(sustain, 0.0, r),
        ]
    )
    return out[:n] if out.size >= n else np.pad(out, (0, n - out.size))


def mallet(freq: float, dur: float, gain: float = 1.0, bright: float = 1.0) -> np.ndarray:
    """A struck wooden bar: marimba and its softer cousin, the vibraphone.

    Three partials rather than a full model. A real bar is tuned so its first
    overtone is two octaves up, which is what separates the sound from a plain
    sine and is most of why it reads as wood rather than as a synthesiser.
    """
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    body = np.sin(2 * np.pi * freq * t) * np.exp(-3.2 * t)
    fourth = np.sin(2 * np.pi * freq * 4.0 * t) * np.exp(-7.0 * t) * 0.34 * bright
    tenth = np.sin(2 * np.pi * freq * 9.8 * t) * np.exp(-14.0 * t) * 0.12 * bright
    # The strike itself: a click of noise, gone in 8 ms.
    knock = np.random.default_rng(int(freq)).normal(0, 1, n) * np.exp(-380.0 * t) * 0.05
    return (body + fourth + tenth + knock) * gain


def bass(freq: float, dur: float, gain: float = 1.0) -> np.ndarray:
    """Upright-ish: a sine with a little grit and a fast thumb attack."""
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    tone = np.sin(2 * np.pi * freq * t)
    second = np.sin(2 * np.pi * freq * 2 * t) * 0.18
    third = np.sin(2 * np.pi * freq * 3 * t) * 0.07
    shape = env(n, 0.006, 0.10, 0.55, max(0.05, dur - 0.14))
    return (tone + second + third) * shape * gain


def pad(freqs: list[float], dur: float, gain: float = 1.0) -> np.ndarray:
    """A breathy chord bed. Slightly detuned pairs so it moves on its own."""
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    out = np.zeros(n)
    for f in freqs:
        for cents in (-6.0, 6.0):
            out += np.sin(2 * np.pi * f * 2 ** (cents / 1200) * t)
    out /= max(1, len(freqs) * 2)
    return out * env(n, 0.35, 0.2, 0.8, 0.5) * gain


def shaker(dur: float, gain: float = 1.0, seed: int = 0) -> np.ndarray:
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    noise = np.random.default_rng(seed).normal(0, 1, n)
    # Crude high-pass: subtract a running mean, which is enough to take the
    # body out of white noise and leave the hiss.
    k = 12
    smoothed = np.convolve(noise, np.ones(k) / k, mode="same")
    return (noise - smoothed) * np.exp(-38.0 * t) * gain


def kick(dur: float = 0.34, gain: float = 1.0, start: float = 132.0, end: float = 44.0) -> np.ndarray:
    """A log drum. Pitch falls fast, which is what makes it read as a hit."""
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    sweep = end + (start - end) * np.exp(-24.0 * t)
    phase = 2 * np.pi * np.cumsum(sweep) / RATE
    click = np.random.default_rng(7).normal(0, 1, n) * np.exp(-320.0 * t) * 0.18
    return (np.sin(phase) + click) * np.exp(-7.0 * t) * gain


def tom(freq: float, dur: float = 0.30, gain: float = 1.0) -> np.ndarray:
    n = int(dur * RATE)
    t = np.arange(n) / RATE
    sweep = freq * (0.6 + 0.4 * np.exp(-18.0 * t))
    phase = 2 * np.pi * np.cumsum(sweep) / RATE
    skin = np.random.default_rng(int(freq) + 3).normal(0, 1, n) * np.exp(-90.0 * t) * 0.22
    return (np.sin(phase) + skin) * np.exp(-9.0 * t) * gain


def reverb(x: np.ndarray, amount: float = 0.22, spread: float = 0.055) -> np.ndarray:
    """Four tap delays. Not a room model, just enough air to sit things back."""
    out = x.copy()
    for i, gain in enumerate((0.7, 0.5, 0.36, 0.24)):
        d = int(spread * (i + 1) * RATE)
        if d < out.size:
            out[d:] += x[:-d] * gain * amount
    return out


class Track:
    """A mono buffer with a bar grid, and a loop seam that actually joins."""

    def __init__(self, bpm: float, bars: int, beats_per_bar: int = 4):
        self.bpm = bpm
        self.beat = 60.0 / bpm
        self.bars = bars
        self.beats_per_bar = beats_per_bar
        self.length = int(bars * beats_per_bar * self.beat * RATE)
        # Rendered long, then folded: a note struck in the last bar has to ring
        # over the seam into the first, or the loop ticks once a cycle.
        self.buffer = np.zeros(self.length + RATE * 4)

    def at(self, bar: float, beat: float = 0.0) -> int:
        return int((bar * self.beats_per_bar + beat) * self.beat * RATE)

    def add(self, sample: np.ndarray, bar: float, beat: float = 0.0, gain: float = 1.0):
        start = self.at(bar, beat)
        end = min(start + sample.size, self.buffer.size)
        if end > start:
            self.buffer[start:end] += sample[: end - start] * gain

    def finish(self, peak: float = 0.86) -> np.ndarray:
        body = self.buffer[: self.length].copy()
        tail = self.buffer[self.length :]
        body[: tail.size] += tail[: body.size]
        body = reverb(body)
        top = np.abs(body).max()
        return body * (peak / top) if top > 0 else body


def polish(x: np.ndarray, peak: float = 0.90) -> np.ndarray:
    """Normalise a one-shot and make sure it starts and ends at zero.

    Both halves matter and neither is optional. Synthesised transients overshoot
    easily -- a kick and a mallet summed at full scale clip on the attack, which
    is heard as a crackle rather than as loudness. And a sound cut off while its
    tail is still ringing ends on a step, which is a click every single time it
    plays: the menu blip was still at two-thirds of its amplitude when the
    buffer ran out.
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
    clipped = np.clip(samples, -1.0, 1.0)
    pcm = (clipped * 32767).astype("<i2")
    path = OUT / name
    with wave.open(str(path), "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(RATE)
        f.writeframes(pcm.tobytes())
    print(f"{name:28s} {path.stat().st_size / 1024:8.0f} KB  {samples.size / RATE:5.1f}s")


# ---------------------------------------------------------------------------
# The lobby theme
# ---------------------------------------------------------------------------

def lobby() -> np.ndarray:
    """Eight bars of ii-V-I in F, at a walking pace.

    The melody is built from the chord tones rather than a scale, which is the
    trick that keeps a simple tune sounding harmonically richer than it is.
    """
    t = Track(bpm=96, bars=8)

    # Fmaj9 | Gm7 | C9 | Fmaj7 | Dm7 | Gm7 | C7sus | C7
    chords = [
        ("F2", ["A3", "C4", "E4", "G4"]),
        ("G2", ["Bb3", "D4", "F4", "A4"]),
        ("C3", ["E3", "G3", "Bb3", "D4"]),
        ("F2", ["A3", "C4", "E4", "G4"]),
        ("D3", ["F3", "A3", "C4", "E4"]),
        ("G2", ["Bb3", "D4", "F4", "A4"]),
        ("C3", ["F3", "G3", "Bb3", "D4"]),
        # Lands on the dominant, so bar one answers it.
        ("C3", ["E3", "G3", "Bb3", "D4"]),
    ]

    for bar, (root, voicing) in enumerate(chords):
        t.add(pad([hz(n) for n in voicing], 2.4, gain=0.15), bar)
        # Walking bass: root on one, fifth on three, a passing tone on four.
        t.add(bass(hz(root), 1.1, 0.5), bar, 0)
        t.add(bass(hz(root) * 1.5, 0.9, 0.36), bar, 2)
        t.add(bass(hz(root) * 1.335, 0.5, 0.26), bar, 3)
        # Vibraphone comping on the off-beats, where a rhythm section sits.
        for beat in (1.5, 3.5):
            for n in voicing[:3]:
                t.add(mallet(hz(n), 1.2, 0.09, bright=0.6), bar, beat)

    melody = [
        # (bar, beat, note, length) -- phrased in two four-bar answers.
        (0, 0.0, "A4", 1.2), (0, 1.5, "C5", 0.9), (0, 2.5, "A4", 0.8),
        (1, 0.0, "D5", 1.4), (1, 2.0, "Bb4", 1.1),
        (2, 0.0, "G4", 1.0), (2, 1.5, "Bb4", 0.9), (2, 3.0, "E4", 1.0),
        (3, 0.0, "F4", 2.2), (3, 2.5, "A4", 1.0),
        (4, 0.0, "C5", 1.3), (4, 2.0, "A4", 1.0), (4, 3.0, "F4", 0.9),
        (5, 0.0, "D5", 1.5), (5, 2.5, "F5", 1.1),
        (6, 0.0, "E5", 1.2), (6, 2.0, "D5", 1.0), (6, 3.0, "Bb4", 0.9),
        (7, 0.0, "G4", 2.0), (7, 2.5, "E4", 1.4),
    ]
    for bar, beat, note, dur in melody:
        t.add(mallet(hz(note), dur, 0.30), bar, beat)

    # Brushed shaker on every off-beat: the only percussion, because a menu
    # that ticks like a drum machine stops being restful after a minute.
    for bar in range(8):
        for beat in (0.5, 1.5, 2.5, 3.5):
            t.add(shaker(0.16, 0.055, seed=bar * 4 + int(beat * 2)), bar, beat)

    return t.finish(0.80)


# ---------------------------------------------------------------------------
# The match theme
# ---------------------------------------------------------------------------

def match() -> np.ndarray:
    """The lobby's harmony, taken up a fourth, doubled in speed and given drums.

    Same tonal centre family on purpose: kickoff should sound like the place
    you were just standing in, not a different game.
    """
    t = Track(bpm=140, bars=12)

    # Dm | Dm | Bb | C | Dm | Dm | Bb | C | F | C | Bb | C
    roots = ["D2", "D2", "Bb1", "C2", "D2", "D2", "Bb1", "C2", "F2", "C2", "Bb1", "C2"]
    voicings = {
        "D2": ["D4", "F4", "A4"],
        "Bb1": ["Bb3", "D4", "F4"],
        "C2": ["C4", "E4", "G4"],
        "F2": ["F3", "A3", "C4"],
    }

    for bar, root in enumerate(roots):
        chord = voicings[root]
        t.add(pad([hz(n) for n in chord], 1.6, gain=0.10), bar)
        # Driving eighths on the bass, the engine of the whole thing.
        for beat in (0.0, 0.75, 1.5, 2.0, 2.75, 3.5):
            t.add(bass(hz(root), 0.42, 0.44), bar, beat)
        # Marimba ostinato, up an octave, alternating chord tones.
        pattern = [0, 2, 1, 2, 0, 1]
        for i, beat in enumerate((0.0, 0.5, 1.0, 2.0, 2.5, 3.0)):
            note = hz(chord[pattern[i]]) * 2
            t.add(mallet(note, 0.5, 0.17, bright=1.3), bar, beat)

    for bar in range(12):
        # Four on the floor, with a log-drum answer on the and-of-three.
        for beat in (0.0, 1.0, 2.0, 3.0):
            t.add(kick(gain=0.62), bar, beat)
        t.add(tom(196.0, 0.26, 0.34), bar, 3.5)
        if bar % 4 == 3:
            # A fill that hands over to the next four bars.
            for i, beat in enumerate((2.5, 2.75, 3.0, 3.25, 3.5, 3.75)):
                t.add(tom(150.0 + i * 26, 0.2, 0.34), bar, beat)
        for beat in (0.5, 1.5, 2.5, 3.5):
            t.add(shaker(0.12, 0.07, seed=bar * 7 + int(beat * 2)), bar, beat)

    return t.finish(0.88)


# ---------------------------------------------------------------------------
# Sound effects
# ---------------------------------------------------------------------------

def sfx_move() -> np.ndarray:
    """Moving the menu cursor. Short, dry, and low enough not to nag."""
    return mallet(hz("A4"), 0.16, 0.65, bright=0.8)


def sfx_select() -> np.ndarray:
    """Confirming. Two notes up a fourth: the smallest possible fanfare."""
    out = np.zeros(int(0.42 * RATE))
    a = mallet(hz("D5"), 0.24, 0.6)
    b = mallet(hz("G5"), 0.34, 0.6)
    out[: a.size] += a
    at = int(0.075 * RATE)
    out[at : at + b.size] += b[: out.size - at]
    return reverb(out, 0.3)


def sfx_back() -> np.ndarray:
    """Backing out. The select sound, inverted."""
    out = np.zeros(int(0.36 * RATE))
    a = mallet(hz("G4"), 0.2, 0.5)
    b = mallet(hz("D4"), 0.3, 0.5)
    out[: a.size] += a
    at = int(0.07 * RATE)
    out[at : at + b.size] += b[: out.size - at]
    return out


def sfx_kick_ball() -> np.ndarray:
    """Boot on ball: a low thud with a leather slap over the top."""
    n = int(0.26 * RATE)
    t = np.arange(n) / RATE
    thud = kick(0.26, 0.9, start=220.0, end=70.0)[:n]
    slap = np.random.default_rng(11).normal(0, 1, n) * np.exp(-90.0 * t) * 0.5
    return (thud + slap) * 0.9


def sfx_whistle() -> np.ndarray:
    """A pea whistle: two close tones warbling against each other."""
    n = int(0.55 * RATE)
    t = np.arange(n) / RATE
    warble = np.sin(2 * np.pi * 26.0 * t) * 42.0
    a = np.sin(2 * np.pi * (2_850 + warble) * t)
    b = np.sin(2 * np.pi * (3_180 + warble) * t) * 0.8
    breath = np.random.default_rng(5).normal(0, 1, n) * 0.06
    return (a + b + breath) * env(n, 0.02, 0.05, 0.85, 0.16) * 0.42


def sfx_goal() -> np.ndarray:
    """A goal: whistle, a rising run up the chord, and a crowd swell."""
    n = int(1.9 * RATE)
    t = np.arange(n) / RATE
    out = np.zeros(n)

    run = ["F4", "A4", "C5", "F5", "A5", "C6"]
    for i, note in enumerate(run):
        at = int((0.06 + i * 0.075) * RATE)
        hit = mallet(hz(note), 1.1, 0.42, bright=1.4)
        end = min(at + hit.size, n)
        out[at:end] += hit[: end - at]

    # The crowd: filtered noise that swells and falls. Not voices, but at this
    # length and with this envelope the ear accepts it as a roar.
    noise = np.random.default_rng(21).normal(0, 1, n)
    k = 90
    body = np.convolve(noise, np.ones(k) / k, mode="same")
    swell = np.minimum(t / 0.35, 1.0) * np.exp(-1.1 * np.maximum(t - 0.35, 0))
    out += body * swell * 0.75

    low = np.sin(2 * np.pi * hz("F2") * t) * swell * 0.22
    return reverb(out + low, 0.32) * 0.75


def sfx_whoosh() -> np.ndarray:
    """The camera flight. Noise swept by a widening smoothing window."""
    n = int(1.1 * RATE)
    t = np.arange(n) / RATE
    noise = np.random.default_rng(33).normal(0, 1, n)
    out = np.zeros(n)
    chunk = n // 24
    for i in range(24):
        s = i * chunk
        e = min(s + chunk, n)
        k = max(2, int(2 + 46 * abs(0.5 - i / 24) * 2))
        out[s:e] = np.convolve(noise[s:e], np.ones(k) / k, mode="same")
    shape = np.sin(np.pi * np.clip(t / (n / RATE), 0, 1)) ** 1.5
    return reverb(out * shape, 0.4) * 0.55


def main():
    write("music-lobby.wav", lobby())
    write("music-match.wav", match())
    write("sfx-menu-move.wav", polish(sfx_move()))
    write("sfx-menu-select.wav", polish(sfx_select()))
    write("sfx-menu-back.wav", polish(sfx_back()))
    write("sfx-kick.wav", polish(sfx_kick_ball()))
    write("sfx-whistle.wav", polish(sfx_whistle()))
    write("sfx-goal.wav", polish(sfx_goal()))
    write("sfx-whoosh.wav", polish(sfx_whoosh()))


if __name__ == "__main__":
    main()
