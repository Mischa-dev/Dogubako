#!/usr/bin/env python3
"""Procedural soundtrack for the Dogubako alpha trailer.

Everything is synthesised from scratch with numpy (no samples, no licensing
questions). The score is written on the same grid as the visuals: 96 BPM,
one bar = 2.5 s, and every scene change in scenes/timeline.js lands on a bar.

    python3 trailer/tools/music.py trailer/build/music.wav
"""
import sys
import wave

import numpy as np

SR = 48_000
BPM = 96
BEAT = 60 / BPM          # 0.625 s
BAR = 4 * BEAT           # 2.5 s
LENGTH = 72.0            # seconds, incl. the tail of the last chord
N = int(LENGTH * SR)
rng = np.random.default_rng(7)  # fixed seed: the same file every render


def t_of(bar, beat=0.0):
    return bar * BAR + beat * BEAT


def midi(n):
    return 440.0 * 2 ** ((n - 69) / 12)


class Bus:
    def __init__(self):
        self.l = np.zeros(N)
        self.r = np.zeros(N)

    def add(self, start, sig, gain=1.0, pan=0.0):
        """Mix a mono or (2, n) signal in at `start` seconds."""
        i = int(round(start * SR))
        if i >= N:
            return
        if i < 0:
            sig = sig[..., -i:]
            i = 0
        if sig.ndim == 1:
            sig = np.vstack([sig, sig])
        n = min(sig.shape[1], N - i)
        gl = gain * np.cos((pan + 1) * np.pi / 4) * np.sqrt(2)
        gr = gain * np.sin((pan + 1) * np.pi / 4) * np.sqrt(2)
        self.l[i:i + n] += sig[0, :n] * gl
        self.r[i:i + n] += sig[1, :n] * gr

    def stereo(self):
        return np.vstack([self.l, self.r])


def env_adsr(n, a, d, s, r, total):
    """Linear-attack, exponential-decay envelope, `total` seconds long incl. release."""
    t = np.arange(n) / SR
    e = np.where(t < a, t / max(a, 1e-4), s + (1 - s) * np.exp(-(t - a) / max(d, 1e-4)))
    rel_start = total - r
    e = np.where(t > rel_start, e * np.clip(1 - (t - rel_start) / max(r, 1e-4), 0, 1), e)
    return e


def lowpass_fft(x, cutoff, slope=2.0):
    X = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    X *= 1 / np.sqrt(1 + (f / cutoff) ** (2 * slope))
    return np.fft.irfft(X, len(x))


def highpass_fft(x, cutoff, slope=2.0):
    X = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    X *= 1 - 1 / np.sqrt(1 + (f / cutoff) ** (2 * slope))
    return np.fft.irfft(X, len(x))


# ----------------------------------------------------------------- voices

def pad(notes, dur, bright=1.0):
    """Warm detuned additive pad, slow swell; returns (2, n)."""
    n = int(dur * SR)
    t = np.arange(n) / SR
    out = np.zeros((2, n))
    for note in notes:
        f0 = midi(note)
        for ch, det in ((0, -0.07), (1, 0.07)):
            for voice_det in (-1, 1):
                f = f0 * 2 ** ((det * voice_det + det) / 12)
                ph = rng.uniform(0, 2 * np.pi)
                for h in range(1, 9):
                    if f * h > 6000:
                        break
                    amp = (1 / h) * np.exp(-(h - 1) * 0.55 / bright)
                    vib = 0.0015 * np.sin(2 * np.pi * (0.21 + 0.05 * h) * t + ph)
                    out[ch] += amp * np.sin(2 * np.pi * f * h * t * (1 + vib) + ph * h)
    out /= max(1, len(notes)) * 4
    e = env_adsr(n, min(1.4, dur * 0.4), 3.0, 0.85, min(1.2, dur * 0.35), dur)
    return out * e


def pluck(note, dur=0.9, bright=1.0):
    """Marimba/kalimba-ish pluck: harmonics that die faster the higher they are."""
    n = int(dur * SR)
    t = np.arange(n) / SR
    f0 = midi(note)
    sig = np.zeros(n)
    for h, a in ((1, 1.0), (2, 0.42 * bright), (3, 0.18 * bright), (4, 0.11 * bright), (6, 0.05 * bright)):
        if f0 * h > 9000:
            continue
        sig += a * np.sin(2 * np.pi * f0 * h * 1.0007 ** (h - 1) * t) * np.exp(-t * (3.2 + 2.6 * h))
    attack = np.clip(t / 0.003, 0, 1)
    return sig * attack * 0.5


def keys(note, dur=2.4):
    """Soft felt-piano tone with slight inharmonicity."""
    n = int(dur * SR)
    t = np.arange(n) / SR
    f0 = midi(note)
    sig = np.zeros(n)
    for h in range(1, 7):
        fh = f0 * h * np.sqrt(1 + 0.0004 * h * h)
        if fh > 8000:
            break
        sig += (0.9 / h ** 1.4) * np.sin(2 * np.pi * fh * t) * np.exp(-t * (0.9 + 0.8 * h))
    hammer = highpass_fft(rng.standard_normal(n) * np.exp(-t * 90), 1200) * 0.03
    return (sig + hammer) * np.clip(t / 0.004, 0, 1) * 0.42


def bass(note, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = midi(note)
    sig = np.sin(2 * np.pi * f * t) + 0.22 * np.sin(4 * np.pi * f * t) + 0.08 * np.sin(6 * np.pi * f * t)
    return sig * env_adsr(n, 0.012, 0.5, 0.65, 0.08, dur) * 0.55


def kick():
    n = int(0.5 * SR)
    t = np.arange(n) / SR
    f = 46 + 95 * np.exp(-t * 32)
    ph = 2 * np.pi * np.cumsum(f) / SR
    body = np.sin(ph) * np.exp(-t * 7.5)
    click = highpass_fft(rng.standard_normal(n) * np.exp(-t * 400), 2500) * 0.12
    return (body + click) * 0.9


def hat(open_=False):
    n = int((0.32 if open_ else 0.09) * SR)
    t = np.arange(n) / SR
    x = highpass_fft(rng.standard_normal(n), 7500, 3)
    return x * np.exp(-t * (14 if open_ else 55)) * 0.16


def clap():
    n = int(0.4 * SR)
    t = np.arange(n) / SR
    x = rng.standard_normal(n)
    x = highpass_fft(lowpass_fft(x, 3200), 900)
    e = np.zeros(n)
    for off in (0.0, 0.011, 0.022):
        e += np.where(t >= off, np.exp(-(t - off) * 70), 0)
    e += np.exp(-t * 16) * 0.35
    return x * e * 0.28


def shaker():
    n = int(0.12 * SR)
    t = np.arange(n) / SR
    x = highpass_fft(rng.standard_normal(n), 5000)
    return x * np.sin(np.pi * np.clip(t / 0.12, 0, 1)) ** 2 * 0.05


def latch_click():
    """The little mechanical 'tk' when the vermilion latch drops onto the box."""
    n = int(0.25 * SR)
    t = np.arange(n) / SR
    x = highpass_fft(rng.standard_normal(n) * np.exp(-t * 260), 1500) * 0.5
    x += np.sin(2 * np.pi * 1850 * t) * np.exp(-t * 60) * 0.25
    x += np.sin(2 * np.pi * 330 * t) * np.exp(-t * 40) * 0.35
    return x


def bell(note, dur=4.0):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = midi(note)
    sig = np.zeros(n)
    for ratio, a, d in ((1, 1, 0.9), (2.76, 0.4, 2.2), (5.4, 0.18, 4), (8.93, 0.08, 6)):
        sig += a * np.sin(2 * np.pi * f * ratio * t) * np.exp(-t * d)
    return sig * np.clip(t / 0.002, 0, 1) * 0.22


def riser(dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = rng.standard_normal(n)
    # sweep a band upward by mixing progressively brighter copies
    lo = lowpass_fft(x, 600)
    hi = highpass_fft(lowpass_fft(x, 9000), 1500)
    p = (t / dur)
    sig = lo * (1 - p) + hi * p
    return sig * p ** 2.2 * 0.22


def impact():
    n = int(3.5 * SR)
    t = np.arange(n) / SR
    f = 38 + 60 * np.exp(-t * 6)
    boom = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t * 1.6)
    air = lowpass_fft(rng.standard_normal(n), 2500) * np.exp(-t * 3) * 0.12
    return (boom * 0.9 + air)


def whoosh(dur=0.9):
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = rng.standard_normal(n)
    x = highpass_fft(lowpass_fft(x, 4000), 400)
    e = np.sin(np.pi * np.clip(t / dur, 0, 1)) ** 3
    return x * e * 0.12


def reverb(sig, seconds=2.8, damp=5000, seed=3):
    """Convolution with a decaying, darkened noise tail (FFT)."""
    r = np.random.default_rng(seed)
    n = int(seconds * SR)
    t = np.arange(n) / SR
    out = []
    for ch in range(2):
        ir = r.standard_normal(n) * np.exp(-t * 6.9 / seconds)
        ir = lowpass_fft(ir, damp)
        ir[: int(0.012 * SR)] = 0  # pre-delay
        ir /= np.sqrt(np.sum(ir ** 2))
        L = len(sig[ch]) + n
        size = 1 << (L - 1).bit_length()
        y = np.fft.irfft(np.fft.rfft(sig[ch], size) * np.fft.rfft(ir, size), size)[: len(sig[ch])]
        out.append(y)
    return np.vstack(out)


# ----------------------------------------------------------------- score

# I – vi – IV – V in D, voiced warm: Dmaj9, Bm9, Gmaj7(#11-ish), A6sus → A
PROG = [
    dict(root=38, pad=[62, 66, 69, 73, 76], arp=[62, 66, 69, 73, 74, 69, 66, 69]),
    dict(root=35, pad=[59, 62, 66, 69, 73], arp=[59, 62, 66, 69, 73, 69, 66, 62]),
    dict(root=31, pad=[59, 62, 66, 67, 71], arp=[55, 59, 62, 66, 71, 66, 62, 59]),
    dict(root=33, pad=[57, 61, 64, 66, 69], arp=[57, 61, 64, 66, 69, 66, 64, 61]),
]
MELODY = {  # bar -> [(beat, midi, dur)] played on keys; a small motif that returns
    4: [(0, 78, 1.2), (1.5, 76, 0.8), (2.5, 73, 1.6)],
    6: [(0, 74, 1.0), (1, 76, 0.8), (2, 78, 1.0), (3, 81, 1.4)],
    8: [(0, 78, 1.2), (1.5, 76, 0.8), (2.5, 73, 1.6)],
    10: [(0, 71, 1.0), (1, 73, 0.8), (2, 74, 1.0), (3, 76, 1.4)],
    19: [(0, 78, 2.0), (2, 76, 2.0)],
    20: [(0, 74, 2.0), (2, 73, 2.0)],
    21: [(0, 71, 1.5), (1.5, 73, 0.5), (2, 74, 2.0)],
    24: [(0, 81, 2.5), (2, 78, 2.0)],
    25: [(0, 76, 2.0), (2, 73, 3.0)],
}

# Sections, in bars (must match scenes/timeline.js)
INTRO_END = 2      # logo
ARP_IN = 2
DRUMS_IN = 4
BREAK_START = 19   # local-first: drums drop out
BREAK_END = 22
FINAL = 24         # end card
END = 28


def build():
    dry = Bus()      # drums, bass: little reverb
    wet = Bus()      # melodic: send to reverb
    fx = Bus()       # one-shots

    # pads: every bar of the piece
    for bar in range(0, END):
        ch = PROG[bar % 4] if bar >= 2 else PROG[0]
        if bar < 2 and bar == 1:
            continue  # first pad holds two bars
        dur = BAR * (2 if bar == 0 else 1) + 1.4
        bright = 0.6 if bar < DRUMS_IN or BREAK_START <= bar < BREAK_END else 1.0
        if bar >= FINAL:
            bright = 1.2
        g = 0.42 if bar >= DRUMS_IN else 0.55
        wet.add(t_of(bar) - 0.05, pad(ch['pad'], dur, bright), g)

    # final chord, held and ringing out
    final = dict(pad=[50, 57, 62, 66, 69, 73, 76])
    wet.add(t_of(26), pad(final['pad'], LENGTH - t_of(26), 1.1), 0.5)

    # arpeggio: 8ths, then 16ths once the groove is in
    for bar in range(ARP_IN, 26):
        ch = PROG[bar % 4]
        sixteenths = DRUMS_IN <= bar < BREAK_START or BREAK_END <= bar < FINAL
        steps = 16 if sixteenths else 8
        for s in range(steps):
            note = ch['arp'][s % 8] + (12 if (sixteenths and s % 8 >= 4 and bar % 2) else 0)
            beat = s * (4 / steps)
            vel = 0.75 if s % 4 == 0 else 0.5
            if bar >= FINAL:
                vel *= max(0.0, 1 - (bar - FINAL + beat / 4) / 2.2)
            pan = 0.35 * np.sin(s * 0.9 + bar)
            wet.add(t_of(bar, beat), pluck(note, 0.8, 0.9 if sixteenths else 0.7), 0.42 * vel, pan)

    # melody on keys
    for bar, notes in MELODY.items():
        for beat, note, d in notes:
            wet.add(t_of(bar, beat), keys(note, d + 2.0), 0.5, -0.1)
            wet.add(t_of(bar, beat), keys(note - 12, d + 2.0), 0.18, 0.1)

    # bass with sidechain-ish shape built into note lengths
    for bar in range(DRUMS_IN, FINAL):
        if BREAK_START <= bar < BREAK_END:
            dry.add(t_of(bar), bass(PROG[bar % 4]['root'], BAR), 0.3)
            continue
        root = PROG[bar % 4]['root']
        for beat, d, dn in ((0, 0.55, 0), (0.75, 0.2, 0), (1.5, 0.45, 0), (2.5, 0.4, 0), (3.25, 0.3, 12 if bar % 2 else 7)):
            dry.add(t_of(bar, beat), bass(root + dn, d * BEAT * 2), 0.36)

    # drums
    for bar in range(DRUMS_IN, FINAL):
        breakdown = BREAK_START <= bar < BREAK_END
        if breakdown:
            # only a soft shaker in the breakdown, coming back toward the end
            for s in range(8):
                fx.add(t_of(bar, s / 2 + 0.5), shaker(), 0.8 if bar == BREAK_END - 1 else 0.5)
            continue
        for b in range(4):
            if b in (0, 2) or (b == 3 and bar % 4 == 3):
                dry.add(t_of(bar, b + (0.5 if b == 3 else 0)), kick(), 0.62)
            if b in (1, 3):
                wet.add(t_of(bar, b), clap(), 0.65, 0.05)
            dry.add(t_of(bar, b + 0.5), hat(open_=(b == 3 and bar % 2 == 1)), 1.25, 0.25)
            dry.add(t_of(bar, b + 0.25), hat(), 0.35, -0.25)
            dry.add(t_of(bar, b + 0.75), hat(), 0.45, -0.25)

    # one-shots on the cut points
    fx.add(1.6, latch_click(), 0.9)               # latch drops at ~1.6 s
    wet.add(1.62, bell(81, 5), 0.55)
    wet.add(1.62, bell(74, 5), 0.35)
    fx.add(t_of(DRUMS_IN) - 2.5, riser(2.5), 0.9)
    fx.add(t_of(DRUMS_IN), impact(), 0.42)
    for bar in (6, 9, 11, 16, 22):
        fx.add(t_of(bar) - 0.45, whoosh(0.9), 1.0)
    fx.add(t_of(14) - 2.5, riser(2.5), 1.0)       # into "249 tools"
    fx.add(t_of(14), impact(), 0.52)
    fx.add(t_of(BREAK_END) - 2.5, riser(2.5), 0.8)
    fx.add(t_of(FINAL) - 2.5, riser(2.5), 1.0)
    fx.add(t_of(FINAL), impact(), 0.62)
    fx.add(t_of(FINAL) + 0.02, latch_click(), 0.7)
    wet.add(t_of(FINAL), bell(86, 6), 0.4)
    wet.add(t_of(FINAL), bell(81, 6), 0.35)

    # duck the melodic bus under the kick (sidechain)
    duck = np.ones(N)
    k = int(0.28 * SR)
    shape = 1 - 0.35 * np.exp(-np.arange(k) / SR * 14)
    for bar in range(DRUMS_IN, FINAL):
        if BREAK_START <= bar < BREAK_END:
            continue
        for b in (0, 2):
            i = int(t_of(bar, b) * SR)
            duck[i:i + k] = np.minimum(duck[i:i + k], shape[: max(0, min(k, N - i))])
    w = wet.stereo() * duck
    d = dry.stereo() * duck ** 0.3
    f = fx.stereo()

    mix = w * 0.85 + reverb(w, 3.2) * 0.42 + d + reverb(d, 1.2, 3000, 5) * 0.08 + f + reverb(f, 2.6, 4000, 9) * 0.35

    # gentle master: low cut, tilt, soft clip, fades
    mix = np.vstack([highpass_fft(mix[0], 32), highpass_fft(mix[1], 32)])
    mix = mix - 0.35 * np.vstack([lowpass_fft(mix[0], 90), lowpass_fft(mix[1], 90)])
    mix /= np.max(np.abs(mix)) + 1e-9
    mix = np.tanh(mix * 1.6) / np.tanh(1.6)
    t = np.arange(N) / SR
    fade_in = np.clip(t / 0.08, 0, 1)
    fade_out = np.clip((LENGTH - t) / 3.0, 0, 1) ** 1.5
    mix *= fade_in * fade_out
    mix *= 10 ** (-1.0 / 20) / (np.max(np.abs(mix)) + 1e-9)
    return mix


def write_wav(path, mix):
    data = (np.clip(mix.T, -1, 1) * 32767).astype('<i2')
    with wave.open(path, 'wb') as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data.tobytes())


if __name__ == '__main__':
    out = sys.argv[1] if len(sys.argv) > 1 else 'music.wav'
    write_wav(out, build())
    print(f'wrote {out} ({LENGTH:.0f}s, {SR} Hz)')
