"""Port of the NGENUITY 2.39 keyboard effect engine (NGenuity2.Effects / NGenuity2.Effects.Keyboard).

Timing follows EffectImplBase.RunEffect: every effect has a cycle length `speed` (ms) and a
TimePosition (tp) in [0, 1]; tp hits exactly 1.0 on the frame a cycle ends. In sync mode (rendering
frames for onboard storage) tp is derived from the frame number instead of the wall clock.

Layers are composited like EffectEngine.CompositeEffects: layer 0 is on top, colors carry alpha,
lower layers show through until a key is opaque, and the rest is blended over black.
"""
import math
import random
import time

from .layout import KEYS, MAX_COL, MAX_ROW, NUM_LEDS

RGB_RAINBOW = [(255, 0, 0), (255, 151, 0), (206, 255, 0), (50, 255, 0), (0, 255, 103),
               (0, 255, 255), (0, 104, 255), (43, 0, 255), (205, 0, 255), (255, 0, 158)]
CYCLE_COLORS = [(255, 0, 0), (255, 255, 0), (0, 255, 0), (0, 255, 255), (0, 0, 255)]
THEME_COLOR = (242, 0, 0)


def rand_color():
    return tuple(random.randrange(256) for _ in range(3))


def lerp(c1, c2, t):
    # Color.GetGradientColor (truncating byte cast)
    return tuple(int(a + t * (b - a)) & 0xFF for a, b in zip(c1[:3], c2[:3]))


def with_alpha(c, a):
    return (c[0], c[1], c[2], max(0.0, min(1.0, a)))


# ------------------------------------------------------------------------------------------------
class Effect:
    """EffectImplBase equivalent."""
    name = "effect"
    min_speed, max_speed, default_speed = 500, 3000, 1000
    triggered = False

    def __init__(self, speed=None, color=None, colors=None, keys=None, opacity=100, random_color=False, **_):
        self.speed = int(speed or self.default_speed)
        self.colors = colors or ([color] if color else None)
        self.random_color = random_color
        self.keys = [k for k in KEYS if keys is None or k.name in keys]
        self.opacity = opacity
        self.sync = False
        self.cycle_count = 1
        self.reset()

    # -- timing --
    def reset(self):
        self.timing = time.monotonic()
        self.frame = 0
        self.tp = 0.0
        self.setup()

    def setup(self):
        pass

    def configure_sync(self, fps):
        self.sync = True
        self.fps = fps
        self.reset()
        self.fpc = max(1, int(fps * self.speed / 1000))
        self.ref_fpc = float(self.fpc)

    def advance(self):
        if self.sync:
            if self.fpc == 1:
                self.tp = 1.0
            else:
                n = int(self.frame % self.ref_fpc)
                self.tp = n / (self.fpc - 1)
                if self.tp > 1.0:
                    self.tp = 1.0
            self.frame += 1
            return
        elapsed = (time.monotonic() - self.timing) * 1000.0
        elapsed = min(elapsed, self.speed)
        self.tp = elapsed / self.speed
        if self.tp >= 1.0:
            self.tp = 1.0
            self.timing = time.monotonic()

    @property
    def color(self):
        return self.colors[0] if self.colors else THEME_COLOR

    def run(self):
        """Advance and return {led: (r, g, b, alpha)}."""
        self.advance()
        return self.render()

    def render(self):
        return {}


# ---- looped effects -------------------------------------------------------------------------------
class Solid(Effect):
    name = "solid"

    def configure_sync(self, fps):
        super().configure_sync(fps)
        self.fpc, self.ref_fpc = 1, 1.0

    def render(self):
        return {k.led: with_alpha(self.color, 1) for k in self.keys}


class Breathing(Effect):
    name = "breathing"
    min_speed, max_speed, default_speed = 500, 4000, 2250

    def setup(self):
        self._c = rand_color() if self.random_color else self.color
        self.cycle_count = 1

    def render(self):
        n = self.tp if self.tp <= 0.5 else 1 - self.tp
        a = math.sin(n * math.pi)
        out = {k.led: with_alpha(self._c, a) for k in self.keys}
        if self.tp >= 1.0 and self.random_color:
            self._c = rand_color()
        return out


class Cycle(Effect):
    name = "cycle"
    min_speed, max_speed, default_speed = 500, 4000, 2250

    def setup(self):
        self.cycle_count = 1 if self.colors and len(self.colors) == 1 else 2

    def render(self):
        stops = list(self.colors or CYCLE_COLORS) + [(self.colors or CYCLE_COLORS)[0]]
        n = len(stops) - 1
        t = self.tp - 1 if self.tp > 1 else self.tp
        c = stops[0]
        for j in range(n):
            p0, p1 = j / n, (j + 1) / n
            if p0 <= t <= p1:
                c = lerp(stops[j], stops[j + 1], (t - p0) / (p1 - p0))
        return {k.led: with_alpha(c, 1) for k in self.keys}


class Wave(Effect):
    name = "wave"
    min_speed, max_speed, default_speed = 1000, 6000, 3000

    def __init__(self, angle=180.0, **kw):
        self.angle = float(angle)
        super().__init__(**kw)

    def render(self):
        cols = list(self.colors or RGB_RAINBOW)
        if len(cols) < 2:
            cols = cols * 2
        stops = [(0.5 / len(cols) + i / len(cols), c) for i, c in enumerate(cols)]
        width = MAX_COL + 1
        width += width // (len(stops) - 1)
        ang = self.angle / 180.0 * math.pi
        cx, cy = width // 2, 3.0
        first_p, last_p = stops[0][0], stops[-1][0]
        w = 1 - last_p + first_p
        if w != 0:
            w = 1.0 - first_p
        wrap = lerp(stops[-1][1], stops[0][1], w)
        out = {}
        for k in self.keys:
            x = (k.col - cx) * math.cos(ang) - (k.row - cy) * math.sin(ang) + cx
            x += width * self.tp
            if x > width:
                x -= width
            x /= width
            color = wrap
            for i, (p, c) in enumerate(stops):
                if i == 0 and x < p:
                    color = lerp(wrap, c, x / p)
                    break
                if i == len(stops) - 1 and x > p:
                    color = lerp(c, wrap, (x - p) / (1 - p))
                    break
                if i < len(stops) - 1 and p <= x <= stops[i + 1][0]:
                    color = lerp(c, stops[i + 1][1], (x - p) / (stops[i + 1][0] - p))
                    break
            out[k.led] = with_alpha(color, 1)
        return out


class Swipe(Effect):
    name = "swipe"
    min_speed, max_speed, default_speed = 500, 3000, 2000

    def __init__(self, direction="right", **kw):
        self.direction = direction
        super().__init__(**kw)

    def setup(self):
        self.cycle_count = 2
        self.state = 0
        if self.random_color:
            self.c1, self.c2 = rand_color(), rand_color()
        else:
            cs = self.colors or [(255, 0, 0), (0, 255, 0)]
            self.c1, self.c2 = cs[0], cs[1] if len(cs) > 1 else (0, 0, 0)

    def render(self):
        out = {}
        for k in self.keys:
            if self.direction in ("up", "down"):
                n = MAX_ROW + 1
                edge = n - int(self.tp * n) if self.direction == "up" else int(self.tp * n)
                inside = k.row < edge
            else:
                n = MAX_COL + 1
                edge = n - int(self.tp * n) if self.direction == "left" else int(self.tp * n)
                inside = k.col < edge
            out[k.led] = with_alpha(self.c2 if inside else self.c1, 1)
        if self.tp >= 1.0:
            if self.random_color and not self.sync:
                self.c1, self.c2 = self.c2, rand_color()
            else:
                self.state ^= 1
                cs = self.colors or [(255, 0, 0), (0, 255, 0)]
                a, b = cs[0], cs[1] if len(cs) > 1 else (0, 0, 0)
                self.c1, self.c2 = (a, b) if self.state == 0 else (b, a)
        return out


class Confetti(Effect):
    name = "confetti"
    min_speed, max_speed, default_speed = 500, 3000, 1000

    def setup(self):
        self.cycle_count = 4
        self.state = 0
        self.items = {}
        self.first = {}

    def render(self):
        if self.state == 0:
            self.state = 1
            self.items = {k.led: [rand_color(), rand_color()] for k in self.keys}
            self.first = {led: v[0] for led, v in self.items.items()}
        out = {led: with_alpha(lerp(f, t, self.tp), 1) for led, (f, t) in self.items.items()}
        if self.tp >= 1.0 and (not self.sync or self.state < self.cycle_count):
            for led, v in self.items.items():
                v[0], v[1] = v[1], rand_color()
                if self.sync and self.state == 3:
                    v[1] = self.first[led]  # close the loop for onboard playback
            self.state += 1
        return out


class Sun(Effect):
    name = "sun"
    min_speed, max_speed, default_speed = 500, 1375, 1000
    THEMES = [
        [((255, 178, 0), 0.0), ((255, 97, 0), 0.5)],
        [((255, 178, 0), 0.0), ((255, 97, 0), 0.25), ((255, 20, 0), 0.5), ((255, 183, 0), 0.75)],
        [((255, 130, 0), 0.0), ((255, 153, 0), 0.2), ((255, 161, 0), 0.4), ((255, 168, 0), 0.6), ((255, 97, 0), 0.8)],
    ]

    def setup(self):
        self.cycle_count = 4
        self.state = {k.led: random.randrange(3) for k in self.keys}

    def render(self):
        out = {}
        for k in self.keys:
            theme = self.THEMES[self.state[k.led]]
            for i, (c, p) in enumerate(theme):
                c2, p2 = (theme[i + 1][0], theme[i + 1][1]) if i + 1 < len(theme) else (theme[0][0], 1.0)
                if p <= self.tp <= p2:
                    out[k.led] = with_alpha(lerp(c, c2, (self.tp - p) / (p2 - p)), 1)
                    break
        return out


class Twilight(Effect):
    name = "twilight"
    min_speed, max_speed, default_speed = 300, 1000, 1000

    def setup(self):
        self.cycle_count = 8
        self.active = {}  # led -> [color, step]
        self.inited = False
        self.cycles = 0

    def _spawn(self):
        free = [k.led for k in self.keys if k.led not in self.active]
        for _ in range(min(random.randint(1, 3), len(free))):
            led = free.pop(random.randrange(len(free)))
            self.active[led] = [rand_color() if self.random_color else self.color, 0, True]

    def advance(self):
        super().advance()
        if not self.inited and not self.sync:
            self.tp = 0.0

    def render(self):
        if not self.inited:
            self.inited = True
            self._spawn()
        out = {}
        for led, item in self.active.items():
            c, step, on = item
            if not on:
                continue
            if step == 0:
                a = self.tp if self.tp < 1 else 1.0
            elif step == 3:
                a = 1.0 - self.tp
            elif step >= 4:
                item[2] = False
                continue
            else:
                a = 1.0
            out[led] = with_alpha(c, a)
            if self.tp == 1.0:
                item[1] += 1
        if self.tp == 1.0:
            self.cycles += 1
            if not self.sync or self.cycles <= self.cycle_count - 4:
                self._spawn()
        if all(led in self.active and not self.active[led][2] for led in (k.led for k in self.keys)):
            self.active.clear()
        return out


# ---- triggered (key-press) effects ----------------------------------------------------------------
class Trigger(Effect):
    """KeyboardTriggerEffectImplBase: one instance per key press, removed when finished."""
    triggered = True

    def spawn(self, key):
        inst = type(self).__new__(type(self))
        inst.__dict__.update(self.__dict__)
        inst.key = key
        inst.done = False
        inst.c = rand_color() if self.random_color else self.color
        inst.reset()
        return inst


class Fade(Trigger):
    name = "fade"
    min_speed, max_speed, default_speed = 1000, 5000, 2000

    def render(self):
        n = self.tp if self.tp <= 0.5 else 1 - self.tp
        if self.tp >= 1.0:
            self.done = True
        return {self.key.led: with_alpha(self.c, n / 0.5)}


class Explosion(Trigger):
    name = "explosion"
    min_speed, max_speed, default_speed = 600, 10000, 1000

    def render(self):
        xs = [k.col for k in self.keys]
        ys = [k.row for k in self.keys]
        max_r = int(math.sqrt((min(xs) - max(xs)) ** 2 + (min(ys) - max(ys)) ** 2)) + 11
        r = self.tp * max_r
        out = {}
        for k in self.keys:
            d = math.hypot(k.col - self.key.col, k.row - self.key.row)
            if d > r or r - d > 10.0:
                continue
            out[k.led] = with_alpha(self.c, (10.0 - (r - d)) / 10.0)
        if self.tp >= 1.0:
            self.done = True
        return out


class Flame(Trigger):
    """KeyboardBlazeEffect ("HyperX Flame"): flame tongues rising from the pressed key."""
    name = "flame"
    min_speed, max_speed, default_speed = 1000, 12000, 2000
    LEVELS = {
        1: [(-1, -1), (-2, -2)],
        2: [(0, -1), (1, -1), (1, -2), (2, -3)],
        3: [(0, -2), (1, -3), (1, -4)],
        4: [(0, -3), (-1, -3), (-2, -4)],
        5: [(0, -4)],
        6: [(-1, -5)],
        7: [(-1, -6)],
    }

    def setup(self):
        if not hasattr(self, "key"):
            return
        self.level = 0
        now = time.monotonic()
        self.lit = {self.key.led: now}
        self.last_step = now
        self.state = self.key.row

    def render(self):
        now = time.monotonic()
        if (now - self.last_step) * 1000 >= self.speed / 50:
            self.level += 1
            self.last_step = now
            if self.state >= 0 and self.level in self.LEVELS:
                for dx, dy in self.LEVELS[self.level]:
                    for k in self.keys:
                        if k.col == self.key.col + dx and k.row == self.key.row + dy:
                            self.lit[k.led] = now
            if self.level == 3:
                self.state += 1
            self.state -= 1
        out = {}
        for led, ts in self.lit.items():
            n = (now - ts) * 1000 / self.speed * 2.0
            if n <= 1.0:
                out[led] = with_alpha(self.c, 1.0 - n)
        if self.tp >= 1.0:
            self.done = True
        return out


class Implosion(Trigger):
    """Ring collapsing onto the pressed key; NGENUITY counts engine ticks (~5 ms), mirrored here in time."""
    name = "implosion"
    min_speed, max_speed, default_speed = 5, 50, 20

    def setup(self):
        if not hasattr(self, "key"):
            return
        self.start = time.monotonic()
        self.ticks_fade = (self.speed / 5) / (20 / 5) * 100  # NGENUITY num2
        self.radius = max(self.key.col, MAX_COL - self.key.col)
        self.hits = {}
        self.last_shrink = 0

    def run(self):
        return self.render()

    def render(self):
        tick = (time.monotonic() - self.start) / 0.005
        while self.last_shrink + self.ticks_fade * 0.1 <= tick and self.radius >= 0:
            for k in self.keys:
                if int(math.hypot(k.col - self.key.col, k.row - self.key.row)) == self.radius:
                    self.hits[k.led] = self.last_shrink + self.ticks_fade * 0.1
            self.radius -= 1
            self.last_shrink += self.ticks_fade * 0.1
        out = {}
        for led, t0 in self.hits.items():
            a = 1.0 - (tick - t0) / self.ticks_fade
            if a > 0:
                out[led] = with_alpha(self.c, a)
        if self.radius < 0 and not out:
            self.done = True
        return out


EFFECTS = {cls.name: cls for cls in (Solid, Breathing, Cycle, Wave, Swipe, Confetti, Sun, Twilight,
                                      Fade, Explosion, Flame, Implosion)}


# ---- compositing ----------------------------------------------------------------------------------
def blend_over(top, bottom):
    """Color.Blend: top over bottom, both (r, g, b, a)."""
    a1, a2 = top[3], bottom[3]
    r = tuple(int(t * a1 + b * (1 - a1)) for t, b in zip(top[:3], bottom[:3]))
    return (*r, 1 - (1 - a1) * (1 - a2))


def composite(layer_results, opacities):
    """layer_results[0] is the top layer. Returns NUM_LEDS rgb tuples."""
    stacks = {}
    for res, op in zip(layer_results, opacities):
        for led, c in res.items():
            if op != 100:
                c = (c[0], c[1], c[2], c[3] * op / 100.0)
            stacks.setdefault(led, []).append(c)
    frame = [(0, 0, 0)] * NUM_LEDS
    for led, cs in stacks.items():
        c = cs[0]
        if c[3] < 1.0:
            for below in cs[1:]:
                c = blend_over(c, below)
                if c[3] >= 1.0:
                    break
            if c[3] < 1.0:
                c = blend_over(c, (0, 0, 0, 0))
        frame[led] = (c[0], c[1], c[2])
    return frame


class Engine:
    """Live renderer: looped layers + per-keypress trigger instances on top."""

    def __init__(self, layers):
        self.layers = [l for l in layers if not l.triggered]
        self.triggers = [l for l in layers if l.triggered]
        self.active = []

    def key_pressed(self, key):
        for t in self.triggers:
            self.active.insert(0, t.spawn(key))

    def frame(self):
        results, ops = [], []
        for inst in list(self.active):
            results.append(inst.run())
            ops.append(100)
        self.active = [i for i in self.active if not i.done]
        # NGENUITY runs lower layers first; index 0 stays on top
        looped = [l.run() for l in reversed(self.layers)][::-1]
        results += looped
        ops += [l.opacity for l in self.layers]
        return composite(results, ops)


def render_sync_frames(layers, fps=20, max_frames=120):
    """EffectEngine.Sync: render a seamless loop of <= max_frames frames for onboard storage."""
    layers = [l for l in layers if not l.triggered]
    if not layers:
        return [[(0, 0, 0)] * NUM_LEDS]
    for l in layers:
        l.configure_sync(fps)
    total = min(max(l.fpc * l.cycle_count for l in layers), max_frames)
    fits = all(not (1.2 < total / (l.fpc * l.cycle_count) < 2) for l in layers)
    if not fits:
        best = 999
        for l in layers:
            n = max_frames // (l.fpc * l.cycle_count)
            if n < best:
                best = n
                total = n * l.fpc * l.cycle_count if n else max_frames
    for l in layers:
        n = total // (l.fpc * l.cycle_count)
        if n == 0:
            l.fpc = max(1, total // l.cycle_count)
            l.ref_fpc = total / l.cycle_count
        else:
            l.fpc = max(1, total // n // l.cycle_count)
            l.ref_fpc = total / (n * l.cycle_count)
    frames = []
    for _ in range(total):
        results = [l.run() for l in reversed(layers)][::-1]
        frames.append(composite(results, [l.opacity for l in layers]))
    return frames
