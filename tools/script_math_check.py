"""script_math_check.py -- JFScript's math (script::math, num::sqrt, num::hypot) against
correctly rounded references from Python's decimal module, with random and special inputs.

    python tools/script_math_check.py [--target win|linux|both] [-n CASES] [--seed S] [--out DIR] [--java]

Writes the cases, lets scripttest --math answer them and reports per function the largest error
in ulps: fdlibm's exp, log, log2, log10, pow, sin, cos, tan, asin, acos, atan and atan2 and the
degree functions sin_deg, cos_deg and tan_deg promise less than 1 ulp, sqrt, hypot and round
must be exact, NaN must be the one NaN (0x7ff8000000000000) and zeros must carry the right
sign.  --target both runs the Windows and the Linux build (through WSL) and also checks that
their answers are bit for bit the same.  --java also answers the cases with Java's StrictMath
(fdlibm; java 21 on the PATH runs a source file) and counts per function how many answers are
bit for bit the same (for the information: pow, atan2 and hypot differ by design, see
docs/LANGUAGE.md).  The scripttest programs come from Cargo's folder (cargo build -p
scripttest; for Linux on Windows: python tools/linux_test.py builds it).  Exit code 1 when a
case is beyond its bound."""
import argparse
import decimal
import math
import os
import random
import struct
import subprocess
import sys
from decimal import Decimal as D
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NAN_BITS = 0x7FF8000000000000
BOUND = {"exp": 1, "log": 1, "log2": 1, "log10": 1, "pow": 1, "sin": 1, "cos": 1, "sqrt": 0, "round": 0, "tan": 1, "asin": 1,
         "acos": 1, "atan": 1, "atan2": 1, "hypot": 0, "sin_deg": 1, "cos_deg": 1, "tan_deg": 1}


def f2b(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def b2f(b):
    return struct.unpack("<d", struct.pack("<Q", b))[0]


def machin_pi(digits):
    """pi to `digits` decimal digits with integer arithmetic"""
    one = 10 ** (digits + 20)

    def atan_inv(n):
        total, term, k, sign = 0, one // n, 1, 1
        while term:
            total += sign * (term // k)
            term //= n * n
            k += 2
            sign = -sign
        return total

    return D(16 * atan_inv(5) - 4 * atan_inv(239)) / D(10) ** (digits + 20)


CTX = decimal.Context(prec=80, Emax=999999999, Emin=-999999999)
WIDE = decimal.Context(prec=1200, Emax=999999999, Emin=-999999999)
with decimal.localcontext(WIDE):
    PI = machin_pi(1250)
    HALF_PI = PI / 2


def series(r, first):
    """sin (first 1) or cos (first 0) of a small decimal r by its Taylor series"""
    term = r if first else D(1)
    total = term
    n = first
    r2 = r * r
    while True:
        term = -term * r2 / ((n + 1) * (n + 2))
        n += 2
        if term == 0 or abs(term) < D(10) ** -90:
            return total
        total += term


def trig(xd, name):
    """sin, cos or tan of a decimal x (radians): reduced by pi/2 in WIDE, the series in CTX"""
    with decimal.localcontext(WIDE):
        k = (xd / HALF_PI).to_integral_value(rounding=decimal.ROUND_HALF_EVEN)
        r = xd - k * HALF_PI
    with decimal.localcontext(CTX):
        r = +r
        s, c = series(r, 1), series(r, 0)
        q = int(k) % 4
        if name == "sin":
            return [s, c, -s, -c][q]
        if name == "cos":
            return [c, -s, -c, s][q]
        return s / c if q % 2 == 0 else -c / s


def atan_d(x):
    """atan of a decimal, in CTX: 1/x above 1, then halving the angle, then the series"""
    with decimal.localcontext(CTX):
        if x < 0:
            return -atan_d(-x)
        if x > 1:
            return +HALF_PI - atan_d(1 / x)
        halvings = 0
        while x > D("0.01"):
            x = x / (1 + (1 + x * x).sqrt())
            halvings += 1
        total, term, k, x2 = x, x, 1, x * x
        while True:
            term = -term * x2
            k += 2
            add = term / k
            if add == 0 or abs(add) < D(10) ** -95:
                break
            total += add
        return total * 2 ** halvings


def ref_deg(name, x):
    """sin, cos or tan of x degrees, with the exact values and signs of sinPi, cosPi, tanPi"""
    if not math.isfinite(x):
        return math.nan
    name = name[:3]
    with decimal.localcontext(WIDE):
        xd = D(x)
        y = xd % 360
        neg = math.copysign(1, x) < 0
        if y % 180 == 0:
            if name == "sin":
                return -0.0 if neg else 0.0
            if name == "cos":
                return 1.0 if y == 0 else -1.0
            odd = int(xd / 180) % 2 == 1
            return -0.0 if neg != odd else 0.0
        if (y - 90) % 180 == 0:
            if name == "sin":
                return 1.0 if y in (90, -270) else -1.0
            if name == "cos":
                return 0.0
            return math.inf if y in (90, -270) else -math.inf
        rad = y * PI / 180
    return float(trig(rad, name))


def ref(name, x, y):
    if name in ("sin_deg", "cos_deg", "tan_deg"):
        return ref_deg(name, x)
    if name == "tan":
        if not math.isfinite(x):
            return math.nan
        if x == 0:
            return x
        return float(trig(D(x), "tan"))
    if name == "atan":
        if math.isnan(x):
            return math.nan
        if math.isinf(x):
            return math.copysign(math.pi / 2, x)
        if x == 0:
            return x
        return float(atan_d(D(x)))
    if name in ("asin", "acos"):
        if math.isnan(x) or abs(x) > 1:
            return math.nan
        with decimal.localcontext(CTX):
            xd = D(x)
            if name == "asin":
                if x == 0:
                    return x
                if abs(x) == 1:
                    return float(HALF_PI) * x
                return float(atan_d(xd / (1 - xd * xd).sqrt()))
            if x == -1:
                return float(+PI)
            return float(2 * atan_d(((1 - xd) / (1 + xd)).sqrt()))
    if name == "atan2":
        return ref_atan2(x, y)
    if name == "hypot":
        if math.isinf(x) or math.isinf(y):
            return math.inf
        if math.isnan(x) or math.isnan(y):
            return math.nan
        with decimal.localcontext(WIDE):
            return float((D(x) * D(x) + D(y) * D(y)).sqrt())
    if name == "sqrt":
        if math.isnan(x) or x < 0:
            return math.nan
        if x == 0 or math.isinf(x):
            return x
        with decimal.localcontext(CTX):
            return float(D(x).sqrt())
    if name == "round":
        if not math.isfinite(x):
            return x
        with decimal.localcontext(WIDE):
            r = float(D(abs(x)).quantize(D(1), rounding=decimal.ROUND_HALF_UP))
        return math.copysign(r, x)
    if name == "exp":
        if math.isnan(x):
            return math.nan
        if x > 710:
            return math.inf
        if x < -746:
            return 0.0
        with decimal.localcontext(CTX):
            return float(D(x).exp())
    if name in ("log", "log2", "log10"):
        if math.isnan(x) or x < 0:
            return math.nan
        if x == 0:
            return -math.inf
        if math.isinf(x):
            return x
        with decimal.localcontext(CTX):
            d = D(x)
            v = d.ln() if name == "log" else d.ln() / D(2).ln() if name == "log2" else d.log10()
            return float(v)
    if name in ("sin", "cos"):
        if not math.isfinite(x):
            return math.nan
        if x == 0:
            return x if name == "sin" else 1.0
        with decimal.localcontext(WIDE):
            xd = D(x)
            k = (xd / HALF_PI).to_integral_value(rounding=decimal.ROUND_HALF_EVEN)
            r = xd - k * HALF_PI
        with decimal.localcontext(CTX):
            r = +r
            s, c = series(r, 1), series(r, 0)
            q = int(k) % 4
            v = [s, c, -s, -c][q] if name == "sin" else [c, -s, -c, s][q]
            return float(v)
    if name == "pow":
        return ref_pow(x, y)
    raise ValueError(name)


def ref_atan2(y, x):
    """C99 Annex F atan2(y, x)"""
    if math.isnan(x) or math.isnan(y):
        return math.nan
    ny = math.copysign(1, y) < 0
    sign = -1 if ny else 1
    nx = math.copysign(1, x) < 0
    if y == 0:
        return sign * (math.pi if nx else 0.0)
    if x == 0:
        return sign * math.pi / 2
    if math.isinf(x):
        if math.isinf(y):
            return sign * (3 * math.pi / 4 if nx else math.pi / 4)
        return sign * (math.pi if nx else 0.0)
    if math.isinf(y):
        return sign * math.pi / 2
    with decimal.localcontext(CTX):
        a = atan_d(D(y) / D(x))
        if x < 0:
            a = a + PI if y > 0 else a - PI
        return float(a)


def is_int(y):
    return math.isfinite(y) and y == math.floor(y)


def odd_int(y):
    return is_int(y) and abs(y) < 2 ** 53 and int(y) % 2 == 1


def ref_pow(x, y):
    """C99 Annex F pow"""
    if y == 0 or x == 1:
        return 1.0
    if math.isnan(x) or math.isnan(y):
        return math.nan
    if math.isinf(y):
        if x == -1:
            return 1.0
        big = abs(x) > 1
        return math.inf if big == (y > 0) else 0.0
    if math.isinf(x):
        if x > 0:
            return math.inf if y > 0 else 0.0
        if y > 0:
            return -math.inf if odd_int(y) else math.inf
        return -0.0 if odd_int(y) else 0.0
    if x == 0:
        neg = math.copysign(1, x) < 0 and odd_int(y)
        if y > 0:
            return -0.0 if neg else 0.0
        return -math.inf if neg else math.inf
    sign = 1
    if x < 0:
        if not is_int(y):
            return math.nan
        if odd_int(y):
            sign = -1
    with decimal.localcontext(CTX):
        lx = D(abs(x)).ln() * D(y)
        if lx > 800:
            return sign * math.inf
        if lx < -800:
            return sign * 0.0
        return sign * float(lx.exp())


def rnd_float(r, lo, hi, neg=True):
    """a random double with a binary exponent in lo..hi (subnormal below -1022)"""
    e = r.randint(lo, hi)
    s = (r.getrandbits(1) if neg else 0) << 63
    if e < -1022:
        return b2f(s | r.getrandbits(52) | 1)
    return b2f(s | (e + 1023) << 52 | r.getrandbits(52))


def near_half_pi(r):
    """the double next to a multiple of pi/2 (the hard cases of the argument reduction)"""
    k = r.choice([r.randint(1, 1000), r.randint(1, 10 ** 6), r.randint(1, 2 ** 40)])
    with decimal.localcontext(WIDE):
        v = float(k * HALF_PI)
    return b2f(f2b(v) + r.randint(-2, 2))


SPECIAL = [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0, 3.0, -3.0, 0.25, 10.0, 1e-300, 5e-324, -5e-324, 1.7976931348623157e308,
           math.inf, -math.inf, math.nan, 2.2250738585072014e-308, 1e300, -1e300]


def cases(n, seed):
    r = random.Random(seed)
    out = []
    add = lambda f, x, y=0.0: out.append((f, x, y))  # noqa: E731
    for x in SPECIAL + [709.782712893383973096, 709.79, -745.13321910194110842, -745.14, -708.4, 0.6931471805599453, 1e-20]:
        add("exp", x)
    for _ in range(n // 2):
        add("exp", r.uniform(-750, 750))
    for _ in range(n // 2):
        add("exp", rnd_float(r, -60, 9))
    for f in ("log", "log2", "log10"):
        for x in SPECIAL + [2.0 ** k for k in range(-1074, 1024, 37)] + [10.0 ** k for k in range(-300, 301, 7)]:
            add(f, x)
        for _ in range(n // 2):
            add(f, rnd_float(r, -1074, 1023, neg=False))
        for _ in range(n // 4):
            add(f, 1 + r.uniform(-1e-3, 1e-3))
        for _ in range(n // 4):
            add(f, r.uniform(0.5, 2))
    for f in ("sin", "cos"):
        for x in SPECIAL + [math.pi, math.pi / 2, math.pi / 4, 1e22, 2.0 ** 1023, 3.0 * 2 ** 1022]:
            add(f, x)
        for _ in range(n // 4):
            add(f, r.uniform(-10, 10))
        for _ in range(n // 4):
            add(f, r.uniform(-1e6, 1e6))
        for _ in range(n // 4):
            add(f, rnd_float(r, -30, 1023))
        for _ in range(n // 4):
            add(f, near_half_pi(r) * r.choice([1, -1]))
    for x in SPECIAL:
        for y in SPECIAL + [0.5, -0.5, 1.5, 1 / 3]:
            add("pow", x, y)
    for _ in range(n // 4):
        add("pow", rnd_float(r, -20, 20, neg=False), r.uniform(-50, 50))
    for _ in range(n // 4):
        add("pow", -rnd_float(r, -10, 10, neg=False), float(r.randint(-60, 60)))
    for _ in range(n // 8):
        add("pow", 1 + r.uniform(-1e-6, 1e-6), r.uniform(-1e9, 1e9))
    for _ in range(n // 8):
        add("pow", float(r.randint(-20, 20)), float(r.randint(-30, 30)))
    for _ in range(n // 4):
        x = rnd_float(r, -300, 300, neg=False)
        lim = 700 / max(abs(math.log(x)), 1e-9)
        add("pow", x, r.uniform(-lim, lim))
    for _ in range(n // 4):
        add("sqrt", rnd_float(r, -1074, 1023, neg=False))
    for x in SPECIAL:
        add("sqrt", x)
    for _ in range(n // 4):
        add("round", r.choice([r.uniform(-1e6, 1e6), r.randint(-1000, 1000) + 0.5, rnd_float(r, -5, 60)]))
    for x in SPECIAL + [0.49999999999999994, -0.49999999999999994, 2.5, -2.5, 4503599627370495.5, -4503599627370495.5]:
        add("round", x)
    # tan, the arc functions, atan2, hypot
    for x in SPECIAL + [math.pi, math.pi / 2, math.pi / 4, -math.pi / 4, 0.6744, 0.67434, 1e22, 2.0 ** 1023, 1e-9, 3e-9]:
        add("tan", x)
    for _ in range(n // 4):
        add("tan", r.uniform(-10, 10))
    for _ in range(n // 4):
        add("tan", r.uniform(-1e6, 1e6))
    for _ in range(n // 4):
        add("tan", rnd_float(r, -30, 1023))
    for _ in range(n // 4):
        add("tan", near_half_pi(r) * r.choice([1, -1]))
    for f in ("asin", "acos"):
        for x in SPECIAL + [0.975, -0.975, 0.97500000000000009, 0.5000000000000001, 0.49999999999999994, 1 - 2 ** -53, -1 + 2 ** -53, 2 ** -57, 2 ** -27, 2 ** -26]:
            add(f, x)
        for _ in range(n // 2):
            add(f, r.uniform(-1, 1))
        for _ in range(n // 4):
            add(f, rnd_float(r, -60, -1))
        for _ in range(n // 4):
            add(f, r.choice([1, -1]) * (1 - rnd_float(r, -53, -5, neg=False)))
    for x in SPECIAL + [0.4375, 0.6875, 1.1875, 2.4375, 2.0 ** 66, -2.0 ** 66, 2.0 ** -27, 2.0 ** -29]:
        add("atan", x)
    for _ in range(n // 2):
        add("atan", r.uniform(-10, 10))
    for _ in range(n // 2):
        add("atan", rnd_float(r, -60, 70))
    for x in SPECIAL:
        for y in SPECIAL:
            add("atan2", x, y)
            add("hypot", x, y)
    for _ in range(n // 2):
        add("atan2", rnd_float(r, -60, 60), rnd_float(r, -60, 60))
    for _ in range(n // 4):
        e = r.randint(55, 70)
        x = rnd_float(r, -20, 20)
        add("atan2", x * 2.0 ** e, -x * r.uniform(0.5, 2))
        add("atan2", -x * r.uniform(0.5, 2), x * 2.0 ** e)
    for _ in range(n // 4):
        add("atan2", r.uniform(-10, 10), r.uniform(-10, 10))
    for _ in range(n // 2):
        add("hypot", rnd_float(r, -1074, 1023), rnd_float(r, -1074, 1023))
    for _ in range(n // 2):
        e = r.randint(-1070, 1020)
        add("hypot", rnd_float(r, e, e + 2), rnd_float(r, e - 3, e + 3))
    for _ in range(n // 8):
        add("hypot", float(r.randint(-10 ** 6, 10 ** 6)), float(r.randint(-10 ** 6, 10 ** 6)))
    # the degree functions: exact values, the quadrants, next to multiples of 90
    exact = [float(15 * k) for k in range(-50, 51)] + [360.0 * 10 ** k + d for k in range(1, 12) for d in (30, 45, 90, 180, 270)]
    for f in ("sin_deg", "cos_deg", "tan_deg"):
        for x in SPECIAL + exact + [-x for x in exact[100:]] + [1e300, 5e-324, 1e-310, 2.0 ** 52 + 1, 2.0 ** 53 * 90]:
            add(f, x)
        for _ in range(n // 4):
            add(f, r.uniform(-720, 720))
        for _ in range(n // 4):
            add(f, rnd_float(r, -30, 1023))
        for _ in range(n // 4):
            k = r.randint(-8, 8) * 90.0
            add(f, b2f(f2b(k) + r.randint(-3, 3)) if k else rnd_float(r, -1074, -30))
        for _ in range(n // 4):
            add(f, r.uniform(-45, 45))
    return out


def wsl_path(p):
    p = str(Path(p).resolve()).replace("\\", "/")
    return "/mnt/" + p[0].lower() + p[2:] if len(p) > 1 and p[1] == ":" else p


def answers(target, out_dir, path):
    exe = ROOT / "target" / ("debug/scripttest.exe" if target == "win" else "x86_64-unknown-linux-gnu/debug/scripttest")
    if not exe.exists():
        how = "cargo build -p scripttest" if target == "win" else "python tools/linux_test.py builds it"
        sys.exit(f"script_math_check: {exe} is missing ({how})")
    if target == "linux" and os.name == "nt":
        cmd = ["wsl.exe", "-d", "Ubuntu-24.04", "--", wsl_path(exe), "--math", wsl_path(path)]
    else:
        cmd = [str(exe), "--math", str(path)]
    return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.split("\n")


JAVA = r"""
import java.nio.file.*;

public class StrictAnswers {
    public static void main(String[] args) throws Exception {
        StringBuilder sb = new StringBuilder();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            String[] f = line.split(" ");
            double x = Double.longBitsToDouble(Long.parseUnsignedLong(f[1], 16));
            double y = Double.longBitsToDouble(Long.parseUnsignedLong(f[2], 16));
            double r;
            switch (f[0]) {
                case "exp": r = StrictMath.exp(x); break;
                case "log": r = StrictMath.log(x); break;
                case "log10": r = StrictMath.log10(x); break;
                case "pow": r = StrictMath.pow(x, y); break;
                case "sin": r = StrictMath.sin(x); break;
                case "cos": r = StrictMath.cos(x); break;
                case "tan": r = StrictMath.tan(x); break;
                case "asin": r = StrictMath.asin(x); break;
                case "acos": r = StrictMath.acos(x); break;
                case "atan": r = StrictMath.atan(x); break;
                case "atan2": r = StrictMath.atan2(x, y); break;
                case "hypot": r = StrictMath.hypot(x, y); break;
                case "sqrt": r = StrictMath.sqrt(x); break;
                default: sb.append("-\n"); continue;
            }
            sb.append(String.format("%016x", Double.doubleToRawLongBits(r))).append('\n');
        }
        System.out.print(sb);
    }
}
"""


def java_answers(out_dir, path):
    src = out_dir / "StrictAnswers.java"
    src.write_text(JAVA, newline="\n")
    return subprocess.run(["java", str(src), str(path)], capture_output=True, text=True, check=True).stdout.split("\n")


def same_bits(a, b):
    """two answers in hex: the same bits, or both a NaN"""
    x, y = int(a, 16), int(b, 16)
    return x == y or (math.isnan(b2f(x)) and math.isnan(b2f(y)))


def key(x):
    b = f2b(x)
    return b if b < 1 << 63 else -(b & ((1 << 63) - 1))


def error(got_bits, want):
    """the error in ulps; None when it is not a number of ulps (NaN, sign of zero, inf)"""
    got = b2f(got_bits)
    if math.isnan(want):
        return 0 if got_bits == NAN_BITS else None
    if math.isnan(got):
        return None
    if want == 0 and got == 0:
        return 0 if math.copysign(1, want) == math.copysign(1, got) else None
    if math.isinf(want) or math.isinf(got):
        return 0 if want == got else None
    return abs(key(got) - key(want))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", choices=["win", "linux", "both"], default="win")
    ap.add_argument("-n", type=int, default=4000, help="random cases per group (default 4000)")
    ap.add_argument("--seed", type=int, default=None)
    ap.add_argument("--out", default="build", help="the folder for the file of cases")
    ap.add_argument("--java", action="store_true", help="also compare with Java's StrictMath (fdlibm)")
    opt = ap.parse_args()
    seed = opt.seed if opt.seed is not None else random.randrange(1 << 30)
    cs = cases(opt.n, seed)
    out_dir = ROOT / opt.out
    path = out_dir / "script_math_cases.txt"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(f"{f} {f2b(x):016x} {f2b(y):016x}\n" for f, x, y in cs), newline="\n")
    targets = ["win", "linux"] if opt.target == "both" else [opt.target]
    got = {t: answers(t, out_dir, path) for t in targets}
    bad = 0
    if len(targets) == 2:
        diff = sum(1 for a, b in zip(got["win"], got["linux"]) if a != b)
        print(f"win and linux: {len(cs) - diff} of {len(cs)} answers bit for bit the same")
        bad += diff
    worst = {}
    for i, (f, x, y) in enumerate(cs):
        g = got[targets[0]][i] if i < len(got[targets[0]]) else ""
        want = ref(f, x, y)
        e = error(int(g, 16), want) if g not in ("", "-") else None
        w = worst.setdefault(f, [0, 0, None, 0])
        w[1] += 1
        w[3] += e == 1
        if e is None or e > BOUND[f]:
            bad += 1
            if bad <= 20:
                print(f"BEYOND {f}({x!r}{', ' + repr(y) if f in ('pow', 'atan2', 'hypot') else ''}): got {g} want {f2b(want):016x} ({want!r})")
        if e is not None and e > w[0]:
            w[0], w[2] = e, (x, y)
    two = ("pow", "atan2", "hypot")
    for f, (e, count, at, ones) in sorted(worst.items()):
        where = f" at {at[0]!r}" + (f", {at[1]!r}" if f in two else "") if at else ""
        print(f"{f:7} {count:6} cases, {ones / count:6.2%} not correctly rounded, largest error {e} ulp{where}")
    if opt.java:
        java = java_answers(out_dir, path)
        mine = got[targets[0]]
        stats = {}
        for i, (f, x, y) in enumerate(cs):
            if i >= len(java) or java[i] in ("", "-"):
                continue
            s = stats.setdefault(f, [0, 0, []])
            s[0] += 1
            if same_bits(mine[i], java[i]):
                s[1] += 1
            elif len(s[2]) < 3:
                s[2].append(f"{f}({x!r}{', ' + repr(y) if f in two else ''}) = {mine[i]}, StrictMath {java[i]}")
        for f, (count, equal, diffs) in sorted(stats.items()):
            print(f"{f:7} {equal:6} of {count:6} bit for bit as Java's StrictMath")
            for d in diffs:
                print(f"          {d}")
    print(f"script_math_check (seed {seed}): {'ok' if bad == 0 else str(bad) + ' cases beyond the bounds'}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
