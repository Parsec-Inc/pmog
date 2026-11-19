"""Placeholder avatar layers: 25x28 RGBA PNGs, symmetric around column 12, forehead free at rows 8-12."""
import sys
from pathlib import Path

from PIL import Image

OUT = Path(sys.argv[1])
OUT.mkdir(parents=True, exist_ok=True)
W, H = 25, 28
CX = 12  # center column
EYE_L, EYE_R, EYE_Y = 9, 15, 15
MOUTH_Y = 22

SKIN = (232, 190, 150)
SKIN_SHADE = (205, 158, 122)
SKIN_LIGHT = (245, 210, 175)
OUTLINE = (110, 70, 50)
EYE_WHITE = (245, 245, 245)
PUPIL = (40, 30, 30)
BROW = (80, 55, 40)
MOUTH = (150, 60, 60)
TEETH = (250, 250, 250)
GOLD = (255, 205, 60)


def new():
    return Image.new("RGBA", (W, H), (0, 0, 0, 0))


def put(img, x, y, c):
    if 0 <= x < W and 0 <= y < H:
        img.putpixel((x, y), (*c, 255))


def mirror(img, x, y, c):
    """Pixel at offset x from the left eye, mirrored on the right eye."""
    put(img, EYE_L + x, y, c)
    put(img, EYE_R - x, y, c)


def save(img, name):
    img.save(OUT / f"{name}.png")


def ellipse(cx, cy, rx, ry):
    return lambda x, y: ((x + 0.5 - cx) / rx) ** 2 + ((y + 0.5 - cy) / ry) ** 2 <= 1


def face(mask, name, skin=SKIN, shade=SKIN_SHADE, light=SKIN_LIGHT, outline=OUTLINE, extra=None):
    img = new()
    cells = {(x, y) for y in range(H) for x in range(W) if mask(x, y)}
    for y in range(15, 19):  # ears
        cells.add((3, y))
        cells.add((21, y))
    for (x, y) in cells:
        edge = any((x + dx, y + dy) not in cells for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
        if edge:
            c = outline
        elif x > CX + 5 or y >= 24:
            c = shade
        elif x < CX - 4 and y < 13:
            c = light
        else:
            c = skin
        put(img, x, y, c)
    for y in (18, 19):  # nose
        put(img, CX + 1, y, shade)
    put(img, CX, 20, shade)
    put(img, CX + 1, 20, shade)
    if extra:
        extra(img)
    save(img, name)


def round_face(x, y):
    return ellipse(12.5, 14.5, 8.6, 12.2)(x, y)


def square_face(x, y):
    return 4 <= x <= 20 and 3 <= y <= 26 and not (x in (4, 20) and y in (3, 26))


def oval_face(x, y):
    return ellipse(12.5, 14.5, 7.6, 12.6)(x, y)


def chad_face(x, y):
    if y < 3 or y > 26 or x < 4 or x > 20:
        return False
    if y <= 19:
        return not (y == 3 and x in (4, 20))
    cut = (y - 19 + 1) // 2  # angular jaw
    return 4 + cut <= x <= 20 - cut


def marble_extra(img):
    for x, y in ((7, 5), (8, 6), (8, 7), (17, 23), (16, 24)):
        put(img, x, y, (170, 170, 185))


face(round_face, "face_round")
face(square_face, "face_square")
face(oval_face, "face_oval")
face(chad_face, "face_chad")
face(chad_face, "face_marble", (225, 225, 232), (195, 195, 208), (245, 245, 250), (120, 120, 135), marble_extra)


def paint(name, pixels):
    img = new()
    for x, y, c in pixels:
        put(img, x, y, c)
    save(img, name)


def hair(name, color, dark, cells):
    paint(name, [(x, y, dark if (x + y) % 5 == 0 else color) for x, y in cells])


cap_dome = [(x, y) for y in range(1, 8) for x in range(4, 21) if ellipse(12.5, 9, 8.6, 7.5)(x, y)]
hair("hair_fade", (70, 50, 35), (50, 35, 25), cap_dome + [(x, y) for y in range(8, 13) for x in (4, 5, 19, 20)])
hair("hair_mohawk", (230, 60, 110), (180, 40, 80), [(x, y) for y in range(0, 7) for x in range(CX - 1, CX + 2)])
hair("hair_fringe", (40, 40, 55), (25, 25, 35), cap_dome + [(x, y) for y in range(8, 13) for x in range(4, 9 - (y - 8))] + [(19, y) for y in range(8, 12)])
afro = [(x, y) for y in range(0, 17) for x in range(W) if ellipse(12.5, 8, 12.5, 9)(x, y) and not (6 <= x <= 18 and y >= 8)]
hair("hair_afro", (60, 40, 30), (40, 25, 20), afro)


def eyes(name, pts):
    img = new()
    for x, y, c in pts:
        mirror(img, x, y, c)
    save(img, name)


BROWS = [(-1, EYE_Y - 2, BROW), (0, EYE_Y - 2, BROW), (1, EYE_Y - 2, BROW)]
eyes("eyes_plain", BROWS + [(-1, EYE_Y, EYE_WHITE), (0, EYE_Y, PUPIL), (1, EYE_Y, EYE_WHITE)])
eyes("eyes_hunter", [(-1, EYE_Y - 2, BROW), (0, EYE_Y - 2, BROW), (1, EYE_Y - 1, BROW), (-1, EYE_Y, EYE_WHITE), (0, EYE_Y, PUPIL), (1, EYE_Y, PUPIL)])
eyes("eyes_big", [(-1, EYE_Y - 3, BROW), (0, EYE_Y - 3, BROW), (1, EYE_Y - 3, BROW)] + [(dx, EYE_Y + dy, EYE_WHITE) for dx in (-1, 0, 1) for dy in (-1, 0, 1)] + [(0, EYE_Y, PUPIL), (0, EYE_Y + 1, PUPIL)])
eyes("eyes_star", [(0, EYE_Y - 1, GOLD), (-1, EYE_Y, GOLD), (0, EYE_Y, (255, 250, 200)), (1, EYE_Y, GOLD), (0, EYE_Y + 1, GOLD)] + BROWS)
eyes("eyes_sigma", BROWS + [(-1, EYE_Y, (60, 120, 200)), (0, EYE_Y, (230, 250, 255)), (1, EYE_Y, (120, 200, 255))])
eyes("mood_blink_eyes", BROWS + [(-1, EYE_Y, PUPIL), (0, EYE_Y, PUPIL), (1, EYE_Y, PUPIL)])
eyes("mood_smug_eyes", [(-1, EYE_Y - 1, BROW), (0, EYE_Y - 1, BROW), (1, EYE_Y - 1, BROW), (-1, EYE_Y, PUPIL), (0, EYE_Y, PUPIL), (1, EYE_Y, EYE_WHITE)])
eyes("mood_mogged_eyes", [(-1, EYE_Y - 1, PUPIL), (1, EYE_Y - 1, PUPIL), (0, EYE_Y, PUPIL), (-1, EYE_Y + 1, PUPIL), (1, EYE_Y + 1, PUPIL)])
eyes("mood_sad_eyes", [(-1, EYE_Y - 1, BROW), (0, EYE_Y - 2, BROW), (1, EYE_Y - 2, BROW), (-1, EYE_Y, PUPIL), (0, EYE_Y, PUPIL), (1, EYE_Y, PUPIL)] + [(0, EYE_Y + k, (110, 180, 255)) for k in (1, 2, 3)])
eyes("mood_proud_eyes", [(-1, EYE_Y, PUPIL), (0, EYE_Y - 1, PUPIL), (1, EYE_Y, PUPIL), (-1, EYE_Y - 3, BROW), (0, EYE_Y - 3, BROW), (1, EYE_Y - 3, BROW)])


def line(y, c, half=2):
    return [(x, y, c) for x in range(CX - half, CX + half + 1)]


M = MOUTH_Y
paint("mouth_plain", line(M, MOUTH, 1))
paint("mouth_smile", [(CX - 3, M - 1, MOUTH), (CX + 3, M - 1, MOUTH)] + line(M, MOUTH) + line(M, TEETH, 1))
paint("mouth_mewing", line(M, OUTLINE, 1) + [(x, 25, SKIN_SHADE) for x in range(7, 18)])
paint("mouth_smirk", line(M, MOUTH, 1) + [(CX + 2, M - 1, MOUTH), (CX + 3, M - 2, MOUTH)])
paint("mouth_gold", [(CX - 3, M - 1, MOUTH), (CX + 3, M - 1, MOUTH)] + line(M, MOUTH) + [(CX - 1, M, TEETH), (CX, M, GOLD), (CX + 1, M, TEETH)])
paint("mood_smug_mouth", line(M, MOUTH, 1) + [(CX + 2, M - 1, MOUTH), (CX + 3, M - 2, MOUTH)])
paint("mood_mogged_mouth", line(M, MOUTH, 1) + [(CX - 2, M + 1, MOUTH), (CX + 2, M + 1, MOUTH)] + [(20, y, (120, 190, 255)) for y in (11, 12, 13)])
paint("mood_sad_mouth", [(CX, M - 1, MOUTH), (CX - 1, M, MOUTH), (CX + 1, M, MOUTH), (CX - 2, M + 1, MOUTH), (CX + 2, M + 1, MOUTH)])
paint("mood_proud_mouth", [(CX - 3, M - 1, MOUTH), (CX + 3, M - 1, MOUTH)] + line(M, TEETH) + line(M + 1, MOUTH))

shine = (140, 140, 170)
paint("acc_earrings", [(3, 19, GOLD), (21, 19, GOLD), (3, 20, (200, 150, 30)), (21, 20, (200, 150, 30))])
paint("acc_scar", [(EYE_R + 1, EYE_Y - 3, (190, 70, 70)), (EYE_R, EYE_Y - 2, (190, 70, 70)), (EYE_R, EYE_Y - 1, (190, 70, 70)), (EYE_R - 1, EYE_Y + 1, (190, 70, 70)), (EYE_R - 1, EYE_Y + 2, (190, 70, 70))])
paint("acc_cap", [(x, y, (50, 110, 220) if (x + y) % 6 else (40, 90, 190)) for x, y in cap_dome if y <= 6] + [(x, 7, (30, 70, 160)) for x in range(4, 25)])
lens = (25, 25, 35)
paint("acc_shades", [(x, y, lens) for y in range(EYE_Y - 1, EYE_Y + 2) for x in list(range(EYE_L - 2, EYE_L + 3)) + list(range(EYE_R - 2, EYE_R + 3))] + [(CX, EYE_Y - 1, lens), (EYE_L - 1, EYE_Y - 1, shine), (EYE_R - 1, EYE_Y - 1, shine)])
crown = [(x, y, GOLD) for y in range(0, 4) for x in range(6, 19) if y >= 2 or x in (6, 9, 12, 15, 18)]
paint("acc_crown", crown + [(CX, 2, (220, 40, 60)), (9, 2, (60, 160, 255)), (15, 2, (60, 200, 90))])
print("ok", len(list(OUT.glob("*.png"))))


# DLC Cadeia: fruit faces from "O sabonete".
def blush(x0, y0, color):
    def draw(img):
        for dx in range(3):
            for dy in range(2):
                put(img, x0 + dx, y0 + dy, color)
    return draw


def combine(*fns):
    def draw(img):
        for fn in fns:
            fn(img)
    return draw


def stem(color, leaf):
    def draw(img):
        for y in (0, 1, 2):
            put(img, CX, y, color)
        for x, y in ((CX + 1, 1), (CX + 2, 1), (CX + 2, 0), (CX + 3, 0)):
            put(img, x, y, leaf)
    return draw


# Zero (Lucas): peach skin, one red cheek.
face(round_face, "fruit_zero", (240, 150, 80), (215, 120, 60), (250, 175, 110), (130, 70, 35), blush(5, 18, (225, 70, 60)))
# Cenoura: clown paint over carrot orange.
def clown(img):
    for y in range(4, 26):
        for x in range(5, 20):
            if img.getpixel((x, y))[3] and img.getpixel((x, y))[:3] != (120, 60, 20):
                put(img, x, y, (240, 240, 240) if (x, y) != (CX, 19) else (220, 40, 50))
    for x, y in ((CX, 19), (CX + 1, 19), (CX, 20), (CX + 1, 20)):
        put(img, x, y, (220, 40, 50))
    for x in range(CX - 4, CX + 5):  # red smile paint
        put(img, x, MOUTH_Y + 1, (200, 30, 40))
face(round_face, "fruit_cenoura", (245, 140, 50), (215, 110, 35), (250, 170, 90), (120, 60, 20), clown)
# Pera: narrow top, wide bottom, yellow-green.
pear = lambda x, y: 3 <= y <= 26 and abs(x + 0.5 - 12.5) <= (4.5 + 4 * max(0, y - 9) / 17 if y >= 9 else 4.5)
face(pear, "fruit_pera", (215, 200, 90), (185, 170, 70), (235, 225, 130), (110, 95, 35), stem((90, 60, 30), (90, 170, 60)))
# Irmão Six Seven: green skin with tattoo lines.
def tattoo(img):
    for x, y in ((6, 20), (7, 21), (6, 22), (18, 20), (17, 21), (18, 22), (5, 9), (6, 10)):
        put(img, x, y, (40, 80, 40))
face(square_face, "fruit_sixseven", (110, 160, 70), (85, 130, 55), (140, 185, 95), (40, 70, 30), tattoo)
# Mamão: orange-yellow papaya.
face(oval_face, "fruit_mamao", (245, 185, 70), (220, 155, 50), (250, 210, 120), (130, 85, 25), blush(6, 18, (240, 130, 60)))
# 99: big dark plum with a stem and leaf.
face(square_face, "fruit_99", (95, 45, 40), (70, 30, 28), (125, 65, 55), (40, 18, 15), stem((70, 45, 25), (80, 170, 60)))

zero_hair = [(x, y) for y in range(1, 8) for x in range(5, 20) if ellipse(12.5, 8, 8, 6.5)(x, y)]
paint("hair_zero", [(x, y, (35, 30, 30) if (x * 3 + y) % 4 else (15, 12, 12)) for x, y in zero_hair])
dreads = [(x, y) for y in range(0, 8) for x in range(3, 22) if ellipse(12.5, 7, 10, 7)(x, y)]
dreads += [(x, y) for x in (3, 4, 20, 21) for y in range(8, 22) if (x + y) % 3]
paint("hair_dreads", [(x, y, (60, 170, 50) if (x + y) % 3 else (40, 120, 35)) for x, y in dreads])
print("fruits ok")
