import json, math

def look(sky, fog, ground, grid, buildings, roof, window, windows, light, near, far, glow=0, stars=False, structure=(90, 170, 255)):
    return dict(sky=list(sky), fog=list(fog), ground=list(ground), grid=list(grid), buildings=[list(b) for b in buildings],
                roof=list(roof), window=list(window), windows=windows, light_dir=list(light), fog_near_m=near, fog_far_m=far,
                ground_glow_pct=glow, stars=stars, structure=list(structure))

def m(id, name, scene, kind, start, reward, plus, **kw):
    d = dict(id=id, name=name, scene=scene, kind=kind, start=start, reward=reward, plus=plus)
    d.update(kw)
    return d

def unit(t, x, z, alt=0, wave=0, goal=None):
    u = {"type": t, "at": [x, z, alt], "wave": wave}
    if goal: u["goal"] = goal
    return u

def ring(cx, cz, r, n, t, alt, wave=0, phase=0):
    return [unit(t, int(cx + r * math.sin(phase + 2 * math.pi * k / n)), int(cz + r * math.cos(phase + 2 * math.pi * k / n)), alt, wave) for k in range(n)]

def plus(rule, value, reward):
    return {"rule": rule, "value": value, "reward": reward}

DEFAULT_LIKE = None


# ---- obstacle-course pieces for the races ----
def gate(x, z, alt, kind="ring", r=None, yaw=None, ride=None, opens=None):
    g = {"at": [x, z, alt]}
    if kind != "ring": g["kind"] = kind
    if r is not None: g["r_m"] = r
    if yaw is not None: g["yaw"] = yaw
    if ride is not None: g["ride"] = ride
    if opens is not None: g["opens"] = opens
    return g

def block(x, z, w, d, h, base=0):
    b = {"at": [x, z], "size": [w, d, h]}
    if base: b["base"] = base
    return b

def holed_wall(x, z, width, height, hx, halt, hw, hh, thick=4, along_x=True):
    """A wall across the way with one hole in it: four blocks round the hole."""
    left, right = x - width // 2, x + width // 2
    lo, hi = halt - hh // 2, halt + hh // 2
    out = []
    lw = (hx - hw // 2) - left
    rw = right - (hx + hw // 2)
    if lw > 0: out.append(block(left + lw // 2, z, lw, thick, height))
    if rw > 0: out.append(block(right - rw // 2, z, rw, thick, height))
    if lo > 0: out.append(block(hx, z, hw, thick, lo))
    out.append(block(hx, z, hw, thick, height - hi, hi))
    return out

def machine(role, x, z, w, d, h, base=0, path=(), speed=0, wait=0):
    m = {"role": role, "at": [x, z, base], "size": [w, d, h]}
    if path: m["path"] = [list(p) for p in path]
    if speed: m["speed_ms"] = speed
    if wait: m["wait_ms"] = wait
    return m

# Halden: the avenue, as a first lesson in every piece
HALDEN_MACHINES = [
    machine("door", 0, -20, 84, 4, 46, path=[(0, 0, 0), (0, 0, -47)], speed=45),
    machine("lift", 0, -160, 20, 20, 4, path=[(0, 0, 0), (0, 0, 30)], speed=9, wait=900),
    machine("sweeper", -30, 230, 14, 4, 16, path=[(0, 0, 0), (60, 0, 0)], speed=22),
    machine("sweeper", 30, 190, 14, 4, 16, path=[(0, 0, 0), (-60, 0, 0)], speed=22),
]
HALDEN_COURSE = [
    gate(0, 940, 8),
    gate(0, 840, 7, r=8, yaw=0),
    gate(0, 760, 8, "boost", yaw=0),
    gate(0, 620, 7, r=9, yaw=0),
    gate(20, 380, 28, "pad", r=9),
    gate(0, 300, 12),
    gate(0, 140, 8),
    gate(28, 60, 6, "switch", r=7, opens=0),
    gate(0, -50, 8, r=10, yaw=0),
    gate(0, -160, 4, "pad", r=9, ride=1),
    gate(0, -260, 34),
    gate(0, -380, 10, "pad", r=12),
]
HALDEN_BLOCKS = (holed_wall(0, 840, 84, 40, 0, 9, 20, 18)
                 + [block(0, 620, 84, 120, 30, 16)]
                 + [block(-20, 460, 20, 20, 8), block(0, 420, 20, 20, 18), block(20, 380, 20, 20, 28)]
                 + [block(28, 60, 16, 16, 6), block(0, -380, 30, 30, 10)])

def yaw_to(a, b):
    """The heading, in degrees, that faces from a to b (x, z)."""
    return round(math.degrees(math.atan2(-(b[0] - a[0]), -(b[1] - a[1])))) % 360

def corridors(start, course, pad=30, wrap_half=None):
    """Rectangles cleared of the planet's buildings along a course."""
    pts = [start] + [g["at"] for g in course]
    out = []
    for a, b in zip(pts, pts[1:]):
        if wrap_half and abs(a[0] - b[0]) >= wrap_half:
            continue
        out.append([min(a[0], b[0]) - pad, min(a[1], b[1]) - pad, max(a[0], b[0]) + pad, max(a[1], b[1]) + pad])
    return out

def vault(x, z, alt_roof=20, half=12, depth=40, thick=4):
    """A short tunnel facing -z: two walls and a roof. Its mouth is at z + depth / 2."""
    return [block(x - half - thick // 2, z, thick, depth, alt_roof + 6), block(x + half + thick // 2, z, thick, depth, alt_roof + 6),
            block(x, z, 2 * half + 2 * thick, depth, 6, alt_roof)]

# Sere: the canyon, as pistons, a slalom of walls, a mesa stair and a sweeper alley
SERE_MACHINES = [
    machine("door", 0, -140, 94, 4, 50, path=[(0, 0, 0), (0, 0, -51)], speed=45),
    machine("piston", 0, 820, 40, 14, 22, base=16, path=[(0, 0, 0), (0, 0, -16)], speed=28, wait=700),
    machine("piston", 0, 780, 40, 14, 22, base=16, path=[(0, 0, -16), (0, 0, 0)], speed=28, wait=700),
    machine("piston", 0, 740, 40, 14, 22, base=16, path=[(0, 0, 0), (0, 0, -16)], speed=28, wait=500),
    machine("sweeper", -40, -260, 14, 4, 18, path=[(0, 0, 0), (80, 0, 0)], speed=26),
    machine("sweeper", 40, -300, 14, 4, 18, path=[(0, 0, 0), (-80, 0, 0)], speed=26),
    machine("sweeper", -40, -340, 14, 4, 18, path=[(0, 0, 0), (80, 0, 0)], speed=30),
]
SERE_COURSE = [
    gate(0, 920, 7),
    gate(0, 700, 7, r=9, yaw=0),
    gate(-20, 620, 7, r=8, yaw=0),
    gate(20, 540, 7, r=8, yaw=0),
    gate(-20, 460, 7, r=8, yaw=0),
    gate(0, 380, 8, "boost", yaw=0),
    gate(-20, 250, 12, "pad", r=8),
    gate(10, 200, 26, "pad", r=8),
    gate(30, 150, 40, "pad", r=8),
    gate(0, 60, 20, yaw=0),
    gate(-30, -40, 30, "switch", r=7, opens=0),
    gate(0, -170, 8, r=10, yaw=0),
    gate(0, -400, 8),
    gate(0, -520, 6, "pad", r=12),
]
SERE_BLOCKS = ([block(0, 780, 94, 100, 10, 40)]
               + holed_wall(0, 620, 94, 44, -20, 9, 18, 18) + holed_wall(0, 540, 94, 44, 20, 9, 18, 18)
               + holed_wall(0, 460, 94, 44, -20, 9, 18, 18)
               + [block(-20, 250, 18, 18, 12), block(10, 200, 18, 18, 26), block(30, 150, 18, 18, 40)]
               + [block(-30, -40, 16, 16, 30), block(0, -520, 30, 30, 6)])

# Spindle: platforms that move, the long way round the ring
SPIN_MACHINES = [
    machine("door", -394, 700, 4, 34, 40, path=[(0, 0, 0), (0, 0, -41)], speed=40),
    machine("lift", 60, 1450, 20, 20, 4, path=[(0, 0, 0), (0, 0, 40)], speed=10, wait=800),
    machine("lift", 160, 1300, 22, 22, 4, base=34, path=[(0, 0, 0), (110, 0, 0)], speed=12, wait=600),
    machine("sweeper", 380, 1090, 4, 14, 18, path=[(0, 0, 0), (0, -60, 0)], speed=22),
    machine("sweeper", 440, 1030, 4, 14, 18, path=[(0, -60, 0), (0, 0, 0)], speed=22),
    machine("lift", -500, 860, 22, 22, 4, path=[(0, 0, 0), (0, 0, 30)], speed=8, wait=600),
]
SPIN_COURSE = [
    gate(0, 1550, 8),
    gate(60, 1450, 4, "pad", r=9, ride=1),
    gate(120, 1380, 45, r=12),
    gate(160, 1300, 38, "pad", r=10, ride=2),
    gate(320, 1220, 34, "boost", yaw=yaw_to((270, 1300), (380, 1180))),
    gate(470, 1000, 8, r=10),
    gate(-520, 940, 8, yaw=270),
    gate(-500, 860, 4, "pad", r=10, ride=5),
    gate(-470, 770, 28, "switch", r=7, opens=0),
    gate(-430, 700, 8, r=10, yaw=270),
    gate(-370, 700, 8, r=10, yaw=270),
    gate(-300, 560, 14),
    gate(-200, 420, 22),
    gate(-100, 300, 8, "boost", yaw=yaw_to((-200, 420), (0, 150))),
    gate(0, 150, 6, "pad", r=12),
]
SPIN_BLOCKS = ([block(-470, 770, 16, 16, 28), block(0, 150, 30, 30, 6)]
               + [block(-380, 700 + 22, 34, 4, 40), block(-380, 700 - 22, 34, 4, 40)]
               + [block(-380, 700, 34, 44, 6, 40)])

# Rime: needles to land on in low gravity and little grip, and a vault to open
RIME_MACHINES = [
    machine("door", -40, 24, 32, 4, 26, path=[(0, 0, 0), (0, 0, 27)], speed=30),
    machine("sweeper", -170, -290, 14, 4, 16, path=[(0, 0, 0), (90, 0, 0)], speed=24),
    machine("sweeper", -80, -330, 14, 4, 16, path=[(0, 0, 0), (-90, 0, 0)], speed=24),
    machine("lift", 60, -600, 18, 18, 4, path=[(0, 0, 0), (0, 0, 36)], speed=12, wait=700),
]
RIME_COURSE = [
    gate(0, 850, 8),
    gate(0, 780, 10, "boost", yaw=0),
    gate(40, 650, 40, "pad", r=6),
    gate(80, 560, 60, r=10),
    gate(120, 460, 56, "pad", r=6),
    gate(60, 360, 20),
    gate(20, 280, 8, r=8, yaw=0),
    gate(-20, 200, 8, r=8, yaw=0),
    gate(-80, 100, 30, "switch", r=6, opens=0),
    gate(-40, 0, 8, r=9, yaw=0),
    gate(-140, -200, 24),
    gate(-60, -420, 12),
    gate(60, -600, 4, "pad", r=8, ride=3),
    gate(160, -700, 40, r=12),
    gate(300, -800, 10, "pad", r=12),
]
RIME_BLOCKS = ([block(40, 650, 14, 14, 40), block(120, 460, 14, 14, 56)]
               + holed_wall(20, 280, 120, 50, 20, 8, 16, 16) + holed_wall(-20, 200, 120, 50, -20, 8, 16, 16)
               + [block(-80, 100, 12, 12, 30)] + vault(-40, 0, 20, 14, 44) + [block(300, -800, 30, 30, 10)])

# Cinder: columns over the lava, a rising lift, a pounding runway, a vault and a swept bridge
CINDER_MACHINES = [
    machine("door", 100, 62, 36, 4, 20, base=30, path=[(0, 0, 0), (0, 0, 21)], speed=30),
    machine("lift", 0, 520, 20, 20, 4, path=[(0, 0, 0), (0, 0, 42)], speed=10, wait=700),
    machine("piston", 100, 330, 30, 12, 14, base=46, path=[(0, 0, 0), (0, 0, -15)], speed=30, wait=600),
    machine("piston", 100, 300, 30, 12, 14, base=46, path=[(0, 0, -15), (0, 0, 0)], speed=30, wait=600),
    machine("piston", 100, 270, 30, 12, 14, base=46, path=[(0, 0, 0), (0, 0, -15)], speed=34, wait=400),
    machine("sweeper", -70, -380, 4, 14, 14, base=30, path=[(0, 0, 0), (20, 0, 0)], speed=10),
    machine("sweeper", -50, -420, 4, 14, 14, base=30, path=[(0, 0, 0), (-20, 0, 0)], speed=10),
]
CINDER_COURSE = [
    gate(0, 780, 30, "pad", r=8),
    gate(-60, 700, 38, "pad", r=8),
    gate(-60, 620, 45, r=10),
    gate(0, 520, 4, "pad", r=9, ride=1),
    gate(60, 440, 55, r=10),
    gate(100, 250, 36, r=8, yaw=0),
    gate(160, 140, 34, "switch", r=7, opens=0),
    gate(100, 40, 38, r=9, yaw=0),
    gate(40, -60, 40, "boost", yaw=yaw_to((40, -60), (-140, -260))),
    gate(-140, -260, 34, "pad", r=9),
    gate(-60, -460, 36, r=9, yaw=0),
    gate(140, -560, 36, "pad", r=12),
]
CINDER_BLOCKS = ([block(0, 900, 60, 40, 25), block(0, 780, 20, 20, 30), block(-60, 700, 20, 20, 38),
                  block(-60, 620, 14, 14, 30), block(100, 300, 30, 130, 30), block(160, 140, 18, 18, 34)]
                 + [block(100 - 16, 40, 4, 44, 50, 30), block(100 + 16, 40, 4, 44, 50, 30), block(100, 40, 36, 44, 6, 50),
                    block(100, 40, 36, 44, 30)]
                 + [block(-140, -260, 20, 20, 34), block(-60, -400, 24, 120, 30), block(140, -560, 30, 30, 36)])

# ---- mission objects ----
def train(name, z0, z1, cars, car_ap, engine_ap, speed):
    """An armoured train: a locomotive and its cars, running the canyon once.
    The cars are its weak points; the engine is sealed until they fall."""
    hull = [dict(at=[0, 5, 0], size=[12, 10, 22], paint="dark"),
            dict(at=[0, 11, -4], size=[8, 3, 10], paint="secondary"),
            dict(at=[0, 10, -11], size=[13, 1, 1], paint="glow")]
    weak, tur = [], []
    for k in range(cars):
        z = 27 + 26 * k
        hull.append(dict(at=[0, 4, z], size=[11, 8, 23], paint="primary"))
        hull.append(dict(at=[0, 8, z], size=[12, 1, 24], paint="glow" if k % 2 else "secondary"))
        weak.append([0, 12, z])
        if k % 2 == 0:
            tur.append([0, 10, z + 8])
    return dict(name=name, at=[0, z0], path=[[0, z1]], speed_ms=speed, size_x=4 + 3 * cars, hull=hull,
                weak_points=weak, weak_ap=car_ap, weak_radius_m=5, core=[0, 13, 2], core_ap=engine_ap, core_radius_m=6,
                turrets=tur, escapes=True)

def light(x, z, h, path, r=14, speed=10):
    return dict(at=[x, z, h], path=[list(p) for p in path], r_m=r, speed_ms=speed)

def stage(objective, wave=0, reach=None, fight=True, hold=0):
    d = dict(objective=objective, wave=wave)
    if reach: d["reach"] = list(reach)
    if not fight: d["fight"] = False
    if hold: d["hold_s"] = hold
    return d

nest = (-450, 100)
NEST_LIGHTS = [light(nest[0] - 110, nest[1] - 60, 40, [(nest[0] - 60, nest[1] - 40), (nest[0] - 60, nest[1] + 70)], speed=9),
               light(nest[0] + 110, nest[1] - 40, 40, [(nest[0] + 70, nest[1] - 50), (nest[0] + 10, nest[1] + 20)], speed=11),
               light(nest[0], nest[1] + 150, 44, [(nest[0] - 40, nest[1] + 90), (nest[0] + 50, nest[1] + 100)], speed=8),
               light(nest[0], nest[1] - 150, 44, [(nest[0] - 30, nest[1] - 80), (nest[0] + 40, nest[1] - 90)], speed=12)]
NEST_TOWERS = [block(l["at"][0], l["at"][1], 6, 6, l["at"][2] - 3) for l in NEST_LIGHTS]

# Spindle's escape: a full turn of the ring from the hub to the dock, with slabs coming down
ESCAPE_ROUTE = [(200, 400), (500, 750), (-480, 1050), (-200, 1350), (0, 1600)]
def slab(x, z, w, d, after_s, h=60, drop=100):
    return machine("slab", x, z, w, d, h, base=drop, path=[(0, 0, 0), (0, 0, -drop)], speed=45) | {"after_ms": after_s * 1000}
ESCAPE_SLABS = [slab(100, 230, 90, 12, 3), slab(350, 575, 90, 12, 8), slab(200, 330, 60, 60, 12),
                slab(530, 900, 90, 12, 13), slab(-350, 1200, 90, 12, 19), slab(-480, 1000, 60, 60, 22),
                slab(-100, 1475, 90, 12, 25), slab(0, 1500, 50, 12, 30)]
GENERATORS = [(300, 250), (-300, 250), (300, -250), (-300, -250)]

# Cinder: a turret field the lava climbs through, and the foundry's insides
ASH_GUNS = [(int(220 * math.sin(2 * math.pi * k / 6)), 500 + int(220 * math.cos(2 * math.pi * k / 6)), 84 + (k % 3) * 6) for k in range(6)]
ASH_STEPS = [(0, 700, 40), (-90, 640, 52), (90, 620, 58), (-150, 520, 64), (150, 470, 70), (0, 500, 76),
             (-60, 360, 78), (80, 330, 80)]
ASH_BLOCKS = ([block(0, 800, 40, 40, 30)] + [block(x, z, 16, 16, h) for x, z, h in ASH_GUNS]
              + [block(x, z, 22, 22, h) for x, z, h in ASH_STEPS])
F = (-400, -200)
def fx_(dx, dz): return (F[0] + dx, F[1] + dz)
FOUNDRY = ([block(*fx_(0, 0), 160, 160, 2)]                                   # floor, above the burning ground
           + [block(*fx_(-55, 80), 50, 6, 36), block(*fx_(55, 80), 50, 6, 36)]     # north wall, its door between
           + [block(*fx_(-50, -80), 60, 6, 36), block(*fx_(50, -80), 60, 6, 36)]   # south wall, the vent between
           + [block(*fx_(-80, 0), 6, 160, 36), block(*fx_(80, 0), 6, 160, 36)]     # west and east walls
           + [block(*fx_(0, 0), 166, 166, 6, 36)]                                 # roof
           + [block(*fx_(-15, 37), 4, 74, 34, 2), block(*fx_(15, 37), 4, 74, 34, 2)]   # corridor, north half
           + [block(*fx_(-15, -30), 4, 46, 34, 2), block(*fx_(15, -30), 4, 46, 34, 2)] # corridor, south half
           + [block(*fx_(-47, -40), 60, 4, 34, 2), block(*fx_(47, -40), 60, 4, 34, 2)] # rooms' south walls
           + [block(*fx_(0, 130), 30, 100, 2)])                                   # the causeway to the door
FOUNDRY_LIGHTS = [light(*fx_(0, 70), 30, [fx_(0, 55), fx_(0, -5)], r=9, speed=7),
                  light(*fx_(-70, 70), 30, [fx_(-55, 40), fx_(-55, -20)], r=9, speed=6),
                  light(*fx_(70, 70), 30, [fx_(55, 40), fx_(55, -20)], r=9, speed=8),
                  light(*fx_(-60, -75), 30, [fx_(-30, -60), fx_(30, -60)], r=9, speed=9)]
REACTORS = [fx_(-50, 10), fx_(50, 10), fx_(0, -60)]
FOUNDRY_MINES = [fx_(0, 30), fx_(-35, 0), fx_(35, -10), fx_(0, -25), fx_(-50, -60), fx_(50, -60)]

# Rime: the great needle, and the route across the ice
NEEDLE_TOP = 260
LEDGES = [(int(40 * math.cos(math.radians(60 * k + 20))), int(40 * math.sin(math.radians(60 * k + 20))), 36 + 36 * k) for k in range(6)]
NEEDLE = ([block(0, 0, 30, 30, NEEDLE_TOP), block(0, 0, 60, 60, 6, NEEDLE_TOP)]
          + [block(x, z, 18, 18, 4, b) for x, z, b in LEDGES])
NEEDLE_LIFT = machine("lift", -45, 45, 16, 16, 4, base=222, path=[(0, 0, 0), (0, 0, 36)], speed=8, wait=1000)
CROSSING = [(400, -760), (200, -600), (0, -420), (-200, -560), (-400, -700)]

planets = []

# ---------------- 1 HALDEN ----------------
yards = (-360, -300)
vault = (350, -800)
# lancers on towers beside the avenue, and on the vault's corners
LANCER_TOWERS = [(-60, 460, 60), (60, 310, 70), (-60, 160, 55)]
VAULT_TOWERS = [(vault[0] - 70, vault[1] - 70, 40), (vault[0] + 70, vault[1] - 60, 46)]
planets.append(dict(
  id="halden", name="HALDEN", opens_at=0,
  blurb="An industrial world of foundries and freight yards, and home port. Its towers are lit all night.",
  gimmick="Standard gravity. A good place to learn the frame.",
  star=dict(color=[86, 140, 210], band=[210, 200, 170], size=100, rings=False, moons=1, glow=0),
  look=look((148, 166, 178), (160, 172, 178), (104, 100, 92), (84, 80, 74),
            [(120, 122, 124), (138, 134, 126), (100, 106, 112), (146, 142, 136), (86, 92, 98)],
            (70, 72, 76), (228, 196, 128), True, (-40, 100, 30), 300, 1400),
  climate=dict(gravity_pct=100, traction_pct=100, floor_dps=0),
  map=dict(seed=20261008, width_m=1400, length_m=2400, cell_m=80, street_m=24, empty_pct=20, min_h_m=12, max_h_m=60,
           tall_pct=12, tall_h_m=130, ceiling_m=420, avenue_m=70, style="towers",
           clearings=[dict(x_m=0, z_m=1000, r_m=70), dict(x_m=yards[0], z_m=yards[1], r_m=160), dict(x_m=350, z_m=-800, r_m=200)]),
  missions=[
    m("halden-1", "GUNSHIP SWEEP", "Two contractor gunships are circling the freight avenue. Clear the sky over Halden.",
      "destroy", [0, 1000, 0], "hd-kestrel", plus("max_rounds", 10, "ml-hornet-x") if False else plus("max_rounds", 10, "cr-vane"),
      units=[unit("heli", -60, 820, 70), unit("heli", 80, 760, 85)], time_limit_s=300),
    m("halden-2", "SHAKEDOWN RUN", "Your old wingmate has built a course down the avenue: rings to thread, a tower to land on, a door only a switch will open. Friendly race, no weapons.",
      "race", [-14, 1060, 0], "ar-reed", plus("under_seconds", 33, "lg-wisp"), weapons=False,
      mechs=[dict(name="WREN · FRIEND", pilot="racer", at=[14, 1060, 0], ap_pct=100, racer=True,
                  loadout=dict(head="hd-warden", core="cr-citadel", arms="ar-lancer", legs="lg-stride", booster="bt-titan", generator="gn-kiln", right_weapon="rf-marrow", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))],
      course=HALDEN_COURSE, machines=HALDEN_MACHINES, pads=HALDEN_BLOCKS, time_limit_s=180),
    m("halden-3", "HOLD THE YARDS", "An invading column is rolling on the freight yards. If more than three depots fall, Halden's supply line goes with them.",
      "defend", [yards[0], yards[1] + 155, 0], "bt-comet", plus("max_lost", 1, "gn-ember"), requires=["halden-1"],
      protect=[dict(at=[yards[0] + dx, yards[1] + dz], size=[22, 22, h], ap=2600) for dx, dz, h in
               [(0, 0, 34), (-70, -50, 22), (70, -50, 22), (-80, 40, 18), (80, 40, 18), (0, -95, 26)]],
      may_lose=3,
      units=ring(yards[0], yards[1], 150, 4, "tank", 0, 0) + ring(yards[0], yards[1], 150, 4, "tank", 0, 1, 0.4) + ring(yards[0], yards[1], 120, 2, "heli", 70, 1)
            + ring(yards[0], yards[1], 150, 6, "tank", 0, 2, 0.2), time_limit_s=420),
    m("halden-4", "MIRROR MATCH", "A rival contractor has copied your frame part for part. Prove the pilot matters.",
      "duel", [350, -640, 0], "sg-brand", plus("min_ap_pct", 50, "mp-swarm"), requires=["halden-2"],
      mechs=[dict(name="MIRROR", pilot="duelist", at=[350, -960, 180], ap_pct=100)], time_limit_s=300),
    m("halden-5", "THE BREAKWATER", "The contractor walled off Halden's south port: a rampart a hundred and eighty metres high, guns along its crest, one gate tunnel through it. Clear the approach, break through the gate, climb the inner face, and take the guardian on top.",
      "assault", [0, -300, 0], "lc-lance", plus("under_seconds", 300, "cr-rampart"),
      requires=["halden-1", "halden-2", "halden-3", "halden-4"], time_limit_s=720, power_pct=115,
      clear=[[-700, -1200, 700, -820]],
      pads=[dict(at=[-363, -880], size=[676, 60, 180]), dict(at=[363, -880], size=[676, 60, 180]),
            dict(at=[0, -880], size=[50, 60, 150], base=30),
            dict(at=[0, -1070], size=[1400, 120, 180])]
           + [dict(at=[x, -995], size=[70, 30, 6], base=top - 6) for x, top in [(-140, 40), (120, 80), (-90, 120), (150, 160)]]
           + [dict(at=[x, z], size=[14, 14, 10], base=180) for x, z in [(-120, -1050), (110, -1090), (0, -1030), (-40, -1110), (180, -1040)]],
      stages=[dict(objective="Clear the approach to the wall", wave=0),
              dict(objective="Break through the gate tunnel", wave=1, reach=[0, -960, 5, 45]),
              dict(objective="Climb the inner face to the top", wave=2, reach=[0, -1070, 200, 170]),
              dict(objective="Destroy the wall's guardian", wave=3)],
      units=[unit("tank", -20, -640, 0, 0), unit("tank", 20, -690, 0, 0), unit("tank", 0, -760, 0, 0),
             unit("turret", -160, -870, -1, 0), unit("turret", 0, -900, -1, 0), unit("turret", 170, -870, -1, 0),
             unit("heli", -80, -700, 90, 0), unit("heli", 90, -760, 110, 0),
             unit("turret", -14, -868, 0, 1), unit("turret", 14, -896, 0, 1),
             unit("turret", -140, -995, -1, 2), unit("turret", 150, -995, -1, 2),
             unit("heli", -60, -960, 100, 2), unit("heli", 60, -960, 130, 2),
             unit("heli", -150, -1080, 220, 3), unit("heli", 150, -1080, 220, 3)],
      mechs=[dict(name="GATEKEEPER", pilot="brute", at=[0, -1105, 0], ap_pct=170, wave=3,
                  loadout=dict(head="hd-mantle", core="cr-citadel", arms="ar-bastion", legs="lg-bastion", booster="bt-titan",
                               generator="gn-forge", right_weapon="sg-brand", left_weapon="rf-marrow", shoulder_weapon="gc-anvil"))]),
    m("halden-6", "LONG NIGHT", "The contractor holds Halden from end to end, and it has brought new hunters: lancers that snipe from the towers with a beam you can see coming, and stalkers that run you down on four legs. Take the city back in one night: the north checkpoint, the avenue's towers, the freight yards, the data vault, and out by the south lift.",
      "assault", [0, 1150, 0], "lr-lancet", plus("under_seconds", 420, "lg-stalker"), requires=["halden-5"],
      pads=[block(-30, 880, 20, 6, 8), block(30, 860, 20, 6, 8)] + [block(x, z, 16, 16, h) for x, z, h in LANCER_TOWERS]
           + [block(x, z, 14, 14, h) for x, z, h in VAULT_TOWERS],
      clear=[[x - 14, z - 14, x + 14, z + 14] for x, z, _ in LANCER_TOWERS],
      resupply=[dict(at=[yards[0], yards[1] + 110, 0], r_m=12)],
      units=[unit("tank", -20, 840, 0), unit("tank", 20, 830, 0), unit("tank", 0, 800, 0),
             unit("turret", -30, 880, -1), unit("turret", 30, 860, -1), unit("heli", 0, 760, 70)]
            + [unit("lancer", x, z, -1, 1) for x, z, _ in LANCER_TOWERS] + [unit("heli", -40, 300, 80, 1), unit("heli", 40, 200, 90, 1)]
            + [unit("stalker", yards[0] + dx, yards[1] + dz, 0, 2) for dx, dz in [(-60, -40), (60, -30), (0, 70)]]
            + [unit("tank", yards[0] - 30, yards[1] - 90, 0, 2), unit("tank", yards[0] + 40, yards[1] - 100, 0, 2)]
            + [unit("lancer", x, z, -1, 3) for x, z, _ in VAULT_TOWERS]
            + [unit("stalker", vault[0] - 60, vault[1] + 40, 0, 3), unit("stalker", vault[0] + 60, vault[1] + 30, 0, 3),
               unit("turret", vault[0] - 20, vault[1] - 30, 0, 3), unit("turret", vault[0] + 20, vault[1] - 30, 0, 3)]
            + [unit("stalker", vault[0] + dx, vault[1] + 120, 0, 4) for dx in (-40, 0, 40)]
            + [unit("heli", -60, -1000, 80, 4), unit("heli", 60, -1050, 90, 4)],
      stages=[stage("Take the north checkpoint"),
              stage("Silence the lancers on the avenue towers", wave=1),
              stage("Fight through the freight yards", wave=2, reach=[yards[0], yards[1], 10, 90]),
              stage("Scan the data vault under its lancers", wave=3, reach=[vault[0], vault[1], 6, 16], hold=4),
              stage("Reach the south lift: they are coming", wave=4, reach=[0, -1150, 10, 40], fight=False)],
      time_limit_s=900, power_pct=110),
  ]))

# ---------------- 2 SERE ----------------
planets.append(dict(
  id="sere", name="SERE", opens_at=2,
  blurb="A desert of red mesas under a sky full of sand. You will hear things before you see them.",
  gimmick="Sandstorm: you see a few hundred metres at most.",
  star=dict(color=[214, 120, 60], band=[240, 190, 120], size=118, rings=False, moons=2, glow=0),
  look=look((196, 140, 92), (190, 132, 84), (176, 128, 84), (150, 104, 66),
            [(168, 92, 58), (150, 80, 52), (184, 108, 68), (136, 74, 50)],
            (120, 66, 44), (255, 200, 120), False, (60, 100, -20), 80, 520, structure=(255, 210, 120)),
  climate=dict(gravity_pct=100, traction_pct=90, floor_dps=0),
  map=dict(seed=7713, width_m=1800, length_m=2400, cell_m=110, street_m=30, empty_pct=45, min_h_m=20, max_h_m=70,
           tall_pct=8, tall_h_m=110, ceiling_m=420, avenue_m=90, style="mesas",
           clearings=[dict(x_m=0, z_m=1000, r_m=80), dict(x_m=-450, z_m=100, r_m=120), dict(x_m=450, z_m=-800, r_m=200)]),
  missions=[
    m("sere-1", "CONVOY BREAK", "An armoured train is running the canyon south with stolen reactor cores. Break its four cars before it reaches the tunnel; land on its roof to keep pace. The engine opens when the cars are gone.",
      "intercept", [0, 1000, 0], "lg-bastion", plus("under_seconds", 80, "ar-bastion"),
      fortresses=[train("IRON SERPENT · TRAIN", 760, -1040, 4, 2600, 4200, 15)],
      units=[unit("heli", -60, 600, 70), unit("heli", 60, 500, 80)],
      clear=[[-30, -1100, 30, 1000]],
      pads=[block(-60, -1090, 90, 40, 60), block(60, -1090, 90, 40, 60), block(0, -1090, 30, 40, 36, 24)],
      time_limit_s=240),
    m("sere-2", "GLASS NEST", "Searchlights sweep the turret nest that closed the western pass. Slip in under the storm, scan its three caches, and get clear. If a light finds you, the nest wakes, and so does what sleeps under it.",
      "recon", [-330, 330, 200], "hd-mantle", plus("max_alarms", 0, "cr-citadel"),
      units=ring(-450, 100, 170, 6, "turret", -1, 1) + [unit("heli", -500, 60, 80, 1), unit("heli", -400, 140, 90, 1)],
      searchlights=NEST_LIGHTS, alarm_wave=1, pads=NEST_TOWERS,
      stages=[stage("Scan the east cache", reach=[nest[0] + 60, nest[1] + 10, 4, 12], fight=False, hold=3),
              stage("Scan the south cache", reach=[nest[0] - 10, nest[1] - 70, 4, 12], fight=False, hold=3),
              stage("Scan the west cache", reach=[nest[0] - 70, nest[1] + 60, 4, 12], fight=False, hold=3),
              stage("Get clear of the nest", reach=[nest[0], nest[1] + 380, 10, 70], fight=False)],
      time_limit_s=300),
    m("sere-3", "DUNE GAUNTLET", "The canyon racers of Sere run a gauntlet: pistons that slam the canyon floor, a slalom of walls, a mesa stair to land your way up. One has challenged you.",
      "race", [-14, 1050, 0], "bt-titan", plus("under_seconds", 44, "rj-hare"), weapons=False, requires=["sere-1"],
      mechs=[dict(name="DUST · RIVAL", pilot="racer_hot", speed_pct=82, at=[14, 1050, 0], ap_pct=100, racer=True,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-ram",
                               generator="gn-ember", right_weapon="rf-marrow", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))],
      course=SERE_COURSE, machines=SERE_MACHINES, pads=SERE_BLOCKS, clear=corridors([-14, 1050], SERE_COURSE, 50), time_limit_s=180),
    m("sere-4", "THE SCORPION", "A routine job: survey drones have strayed over the southern pass. Shoot them down and come home.",
      "ambush", [450, -640, 0], "gn-forge", plus("under_seconds", 130, "lr-glint"), requires=["sere-2", "sere-3"],
      units=ring(450, -820, 90, 6, "drone", 30) + [unit("tank", 420, -900, 0), unit("tank", 480, -900, 0)],
      stages=[stage("Shoot down the stray drones"),
              stage("AMBUSH — the Scorpion and its spotter", wave=1)],
      mechs=[dict(name="SCORPION", pilot="brute", at=[450, -960, 180], ap_pct=110, wave=1,
                  loadout=dict(head="hd-mantle", core="cr-citadel", arms="ar-bastion", legs="lg-bastion", booster="bt-titan",
                               generator="gn-forge", right_weapon="sg-brand", left_weapon="rf-marrow", shoulder_weapon="gc-anvil")),
             dict(name="STINGER", pilot="keeper", at=[560, -940, 180], ap_pct=65, wave=1,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="rj-hare", booster="bt-comet",
                               generator="gn-ember", right_weapon="ml-hornet", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))],
      time_limit_s=360),
  ]))
# lc-halo is the second plus of Sere's duel? keep: sere-4 plus gives lr-glint; lc-halo goes to sere-2? (set below)

# ---------------- 3 SPINDLE (ring) ----------------
planets.append(dict(
  id="spindle", name="SPINDLE", opens_at=5,
  blurb="A ring of a world that broke apart, rebuilt around its own wreckage and spun for gravity. The ground curves up overhead.",
  gimmick="Ring gravity: you are thrown outward onto the inside of the ring. Run far enough sideways and you come back round.",
  star=dict(color=[150, 160, 176], band=[110, 230, 255], size=92, rings=True, moons=0, glow=0),
  look=look((14, 18, 26), (26, 32, 44), (72, 78, 88), (110, 230, 255),
            [(90, 94, 102), (70, 74, 82), (110, 104, 96), (58, 62, 70)],
            (40, 44, 50), (110, 230, 255), True, (0, 100, 20), 260, 1200, stars=True, structure=(255, 170, 80)),
  climate=dict(gravity_pct=100, traction_pct=100, floor_dps=0),
  map=dict(seed=5151, width_m=1100, length_m=3600, cell_m=80, street_m=26, empty_pct=55, min_h_m=8, max_h_m=40,
           tall_pct=10, tall_h_m=95, ceiling_m=150, style="ruins", wrap=True,
           clearings=[dict(x_m=0, z_m=1600, r_m=80), dict(x_m=0, z_m=0, r_m=150), dict(x_m=0, z_m=-1400, r_m=160)]),
  missions=[
    m("spindle-1", "SPIN-UP", "A coolant line has burst at the hub and the ring is shaking itself apart. Run a full turn of the Spindle to the dock before the sections behind you, and ahead of you, come down.",
      "escape", [0, 60, 180], "hd-osprey", plus("under_seconds", 55, "cr-spire"),
      units=[unit("heli", 300, 500, 60), unit("heli", -520, 950, 70), unit("heli", -260, 1300, 60)],
      machines=ESCAPE_SLABS,
      clear=corridors([0, 60], [dict(at=[x, z]) for x, z in ESCAPE_ROUTE], 50, 550),
      stages=[stage("Get clear of the hub", reach=[200, 400, 10, 60], fight=False),
              stage("Run the spin", reach=[500, 750, 10, 60], fight=False),
              stage("Round the seam", reach=[-480, 1050, 10, 60], fight=False),
              stage("Keep running", reach=[-200, 1350, 10, 60], fight=False),
              stage("Reach the dock", reach=[0, 1600, 10, 50], fight=False)],
      time_limit_s=90),
    m("spindle-2", "COLD FLOW", "The hub's coolant plant sits under a shield dome, and nothing you fire gets through. Four generators round the ring hold it up, and the Ring Hulk walks between them. Break the generators, then the plant.",
      "destroy", [0, 420, 0], "ar-heron", plus("min_ap_pct", 60, "lg-gale"),
      shields=[dict(at=[0, 0, 0], r_m=60, generators=[[x, z, -1] for x, z in GENERATORS])],
      units=[dict(type="core", at=[0, 0, 22], ap=9000)] + [unit("turret", x + 30, z, -1) for x, z in GENERATORS]
            + ring(0, 0, 220, 4, "heli", 70),
      clear=[[x - 40, z - 40, x + 40, z + 40] for x, z in GENERATORS], time_limit_s=360),
    m("spindle-3", "LONG WAY ROUND", "The ring's couriers race a full turn of the Spindle on platforms that move. Land on the lifts, ride them up, and keep your head; the floor goes up.",
      "race", [-14, 1650, 0], "bt-flare", plus("under_seconds", 45, "rf-gatling"), weapons=False, requires=["spindle-1"],
      mechs=[dict(name="ORBIT · COURIER", pilot="racer_hot", speed_pct=85, at=[14, 1650, 0], ap_pct=100, racer=True,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-surge",
                               generator="gn-corona", right_weapon="rf-marrow", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))],
      course=SPIN_COURSE, machines=SPIN_MACHINES, pads=SPIN_BLOCKS,
      clear=corridors([-14, 1650], SPIN_COURSE, 40, 550) + [[g["at"][0] - 40, g["at"][1] - 40, g["at"][0] + 40, g["at"][1] + 40] for g in SPIN_COURSE],
      time_limit_s=200),
    m("spindle-4", "KEEPER OF THE SPINDLE", "The ring's keeper flies close to the axis and calls down shells on wherever you are heading, while two wardens hunt you along the floor. Watch the ground: a ring of light is where the next one lands.",
      "duel", [0, -1250, 0], "bz-maul", plus("under_seconds", 150, "gn-corona"), requires=["spindle-2", "spindle-3"],
      artillery=dict(every_ms=6000, warn_ms=2000, radius_m=16, damage=600),
      mechs=[dict(name="KEEPER", pilot="keeper", at=[0, -1550, 180], ap_pct=100,
                  loadout=dict(head="hd-kestrel", core="cr-bulwark", arms="ar-lancer", legs="rj-hare", booster="bt-comet",
                               generator="gn-forge", right_weapon="ml-hornet", left_weapon="rf-marrow", shoulder_weapon="mp-swarm")),
             dict(name="WARDEN A", pilot="twin", at=[-60, -1500, 180], ap_pct=55,
                  loadout=dict(head="hd-warden", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-surge",
                               generator="gn-ember", right_weapon="sg-brand", left_weapon="rf-marrow", shoulder_weapon="gc-anvil")),
             dict(name="WARDEN B", pilot="twin", at=[60, -1500, 180], ap_pct=55,
                  loadout=dict(head="hd-warden", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-surge",
                               generator="gn-ember", right_weapon="ml-hornet", left_weapon="rf-marrow", shoulder_weapon="gc-anvil"))],
      time_limit_s=360),
  ]))

# ---------------- 4 RIME (ice moon) ----------------
relay = (-400, -700)
planets.append(dict(
  id="rime", name="RIME", opens_at=8,
  blurb="A pale moon of ice needles. Gravity is light and the ground gives no grip.",
  gimmick="Low gravity and ice: jumps soar, and a frame slides into and out of every move.",
  star=dict(color=[220, 236, 250], band=[150, 200, 240], size=86, rings=False, moons=0, glow=0),
  look=look((8, 12, 26), (40, 56, 82), (214, 226, 236), (170, 196, 222),
            [(196, 220, 240), (170, 204, 236), (226, 236, 246), (150, 188, 226)],
            (240, 248, 255), (120, 200, 255), True, (30, 100, 60), 360, 1700, stars=True, structure=(255, 120, 90)),
  climate=dict(gravity_pct=40, traction_pct=22, floor_dps=0),
  map=dict(seed=313, width_m=1600, length_m=2200, cell_m=90, street_m=26, empty_pct=35, min_h_m=30, max_h_m=110,
           tall_pct=25, tall_h_m=220, ceiling_m=520, style="spires",
           clearings=[dict(x_m=0, z_m=900, r_m=90), dict(x_m=0, z_m=0, r_m=200), dict(x_m=relay[0], z_m=relay[1], r_m=160),
                      dict(x_m=400, z_m=-700, r_m=200)]),
  missions=[
    m("rime-1", "WHITEOUT", "Three guns on the summit of Rime's great needle fire down its whole length. Climb it, ledge by ledge and up the lift, past the mines; silence the guns; and the Frost Colossus waits on top.",
      "climb", [0, 160, 0], "hd-bastille", plus("under_seconds", 180, "cr-anvil"),
      pads=NEEDLE, machines=[NEEDLE_LIFT],
      units=[unit("turret", 22, 0, -1), unit("turret", -22, 0, -1), unit("turret", 0, -22, -1)]
            + [unit("mine", x, z, b + 4) for x, z, b in LEDGES[1::2]]
            + ring(0, 0, 90, 2, "heli", 140),
      stages=[stage("Climb the needle", reach=[0, 0, NEEDLE_TOP + 10, 32], fight=False),
              stage("Silence the summit guns"),
              stage("The Frost Colossus", wave=1)],
      time_limit_s=360),
    m("rime-2", "SKATER", "Rime's racers land on the needle tops in low gravity and thread the ice walls. Little grip, less margin.",
      "race", [-14, 950, 0], "ar-grip", plus("under_seconds", 62, "rj-lynx"), weapons=False,
      mechs=[dict(name="FROST · SKATER", pilot="racer_hot", at=[14, 950, 0], ap_pct=100, racer=True,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-surge",
                               generator="gn-corona", right_weapon="rf-marrow", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))],
      course=RIME_COURSE, machines=RIME_MACHINES, pads=RIME_BLOCKS, clear=corridors([-14, 950], RIME_COURSE, 45), time_limit_s=180),
    m("rime-3", "THAW LINE", "A crawler is carrying the relay core across the ice to the western shelf. Raiders want the core, not you: keep the crawler alive to the end of its route.",
      "escort", [440, -720, 90], "bt-ram", plus("escort_ap_pct", 70, "gn-reactor"), requires=["rime-1"],
      escort=dict(name="RELAY CRAWLER", route=[list(p) for p in CROSSING], speed_ms=7, ap=7000, radius_m=8),
      units=[unit("tank", 150, -520, 0), unit("tank", 230, -500, 0), unit("heli", 120, -660, 70)]
            + [unit("tank", -60, -330, 0, 1), unit("tank", 40, -320, 0, 1), unit("tank", -20, -520, 0, 1), unit("heli", 0, -380, 80, 1)]
            + [unit("tank", -300, -520, 0, 2), unit("tank", -320, -640, 0, 2), unit("heli", -260, -600, 70, 2), unit("heli", -340, -560, 90, 2)],
      clear=corridors([440, -760], [dict(at=list(p)) for p in CROSSING], 60) + [[-560, -860, -240, -540], [240, -900, 560, -600]],
      time_limit_s=300),
    m("rime-4", "TWIN SHADOWS", "Two light frames that fight as one, inside the storm front where nothing locks. No lock-on, and the storm drains EN: fight them by eye.",
      "duel", [400, -540, 0], "sg-thunder", plus("min_ap_pct", 40, "mp-hydra"), requires=["rime-2", "rime-3"],
      jam=[dict(at=[400, -700, 420], drain_pct=4)],
      mechs=[dict(name="SHADOW A", pilot="twin", at=[340, -880, 180], ap_pct=80,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-comet",
                               generator="gn-ember", right_weapon="sg-brand", left_weapon="rf-marrow", shoulder_weapon="mp-swarm")),
             dict(name="SHADOW B", pilot="twin", at=[460, -880, 180], ap_pct=80,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-comet",
                               generator="gn-ember", right_weapon="ml-hornet", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))],
      time_limit_s=360),
  ]))

# ---------------- 5 CINDER (lava) ----------------
foundry = (-400, -200)
planets.append(dict(
  id="cinder", name="CINDER", opens_at=11,
  blurb="A world of basalt columns standing in lava. The air is ash; the ground is death.",
  gimmick="The floor burns: every second on the bare ground costs AP. Stay on the columns or in the air.",
  star=dict(color=[60, 30, 26], band=[255, 110, 30], size=110, rings=False, moons=1, glow=1),
  look=look((44, 18, 14), (70, 30, 20), (255, 96, 30), (255, 190, 80),
            [(52, 46, 46), (40, 36, 38), (64, 56, 52), (34, 30, 32)],
            (90, 40, 30), (255, 120, 40), False, (-20, 100, -40), 160, 900, glow=70, structure=(120, 220, 255)),
  climate=dict(gravity_pct=110, traction_pct=100, floor_dps=240),
  map=dict(seed=6660, width_m=1600, length_m=2200, cell_m=70, street_m=22, empty_pct=30, min_h_m=15, max_h_m=80,
           tall_pct=10, tall_h_m=140, ceiling_m=480, style="pillars",
           clearings=[dict(x_m=foundry[0], z_m=foundry[1], r_m=140), dict(x_m=300, z_m=-700, r_m=180)]),
  missions=[
    m("cinder-1", "ASHFALL", "The lava is rising through the column field, and turrets on the tallest columns are shelling the evacuation. Silence them before it reaches you; climb as it climbs.",
      "destroy", [0, 800, 0], "hd-sentinel", plus("max_floor_seconds", 3, "cr-lattice"),
      pads=ASH_BLOCKS, lava=dict(from_m=0, to_m=74, over_s=150, dps=420),
      units=[unit("turret", x, z, -1) for x, z, _ in ASH_GUNS] + ring(0, 500, 150, 3, "heli", 100, 1), time_limit_s=240),
    m("cinder-2", "EMBER RUN", "Column to column over the lava, up a rising lift, under the pistons and through a vault only a switch opens. Touch the lava and it will cost you more than time.",
      "race", [-14, 900, 0], "ar-titan", plus("under_seconds", 67, "lg-monolith"), weapons=False,
      pads=CINDER_BLOCKS,
      mechs=[dict(name="SPARK · RIVAL", pilot="racer_hot", speed_pct=80, at=[14, 900, 0], ap_pct=100, racer=True,
                  loadout=dict(head="hd-warden", core="cr-bulwark", arms="ar-lancer", legs="rj-hare", booster="bt-surge", generator="gn-kiln", right_weapon="rf-marrow", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))],
      course=CINDER_COURSE, machines=CINDER_MACHINES, time_limit_s=180,
      clear=corridors([0, 900], CINDER_COURSE, 35)),
    m("cinder-3", "LAST FOUNDRY", "The contractor has taken Cinder's last foundry to build frames against us. Go inside: break its three reactors, stay out of the lights, and get out by the south vent. An alarm seals the door you came in by.",
      "sabotage", [F[0], F[1] + 170, 0], "bt-wisp", plus("under_seconds", 140, "lr-spear"), requires=["cinder-1"],
      pads=FOUNDRY, searchlights=FOUNDRY_LIGHTS, alarm_wave=2,
      machines=[machine("door", F[0], F[1] + 80, 22, 6, 34, base=36, path=[(0, 0, 0), (0, 0, -34)], speed=30) | {"on_alarm": True}],
      units=[dict(type="core", at=[x, z, 10], ap=4000, wave=1) for x, z in REACTORS]
            + [unit("mine", x, z, -1) for x, z in FOUNDRY_MINES]
            + [unit("tank", *fx_(-50, 30), -1, 2), unit("tank", *fx_(50, 30), -1, 2), unit("tank", *fx_(0, -60), -1, 2)]
            + [unit("heli", *fx_(-120, -130), 70, 2), unit("heli", *fx_(120, -130), 70, 2)],
      stages=[stage("Get inside the foundry", reach=[F[0], F[1] + 60, 6, 20], fight=False),
              stage("Break the three reactors", wave=1),
              stage("Get out by the south vent", reach=[F[0], F[1] - 140, 10, 40], fight=False)],
      clear=[[F[0] - 120, F[1] - 120, F[0] + 120, F[1] + 220]],
      time_limit_s=300, power_pct=110),
    m("cinder-4", "IRON VECTOR", "The contractor's ace, in the best frame money can buy, with two gunships at its shoulders. Its pulse armour turns every hit until it breaks; then you have seconds before it is whole again. This is the job.",
      "duel", [300, -560, 0], "gc-quake", plus("min_ap_pct", 50, "bt-ram-x") if False else plus("min_ap_pct", 50, "gc-quake-x") if False else plus("min_ap_pct", 50, "lg-bastion-x") if False else plus("min_ap_pct", 50, "ar-titan-x") if False else plus("min_ap_pct", 50, "PLACEHOLDER"),
      requires=["cinder-2", "cinder-3"],
      pads=[dict(at=[300, -560], size=[50, 40, 30]), dict(at=[300, -860], size=[50, 40, 30])],
      mechs=[dict(name="ACE · IRON VECTOR", pilot="ace", at=[300, -860, 180], ap_pct=110, pulse_ap=2600, pulse_down_ms=7000,
                  loadout=dict(head="hd-sentinel", core="cr-lattice", arms="ar-grip", legs="rj-lynx", booster="bt-ram",
                               generator="gn-reactor", right_weapon="lr-spear", left_weapon="rf-marrow", shoulder_weapon="mp-hydra"))],
      units=[unit("heli", 220, -820, 90), unit("heli", 380, -820, 90)], time_limit_s=420, power_pct=130),
  ]))

# ---------------- THE ARENA: a ladder of duels ----------------
LIGHT = dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-comet", generator="gn-ember")
HEAVY = dict(head="hd-mantle", core="cr-citadel", arms="ar-bastion", legs="lg-bastion", booster="bt-titan", generator="gn-forge")
def rival(name, pilot, ap, frame, r, l, s, **kw):
    return dict(name=name, pilot=pilot, at=[0, -220, 180], ap_pct=ap,
                loadout=dict(frame, right_weapon=r, left_weapon=l, shoulder_weapon=s), **kw)
def rank(n, name, scene, foe, plus_, req):
    return m("arena-%d" % n, name, scene, "duel", [0, 220, 0], "PLACEHOLDER", plus_, mechs=[foe],
             requires=[req] if req else [], time_limit_s=240)
planets.append(dict(
  id="arena", name="THE ARENA", opens_at=3, arena=True,
  blurb="An old orbital hangar the contractors fight in for rank. Five named frames stand between you and the top of the board.",
  gimmick="One on one, rank by rank. Each rank you take pays a part built for the arena.",
  star=dict(color=[200, 150, 80], band=[255, 200, 120], size=70, rings=False, moons=0, glow=0),
  look=look((20, 16, 30), (34, 26, 44), (44, 42, 52), (255, 170, 60),
            [(64, 58, 74), (52, 48, 62), (74, 66, 84)],
            (86, 74, 96), (255, 170, 60), True, (-30, 100, 20), 260, 900, glow=12, stars=True, structure=(255, 170, 60)),
  climate=dict(gravity_pct=100, traction_pct=100, floor_dps=0),
  map=dict(seed=777, width_m=640, length_m=640, cell_m=80, street_m=30, empty_pct=70, min_h_m=8, max_h_m=30,
           tall_pct=12, tall_h_m=60, ceiling_m=160, style="pillars", clearings=[dict(x_m=0, z_m=0, r_m=70)]),
  missions=[
    rank(1, "RANK E · RUST HOUND", "The board's lowest rank, and hungry. A light rifle frame that never stops moving.",
         rival("RUST HOUND", "duelist", 80, LIGHT, "rf-marrow", "rf-marrow", "mp-swarm"), plus("min_ap_pct", 70, "PLACEHOLDER"), None),
    rank(2, "RANK D · IRONJAW", "A heavy shotgun frame that wants to be close enough to touch.",
         rival("IRONJAW", "brute", 95, HEAVY, "sg-brand", "rf-marrow", "gc-anvil"), plus("under_seconds", 70, "PLACEHOLDER"), "arena-1"),
    rank(3, "RANK C · SKYLARK", "It fights from above and lets its missiles do the work.",
         rival("SKYLARK", "keeper", 105, LIGHT, "ml-hornet", "rf-marrow", "mp-swarm"), plus("min_ap_pct", 60, "PLACEHOLDER"), "arena-2"),
    rank(4, "RANK B · NIGHTCALL", "A contractor's ace on loan to the board. It reads you as you read it.",
         rival("NIGHTCALL", "ace", 115, dict(LIGHT, legs="rj-hare", booster="bt-surge"), "lr-glint", "rf-marrow", "mp-swarm"),
         plus("under_seconds", 90, "PLACEHOLDER"), "arena-3"),
    rank(5, "RANK A · THE CHAMPION", "Top of the board, in pulse armour. Break the shield, then make the window count.",
         rival("THE CHAMPION", "ace", 130, dict(HEAVY, legs="rj-hare", booster="bt-surge"), "lr-glint", "sg-brand", "gc-anvil",
               pulse_ap=2200, pulse_down_ms=6000),
         plus("min_ap_pct", 50, "PLACEHOLDER"), "arena-4"),
  ]))

GIANT = dict(head="hd-mantle", core="cr-citadel", arms="ar-bastion", legs="lg-bastion", booster="bt-titan",
                               generator="gn-forge", right_weapon="ml-hornet", left_weapon="rf-marrow", shoulder_weapon="gc-anvil")
def giant(name, at, scale, ap, speed=60, damage=140, wave=0, turrets=()):
    return dict(name=name, pilot="giant", at=at, ap_pct=ap, scale_pct=scale, speed_pct=speed, damage_pct=damage,
                wave=wave, loadout=GIANT, turrets=[[round(v * 100) for v in t] for t in turrets])

# one giant per planet, in a mission that has room for it
def add_giant(pid, mid, g):
    for pl in planets:
        if pl["id"] == pid:
            for mi in pl["missions"]:
                if mi["id"] == mid:
                    mi.setdefault("mechs", []).append(g)
                    return
    raise SystemExit(mid)
add_giant("halden", "halden-3", giant("SIEGE WALKER", [yards[0], yards[1] - 170, 0], 240, 260, wave=2))
add_giant("sere", "sere-2", giant("DUNE STRIDER", [-450, -80, 0], 220, 220, wave=1))
add_giant("spindle", "spindle-2", giant("RING HULK", [0, -160, 0], 200, 220))
add_giant("rime", "rime-1", giant("FROST COLOSSUS", [0, 0, 180], 260, 220, wave=1))
add_giant("cinder", "cinder-3", giant("MAGMA TITAN", [foundry[0], foundry[1] - 170, 0], 280, 280, wave=2))

# ---------------- 6 TETHYS (hidden) ----------------
def deck_resupply(L, deck_y, fi=0):
    """Two resupply pads on an arms fort's deck, clear of its towers and guns."""
    top = deck_y + max(20, L // 90)
    W = L * 3 // 10
    return [dict(at=[W * 3 // 10, -L // 4, top], fortress=fi, r_m=14), dict(at=[-W * 3 // 10, L // 4, top], fortress=fi, r_m=14)]

def fortress(name, L, deck_y, legs_per_side, towers, weak_ap, core_ap, path, speed, size_x, turrets_per_side, batteries, engines=0):
    """An arms fort L metres long: legs with ledges to climb, a deck, towers,
    the core on the tallest tower, weak points on the leg joints (and engines),
    guns along the deck."""
    W = L * 3 // 10
    leg = max(40, L // 28)
    deck_t = max(20, L // 90)
    hull, weak, tur, bat = [], [], [], []
    xs = [-W // 2 + leg, W // 2 - leg]
    zs = [-L // 2 + L * (k + 1) // (legs_per_side + 1) for k in range(legs_per_side)]
    for x in xs:
        for z in zs:
            hull.append(dict(at=[x, deck_y // 2, z], size=[leg, deck_y, leg], paint="dark"))
            # ledges every 120 m, each a little wider than the leg
            for h in range(110, deck_y - 30, 120):
                hull.append(dict(at=[x, h, z], size=[leg * 2, 8, leg * 2], paint="secondary"))
            weak.append([x + (leg if x > 0 else -leg), deck_y // 2, z])
    hull.append(dict(at=[0, deck_y + deck_t // 2, 0], size=[W, deck_t, L], paint="primary"))
    for x in (-W // 2, W // 2):
        hull.append(dict(at=[x, deck_y + deck_t, 0], size=[6, 4, L], paint="glow"))
    top = deck_y + deck_t
    th = []
    for k, tz in enumerate(towers):
        h = min(L // 6, 320) if k == len(towers) // 2 else min(L // 10, 200)
        th.append(h)
        hull.append(dict(at=[0, top + h // 2, tz], size=[W // 4, h, L // 12], paint="secondary"))
        hull.append(dict(at=[0, top + h - 4, tz], size=[W // 4 + 4, 4, L // 12 + 4], paint="glow"))
        bat.append([W // 8 + 10, top + h + 4, tz])
        bat.append([-W // 8 - 10, top + h + 4, tz])
    mid = len(towers) // 2
    core = [0, top + th[mid] + max(30, L // 60), towers[mid]]
    for side in (-1, 1):
        for k in range(turrets_per_side):
            tur.append([side * (W // 2 - 12), top + 2, -L // 2 + L * (k + 1) // (turrets_per_side + 1)])
    for k in range(engines):
        ex = -W // 3 + (2 * W // 3) * k // max(1, engines - 1)
        hull.append(dict(at=[ex, top - deck_t // 2, L // 2 + L // 40], size=[W // 8, deck_t * 2, L // 20], paint="dark"))
        weak.append([ex, top, L // 2 + L // 20])
    for b in batteries:
        bat.append([b[0] * W // 2, top + 2, b[1] * L // 2])
    return dict(name=name, at=[0, 0], path=path, speed_ms=speed, size_x=size_x, hull=hull,
                weak_points=weak, weak_ap=weak_ap, weak_radius_m=max(25, min(90, L // 50)),
                core=core, core_ap=core_ap, core_radius_m=max(30, min(120, L // 40)),
                turrets=tur, batteries=bat)

GIANT_SHOULDERS = [(-2.0, 4.2, 0), (2.0, 4.2, 0), (0, 7.4, -0.6)]
planets.append(dict(
  id="tethys", name="TETHYS", opens_at=11, opens_at_plus=8, hidden=True,
  blurb="A dead sea of black glass that no chart shows. The things that walk here were built for wars between worlds: frames the size of towers, and fortresses the size of cities.",
  gimmick="Arms forts: past the first giants, each target is a walking fortress. Land on it, break its weak points, and the core opens.",
  star=dict(color=[60, 30, 120], band=[190, 120, 255], size=124, rings=True, moons=3, glow=1),
  look=dict(look((6, 4, 14), (22, 14, 44), (26, 22, 44), (150, 90, 255),
            [(40, 36, 60), (30, 28, 48), (54, 46, 76), (24, 22, 38)],
            (70, 50, 110), (190, 120, 255), True, (20, 100, 40), 1500, 11000, glow=20, stars=True, structure=(190, 120, 255)),
            far_m=14000, near_cm=50),
  climate=dict(gravity_pct=90, traction_pct=100, floor_dps=0),
  map=dict(seed=9090, width_m=16000, length_m=16000, cell_m=500, street_m=60, empty_pct=88, min_h_m=10, max_h_m=60,
           tall_pct=6, tall_h_m=120, ceiling_m=2400, style="mesas",
           clearings=[dict(x_m=0, z_m=0, r_m=5200)]),
  missions=[
    m("tethys-1", "FIRST GIANT", "A frame ten times your height walks the glass, armed like any frame. Find out what it can take.",
      "duel", [0, 900, 0], "hd-argus", plus("min_ap_pct", 60, "cr-aegis"), time_limit_s=420, power_pct=120,
      # 550%: about seven tenths of the starting frame's ammunition, with one resupply behind you
      mechs=[giant("TITAN-CLASS", [0, 0, 180], 1000, 550, speed=45, damage=220)],
      resupply=[dict(at=[0, 1100, 0], r_m=12)]),
    m("tethys-2", "TWIN COLOSSI", "Two frames thirty times your height, each with guns riding its shoulders. They walk together.",
      "duel", [0, 1500, 0], "ar-talon", plus("under_seconds", 300, "lt-crawler"), requires=["tethys-1"], time_limit_s=540,
      # 240% AP each and three guns apiece: the pair and their guns take about two thirds of the
      # starting frame's ammunition, where at 1500% they took four times all of it
      mechs=[giant("COLOSSUS ALPHA", [-320, 0, 180], 3000, 240, speed=30, damage=170, turrets=GIANT_SHOULDERS),
             giant("COLOSSUS BETA", [320, -500, 180], 3000, 240, speed=30, damage=170, turrets=GIANT_SHOULDERS)],
      resupply=[dict(at=[0, 1700, 0], r_m=12)]),
    m("tethys-3", "WALKING BASTION", "An arms fort seven hundred metres long, on four legs. Climb its legs, break the joints, and the core on its tower opens.",
      "duel", [0, 1700, 0], "bt-nova", plus("min_ap_pct", 50, "pr-corona"), requires=["tethys-2"], time_limit_s=720, power_pct=130,
      fortresses=[fortress("BASTION · ARMS FORT", 700, 240, 2, [0], 6000, 30000, [[0, 0], [0, -1500]], 10, 100, 4, [[-1, -1], [1, 1]])],
      resupply=deck_resupply(700, 240) + [dict(at=[0, 1900, 0], r_m=12)]),
    m("tethys-4", "LEVIATHAN", "Two kilometres of fortress on six legs, with engines astern and guns end to end.",
      "duel", [0, 2900, 0], "gn-singularity", plus("under_seconds", 600, "pc-nova"), requires=["tethys-3"], time_limit_s=960, power_pct=135,
      fortresses=[fortress("LEVIATHAN · ARMS FORT", 2100, 520, 3, [-600, 0, 600], 9000, 50000, [[0, 0], [1500, -800]], 7, 300, 8,
                           [[-1, -1], [1, -1], [-1, 1], [1, 1]], engines=2)],
      resupply=deck_resupply(2100, 520) + [dict(at=[0, 3100, 0], r_m=12)]),
    m("tethys-5", "THE ARK", "Seven kilometres long and taller than any mountain on Halden, with escorts at its feet. Whoever built it is not coming back for it.",
      "duel", [0, 5000, 0], "pb-ramspike", plus("min_ap_pct", 40, "cr-ark"), requires=["tethys-4"], time_limit_s=1500, power_pct=140,
      fortresses=[fortress("THE ARK · ARMS FORT", 7000, 900, 4, [-2200, 0, 2200], 14000, 80000, [[0, 0], [0, -2500]], 4, 1000, 12,
                           [[-1, -1], [1, -1], [-1, 0], [1, 0], [-1, 1], [1, 1]], engines=3)],
      resupply=deck_resupply(7000, 900) + [dict(at=[0, 5200, 0], r_m=12)],
      mechs=[dict(name="ESCORT ONE", pilot="twin", at=[-150, 3800, 180], ap_pct=90,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-comet",
                               generator="gn-ember", right_weapon="sg-brand", left_weapon="rf-marrow", shoulder_weapon="mp-swarm")),
             dict(name="ESCORT TWO", pilot="twin", at=[150, 3800, 180], ap_pct=90,
                  loadout=dict(head="hd-kestrel", core="cr-vane", arms="ar-reed", legs="lg-wisp", booster="bt-comet",
                               generator="gn-ember", right_weapon="ml-hornet", left_weapon="rf-marrow", shoulder_weapon="mp-swarm"))]),
  ]))

# the arena opens after three clears, so it sits between Sere (two) and Spindle (five)
planets.insert(2, planets.pop(next(i for i, p in enumerate(planets) if p["id"] == "arena")))
# reward bookkeeping: every non-starter part exactly once
starters = {"hd-warden", "cr-bulwark", "ar-lancer", "lg-stride", "bt-surge", "gn-kiln", "ml-hornet", "rf-marrow", "gc-anvil", "bl-emberline"}
# fix rewards to cover all 40 parts
assign = {
 "halden-1": ("hd-kestrel", "cr-vane"), "halden-2": ("ar-reed", "lg-wisp"), "halden-3": ("bt-comet", "gn-ember"), "halden-4": ("sg-brand", "mp-swarm"),
 "sere-1": ("lg-bastion", "ar-bastion"), "sere-2": ("hd-mantle", "cr-citadel"), "sere-3": ("bt-titan", "rj-hare"), "sere-4": ("gn-forge", "lr-glint"),
 "spindle-1": ("hd-osprey", "cr-spire"), "spindle-2": ("ar-heron", "lg-gale"), "spindle-3": ("bt-flare", "rf-gatling"), "spindle-4": ("bz-maul", "gn-corona"),
 "rime-1": ("hd-bastille", "cr-anvil"), "rime-2": ("ar-grip", "rj-lynx"), "rime-3": ("bt-ram", "gn-reactor"), "rime-4": ("sg-thunder", "mp-hydra"),
 "halden-5": ("lc-lance", "cr-rampart"), "halden-6": ("lr-lancet", "lg-stalker"),
 "cinder-1": ("hd-sentinel", "cr-lattice"), "cinder-2": ("ar-titan", "lg-monolith"), "cinder-3": ("bt-wisp", "lr-spear"), "cinder-4": ("gc-quake", "lc-halo"),
 "tethys-1": ("hd-argus", "cr-aegis"), "tethys-2": ("ar-talon", "lt-crawler"), "tethys-3": ("bt-nova", "pr-corona"),
 "tethys-4": ("gn-singularity", "pc-nova"), "tethys-5": ("pb-ramspike", "cr-ark"),
 "arena-1": ("bt-sprint", "gn-ludus"), "arena-2": ("rf-murmillo", "cr-gladius"), "arena-3": ("sg-thraex", "lg-retiarius"),
 "arena-4": ("ml-secutor", "ar-cestus"), "arena-5": ("gc-coliseum", "hd-crown"),
}
for p in planets:
    for mi in p["missions"]:
        if "course" in mi:
            mi["course"] = [c if isinstance(c, dict) else gate(*c, r=20) for c in mi["course"]]
        a, b = assign[mi["id"]]
        mi["reward"] = a
        mi["plus"]["reward"] = b

print(json.dumps({"_note": "The campaign. Planets open when the number of missions cleared reaches opens_at. A mission opens with its planet once every mission it requires is cleared. Each mission gives one part on its first clear and another for meeting its plus challenge, which is shown only after the first clear. Coordinates in metres: start and mech at are [x, z, yaw_deg]; unit at is [x, z, altitude], and an altitude below zero means on top of whatever stands there.", "planets": planets}, indent=1, ensure_ascii=False))
