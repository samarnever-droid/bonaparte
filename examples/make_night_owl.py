#!/usr/bin/env python3
"""Generate night-owl.bonaparte.json — the Bonaparte Studio owl ident.

A flat night-scene diorama: moon, twinkling stars, a cloud, a branch, and
a fully vector owl (every part is LayerKind::Shape { points }) that breathes,
bobs (parenting moves the subtree), flaps its wings, darts its eyes, blinks,
and squeaks a little HOOT ring at the end. All motion is keyframes on the
documented ops surface — nothing renderer-side.
"""
import json, math

TICKS = 120_000  # per second
DUR = 960_000    # 8 s
P = 0            # comp-center origin; y is down

def srgb_to_linear(c: float) -> float:
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

def col(r, g, b, a=1.0):
    return [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a]

def ellipse_points(rx, ry, n=56):
    return [[round(rx * math.cos(2 * math.pi * i / n), 2),
             round(ry * math.sin(2 * math.pi * i / n), 2)] for i in range(n)]

def wing_points(w, h, n=44):
    """Teardrop: broad rounded top, tapering to a pointed tip."""
    pts = []
    for i in range(n):
        t = 2 * math.pi * i / n
        y = (h / 2) * -math.cos(t)            # -h/2 .. +h/2
        narrow = 1.0 - 0.62 * max(0.0, (y + h / 2) / h) ** 1.4
        x = (w / 2) * math.sin(t) * narrow
        pts.append([round(x, 2), round(y, 2)])
    return pts

def star_points(r_out, r_in, n=5):
    pts = []
    for i in range(n * 2):
        r = r_out if i % 2 == 0 else r_in
        a = math.pi * i / n - math.pi / 2
        pts.append([round(r * math.cos(a), 2), round(r * math.sin(a), 2)])
    return pts

def tri(w, h, skew=0.0):
    return [[-w / 2 + skew, h / 2], [w / 2, h / 2], [0, -h / 2]]

def rect(w, h):
    return [[-w / 2, -h / 2], [w / 2, -h / 2], [w / 2, h / 2], [-w / 2, h / 2]]

def shape(name, color, size, points, pos, parent=None, z=None, rot=0.0, stroke=None):
    style = {"size": size, "corner_radius": 0.0,
             "stroke_width": stroke[0] if stroke else 0.0,
             "stroke_color": stroke[1] if stroke else col(0, 0, 0, 0)}
    # points wire format: [ subpath = [ [x, y], ... ], ... ] — wrap singles.
    subpaths = [points]
    return {
        "id": 0, "name": name,
        "kind": {"Shape": {"color": color, "generator": None, "style": style, "points": subpaths}},
        "start": 0, "duration": DUR,
        "transform": {"position": pos, "scale": [100, 100], "rotation": rot,
                      "opacity": 1, "anchor_point": [0, 0], "z": z or 0},
        "tracks": {}, "parent": parent, "blend_mode": "Normal",
        "visible": True, "locked": False, "effects": [],
    }

def keys(*pairs, easing="Linear"):
    return {"keys": [{"time": t, "value": v, "easing": easing} for t, v in pairs]}

BEZ = lambda: {"Bezier": {"p1": [0.42, 0.0], "p2": [0.58, 1.0]}}
SOFT = {"Bezier": {"p1": [0.16, 1], "p2": [0.3, 1]}}

layers = {}

# ---- backdrop -----------------------------------------------------------
layers["1"] = shape("Moon", col(250, 244, 222), [220, 220],
                    ellipse_points(110, 110), [300, -158])
layers["1"]["tracks"]["Scale"] = keys(
    (0, {"Vec2": [100, 100]}), (480_000, {"Vec2": [102.5, 102.5]}),
    (960_000, {"Vec2": [100, 100]}), easing=SOFT)

star_spots = [(-360, -190, 8, 0), (-180, -110, 6, 320_000), (-60, -210, 7, 640_000),
              (60, -60, 5, 160_000), (200, -230, 9, 480_000), (-300, 40, 5, 800_000)]
for i, (x, y, r, phase) in enumerate(star_spots):
    lid = str(2 + i)
    pts = star_points(r, r * 0.45)
    layers[lid] = shape(f"Star {i + 1}", col(235, 228, 200), [r * 2, r * 2], pts, [x, y])
    a, b = 0.25, 1.0
    keys_list = []
    t = -phase % 400_000
    times = []
    tt = t
    while tt < DUR:
        times.append(tt)
        tt += 200_000
    for k, tm in enumerate(times):
        keys_list.append((tm, {"Scalar": a if k % 2 else b}))
    if keys_list and keys_list[-1][0] != DUR:
        keys_list.append((DUR, {"Scalar": keys_list[0][1]["Scalar"]}))
    layers[lid]["tracks"]["Opacity"] = keys(*keys_list)

layers["8"] = shape("Cloud", col(30, 38, 62), [520, 74],
                    ellipse_points(260, 37), [-250, 190])

# ---- branch + feet ------------------------------------------------------
layers["9"] = shape("Branch", col(96, 66, 40), [640, 36], rect(640, 36),
                    [10, 158], rot=-2.0)
layers["9"]["tracks"]["Rotation"] = keys((0, {"Scalar": -2.0}), (DUR, {"Scalar": -2.0}))
for i, (x, y, rot) in enumerate([(-72, 150, 6), (18, 150, -4)]):
    lid = str(10 + i)
    layers[lid] = shape(f"Feet {'L' if i == 0 else 'R'}", col(224, 148, 66),
                        [44, 18], rect(44, 18), [x, y], rot=rot)

# ---- the owl (every part parents to the Body so it bobs as one) ---------
BODY = 12
layers["12"] = shape("Body", col(140, 96, 62), [300, 340],
                     ellipse_points(150, 170), [-30, -12])
layers["12"]["tracks"]["Scale"] = keys(
    (0, {"Vec2": [100, 100]}), (320_000, {"Vec2": [101.5, 102.5]}),
    (640_000, {"Vec2": [100, 100]}), (960_000, {"Vec2": [101.5, 102.5]}), easing=BEZ())
layers["12"]["tracks"]["Rotation"] = keys(
    (0, {"Scalar": -1.4}), (480_000, {"Scalar": 1.4}), (960_000, {"Scalar": -1.4}),
    easing=BEZ())

layers["13"] = shape("Belly", col(222, 196, 152), [196, 224],
                     ellipse_points(98, 112), [-30, 52], parent=BODY)

# wings: broad top pivots at the shoulder (anchor at top edge), tip swings
for i, (name, x, mirror) in enumerate([("Wing left", -168, 1), ("Wing right", 106, -1)]):
    lid = str(14 + i)
    pts = [[round(px * mirror, 2), py] for px, py in wing_points(118, 236)]
    layers[lid] = shape(name, col(112, 74, 46), [118, 236], pts, [x, -26],
                        parent=BODY)
    layers[lid]["transform"]["anchor_point"] = [0, -108]
    flap = []
    t = 0
    while t <= DUR:
        flap.append((t, {"Scalar": -6}))
        flap.append((t + 45_000, {"Scalar": -30}))
        t += 90_000
    layers[lid]["tracks"]["Rotation"] = keys(*flap)

# eyes + pupils (dart with Hold, blink by squashing Y)
layers["16"] = shape("Eye white left", col(246, 240, 224), [124, 124],
                     ellipse_points(62, 62), [-92, -84], parent=BODY)
layers["17"] = shape("Eye white right", col(246, 240, 224), [124, 124],
                     ellipse_points(62, 62), [34, -84], parent=BODY)

# Dart offsets around each pupil's HOME (tracks override statics, so every
# key = home + offset).
darts = [(0, [0, 0]), (60_000, [-13, -2]), (210_000, [9, 3]), (330_000, [0, 0]),
         (400_000, [12, -7]), (520_000, [0, 0]), (600_000, [-12, 5]),
         (720_000, [0, 0]), (960_000, [0, 0])]
for i, (name, hx, hy) in enumerate([("Pupil left", -78, -76), ("Pupil right", 48, -76)]):
    lid = str(18 + i)
    layers[lid] = shape(name, col(30, 22, 18), [52, 52],
                        ellipse_points(26, 26), [hx, hy], parent=BODY)
    layers[lid]["tracks"]["Position"] = keys(
        *[(t, {"Vec2": [hx + v[0], hy + v[1]]}) for t, v in darts], easing="Hold")
    blink = [(288_000, {"Vec2": [100, 100]}), (292_000, {"Vec2": [100, 6]}),
             (296_000, {"Vec2": [100, 100]}), (612_000, {"Vec2": [100, 100]}),
             (616_000, {"Vec2": [100, 6]}), (620_000, {"Vec2": [100, 100]})]
    layers[lid]["tracks"]["Scale"] = keys(*blink, easing="Linear")

layers["20"] = shape("Beak", col(236, 158, 60), [36, 30],
                     [[-18, -14], [18, -14], [0, 16]], [-30, -30], parent=BODY)

for i, (name, x, rot) in enumerate([("Ear tuft left", -118, -16), ("Ear tuft right", 62, 16)]):
    lid = str(21 + i)
    layers[lid] = shape(name, col(112, 74, 46), [54, 66],
                        tri(54, 66), [x, -196], parent=BODY, rot=rot)

# ---- playful HOOT ring popping from the beak at 6.6 s -------------------
hoot = shape("HOOT ring", col(235, 228, 200), [90, 90], ellipse_points(45, 45),
             [34, -120], parent=BODY, stroke=(5.0, col(235, 228, 200)))
hoot["tracks"]["Opacity"] = keys((0, {"Scalar": 0}), (768_000, {"Scalar": 0}),
                                 (792_000, {"Scalar": 1}), (900_000, {"Scalar": 0}),
                                 (960_000, {"Scalar": 0}))
hoot["tracks"]["Scale"] = keys((0, {"Vec2": [20, 20]}), (768_000, {"Vec2": [20, 20]}),
                               (900_000, {"Vec2": [130, 130]}), (960_000, {"Vec2": [130, 130]}),
                               easing=SOFT)
hoot["transform"]["anchor_point"] = [0, 0]
layers["23"] = hoot

order = [int(k) for k in layers]
for lid, layer in layers.items():
    layer["id"] = int(lid)

project = {
    "name": "Night Owl — Studio ident",
    "comps": {
        "1": {
            "id": 1, "name": "Night Owl", "width": 960, "height": 540,
            "fps": {"num": 30, "den": 1}, "duration": DUR,
            "background": col(10, 14, 30),
            "layer_order": order,
            "layers": layers,
        }
    },
    "media": {}, "next_comp": 2, "next_layer": max(order) + 1, "next_media": 1,
}

with open("/home/user/bonaparte/examples/night-owl.bonaparte.json", "w") as f:
    json.dump(project, f, indent=2)
with open("/home/user/night-owl.bonaparte.json", "w") as f:
    json.dump(project, f, indent=2)
print("owl written:", len(order), "layers")
