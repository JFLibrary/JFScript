// math -- the transcendental functions of the script library in software: exp, log, log2,
// log10, pow, sin, cos, tan, asin, acos, atan, atan2, the sine, cosine and tangent of
// degrees and round, bit-identical on every target
//
//   exp(x) log(x) log2(x) log10(x)   ports of fdlibm (as in FreeBSD and musl), error below
//   pow(x, y) sin(x) cos(x) tan(x)   1 ulp; exact where the result is exact: log2(8) = 3,
//   asin(x) acos(x) atan(x)          pow(2, 10) = 1024, pow(x, 0.5) = sqrt(x), exp(0) = 1,
//   atan2(y, x)                      atan2(1, 1) = pi/4 rounded
//   sin_deg(x) cos_deg(x) tan_deg(x) x in degrees: the argument reduced exactly (fmod by 360,
//                                    the quadrant, the rest r in [-45, 45] times 128 bits of
//                                    pi/180), then fdlibm's kernels; exact wherever the result
//                                    is a float (sin and cos at multiples of 30 degrees, tan
//                                    at multiples of 45: Niven's theorem says there are no
//                                    others), signed zeros and infinities as sinPi, cosPi and
//                                    tanPi of IEEE 754-2019 (tan_deg(90) = inf, tan_deg(180)
//                                    = -0)
//   round(x)                         to the nearest integer, halves away from zero
//   scalbn(x, n)                     x * 2^n, rounded once
//
// Every float operation is one of num's correctly rounded integer routines (through the F
// wrapper below, so the ports read like the C originals), so the results depend on nothing
// but the input bits.  sin, cos and tan reduce their argument exactly: integer Payne-Hanek
// with 1280 bits of 2/pi (computed with Python's integers from Machin's formula), so
// sin(1e300) is as good as sin(1).  NaN results are num::NAN; special values follow C99 Annex
// F (pow(x, 0) = 1 even for NaN, pow(-1, inf) = 1, log(0) = -inf, sin(inf) = NaN, asin(2) =
// NaN, atan2(+-0, -0) = +-pi).  The constants of tan, atan, atan2, asin and acos are fdlibm's
// (checked bit for bit against Java's StrictMath, which is fdlibm); atan2 takes FreeBSD's fix
// for |y / x| > 2^60 with x < 0 (pi/2 rounded, where fdlibm 5.3 is one ulp above).  hypot is
// in num (correctly rounded).

use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::num::{self, NAN};

/// a float whose operators are num's
#[derive(Clone, Copy)]
struct F(f64);

impl Add for F {
    type Output = F;
    fn add(self, b: F) -> F {
        F(num::add(self.0, b.0))
    }
}

impl Sub for F {
    type Output = F;
    fn sub(self, b: F) -> F {
        F(num::sub(self.0, b.0))
    }
}

impl Mul for F {
    type Output = F;
    fn mul(self, b: F) -> F {
        F(num::mul(self.0, b.0))
    }
}

impl Div for F {
    type Output = F;
    fn div(self, b: F) -> F {
        F(num::div(self.0, b.0))
    }
}

impl Neg for F {
    type Output = F;
    fn neg(self) -> F {
        F(num::neg(self.0))
    }
}

impl F {
    fn hi(self) -> u32 {
        (self.0.to_bits() >> 32) as u32
    }
    fn lo(self) -> u32 {
        self.0.to_bits() as u32
    }
    fn with_hi(self, h: u32) -> F {
        F(f64::from_bits((h as u64) << 32 | self.lo() as u64))
    }
    fn with_lo(self, l: u32) -> F {
        F(f64::from_bits((self.hi() as u64) << 32 | l as u64))
    }
    fn lt(self, b: F) -> bool {
        num::lt(self.0, b.0)
    }
    fn le(self, b: F) -> bool {
        num::le(self.0, b.0)
    }
}

fn int(i: i32) -> F {
    F(num::from_i64(i as i64))
}

/// truncated towards zero (for values that fit)
fn to_i32(x: F) -> i32 {
    num::to_i64(x.0).unwrap_or(0) as i32
}

const ONE: F = F(1.0);
const TWO: F = F(2.0);
const HALF: F = F(0.5);

/// x * 2^n, rounded once (musl's scalbn)
pub fn scalbn(x: f64, n: i32) -> f64 {
    let p1023 = f64::from_bits(0x7FE0_0000_0000_0000);
    // 2^-1022 * 2^53: the last step lands below -53 so a subnormal result is rounded once
    let m969 = f64::from_bits(0x0360_0000_0000_0000);
    let (mut y, mut n) = (x, n);
    if n > 1023 {
        y = num::mul(y, p1023);
        n -= 1023;
        if n > 1023 {
            y = num::mul(y, p1023);
            n -= 1023;
            n = n.min(1023);
        }
    } else if n < -1022 {
        y = num::mul(y, m969);
        n += 1022 - 53;
        if n < -1022 {
            y = num::mul(y, m969);
            n += 1022 - 53;
            n = n.max(-1022);
        }
    }
    num::mul(y, f64::from_bits(((0x3FF + n) as u64) << 52))
}

/// to the nearest integer, halves away from zero (C's round); keeps -0 and the sign of
/// results that round to zero
pub fn round(x: f64) -> f64 {
    let t = num::trunc(x);
    if num::is_nan(x) || t.to_bits() == x.to_bits() {
        return t;
    }
    let d = num::abs(num::sub(x, t));
    if num::le(0.5, d) {
        let neg = x.to_bits() >> 63 != 0;
        num::add(t, if neg { -1.0 } else { 1.0 })
    } else {
        t
    }
}

// ---- exp

const LN2HI: F = F(6.93147180369123816490e-01);
const LN2LO: F = F(1.90821492927058770002e-10);
const INVLN2: F = F(1.44269504088896338700e+00);
const P1: F = F(1.66666666666666019037e-01);
const P2: F = F(-2.77777777770155933842e-03);
const P3: F = F(6.61375632143793436117e-05);
const P4: F = F(-1.65339022054652515390e-06);
const P5: F = F(4.13813679705723846039e-08);

/// e^x
pub fn exp(x: f64) -> f64 {
    let x = F(x);
    let hx = x.hi() & 0x7FFF_FFFF;
    let neg = x.hi() >> 31 != 0;
    if hx >= 0x4086_232B {
        // |x| >= 708.39..
        if num::is_nan(x.0) {
            return NAN;
        }
        if F(709.782712893383973096).lt(x) {
            return f64::INFINITY;
        }
        if x.lt(F(-745.13321910194110842)) {
            return 0.0;
        }
    }
    let (hi, lo, r, k);
    if hx > 0x3FD6_2E42 {
        // |x| > ln2 / 2
        k = if hx >= 0x3FF0_A2B2 {
            to_i32(INVLN2 * x + if neg { -HALF } else { HALF })
        } else if neg {
            -1
        } else {
            1
        };
        hi = x - int(k) * LN2HI;
        lo = int(k) * LN2LO;
        r = hi - lo;
    } else if hx > 0x3E30_0000 {
        // |x| > 2^-28
        k = 0;
        hi = x;
        lo = F(0.0);
        r = x;
    } else {
        return (ONE + x).0;
    }
    let xx = r * r;
    let c = r - xx * (P1 + xx * (P2 + xx * (P3 + xx * (P4 + xx * P5))));
    let y = ONE + (r * c / (TWO - c) - lo + hi);
    if k == 0 { y.0 } else { scalbn(y.0, k) }
}

// ---- log, log2, log10

const LG1: F = F(6.666666666666735130e-01);
const LG2: F = F(3.999999999940941908e-01);
const LG3: F = F(2.857142874366239149e-01);
const LG4: F = F(2.222219843214978396e-01);
const LG5: F = F(1.818357216161805012e-01);
const LG6: F = F(1.531383769920937332e-01);
const LG7: F = F(1.479819860511658591e-01);

/// the common part of the logarithms: x = 2^k * (1 + f) with 1 + f in [sqrt(2)/2, sqrt(2)];
/// Err for the special values (the result itself)
fn log_reduce(x: f64) -> Result<(i32, F), f64> {
    let mut u = x.to_bits();
    let mut hx = (u >> 32) as u32;
    let mut k: i32 = 0;
    if hx < 0x0010_0000 || hx >> 31 != 0 {
        if u << 1 == 0 {
            return Err(f64::NEG_INFINITY);
        }
        if hx >> 31 != 0 {
            return Err(NAN);
        }
        // subnormal: scale up by 2^54
        k -= 54;
        u = num::mul(x, 18014398509481984.0).to_bits();
        hx = (u >> 32) as u32;
    } else if hx >= 0x7FF0_0000 {
        return Err(if num::is_nan(x) { NAN } else { x });
    } else if hx == 0x3FF0_0000 && u << 32 == 0 {
        return Err(0.0);
    }
    hx = hx.wrapping_add(0x3FF0_0000 - 0x3FE6_A09E);
    k += (hx >> 20) as i32 - 0x3FF;
    hx = (hx & 0x000F_FFFF) + 0x3FE6_A09E;
    u = (hx as u64) << 32 | (u & 0xFFFF_FFFF);
    Ok((k, F(f64::from_bits(u)) - ONE))
}

/// s, hfsq and R of log(1 + f) = f - hfsq + s * (hfsq + R)
fn log_poly(f: F) -> (F, F, F) {
    let hfsq = HALF * f * f;
    let s = f / (TWO + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    (s, hfsq, t2 + t1)
}

/// the natural logarithm
pub fn log(x: f64) -> f64 {
    let (k, f) = match log_reduce(x) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (s, hfsq, r) = log_poly(f);
    let dk = int(k);
    (s * (hfsq + r) + dk * LN2LO - hfsq + f + dk * LN2HI).0
}

/// hi + lo = log(1 + f) with hi's low word cleared (the extra precision of log2 / log10)
fn log_split(f: F) -> (F, F) {
    let (s, hfsq, r) = log_poly(f);
    let hi = (f - hfsq).with_lo(0);
    let lo = f - hi - hfsq + s * (hfsq + r);
    (hi, lo)
}

const IVLN2HI: F = F(1.44269504072144627571e+00);
const IVLN2LO: F = F(1.67517131648865118353e-10);

/// the logarithm to base 2 (exact for powers of two)
pub fn log2(x: f64) -> f64 {
    let (k, f) = match log_reduce(x) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (hi, lo) = log_split(f);
    let mut val_hi = hi * IVLN2HI;
    let mut val_lo = (lo + hi) * IVLN2LO + lo * IVLN2HI;
    let y = int(k);
    let w = y + val_hi;
    val_lo = val_lo + ((y - w) + val_hi);
    val_hi = w;
    (val_lo + val_hi).0
}

const IVLN10HI: F = F(4.34294481878168880939e-01);
const IVLN10LO: F = F(2.50829467116452752298e-11);
const LOG10_2HI: F = F(3.01029995663611771306e-01);
const LOG10_2LO: F = F(3.69423907715893078616e-13);

/// the logarithm to base 10
pub fn log10(x: f64) -> f64 {
    let (k, f) = match log_reduce(x) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (hi, lo) = log_split(f);
    let mut val_hi = hi * IVLN10HI;
    let dk = int(k);
    let y = dk * LOG10_2HI;
    let mut val_lo = dk * LOG10_2LO + (lo + hi) * IVLN10LO + lo * IVLN10HI;
    let w = y + val_hi;
    val_lo = val_lo + ((y - w) + val_hi);
    val_hi = w;
    (val_lo + val_hi).0
}

// ---- pow

const BP: [F; 2] = [F(1.0), F(1.5)];
const DP_H: [F; 2] = [F(0.0), F(5.84962487220764160156e-01)];
const DP_L: [F; 2] = [F(0.0), F(1.35003920212974897128e-08)];
const TWO53: f64 = 9007199254740992.0;
const HUGE: F = F(1.0e300);
const TINY: F = F(1.0e-300);
const L1: F = F(5.99999999999994648725e-01);
const L2: F = F(4.28571428578550184252e-01);
const L3: F = F(3.33333329818377432918e-01);
const L4: F = F(2.72728123808534006489e-01);
const L5: F = F(2.30660745775561754067e-01);
const L6: F = F(2.06975017800338417784e-01);
const LG2P: F = F(6.93147180559945286227e-01);
const LG2_H: F = F(6.93147182464599609375e-01);
const LG2_L: F = F(-1.90465429995776804525e-09);
const OVT: F = F(8.0085662595372944372e-17);
const CP: F = F(9.61796693925975554329e-01);
const CP_H: F = F(9.61796700954437255859e-01);
const CP_L: F = F(-7.02846165095275826516e-09);
const IVLN2: F = F(1.44269504088896338700e+00);
const IVLN2_H: F = F(1.44269502162933349609e+00);
const IVLN2_L: F = F(1.92596299112661746887e-08);

fn words(x: f64) -> (i32, u32) {
    ((x.to_bits() >> 32) as i32, x.to_bits() as u32)
}

/// x to the power y
pub fn pow(x: f64, y: f64) -> f64 {
    let (hx, lx) = words(x);
    let (hy, ly) = words(y);
    let mut ix = hx & 0x7FFF_FFFF;
    let iy = hy & 0x7FFF_FFFF;
    // x^0 = 1 and 1^y = 1, even for NaN
    if iy as u32 | ly == 0 || (hx == 0x3FF0_0000 && lx == 0) {
        return 1.0;
    }
    if ix > 0x7FF0_0000 || (ix == 0x7FF0_0000 && lx != 0) || iy > 0x7FF0_0000 || (iy == 0x7FF0_0000 && ly != 0) {
        return NAN;
    }
    // for x < 0: 0 when y is no integer, 1 for an odd one, 2 for an even one
    let mut yisint = 0;
    if hx < 0 {
        if iy >= 0x4340_0000 {
            yisint = 2;
        } else if iy >= 0x3FF0_0000 {
            let k = (iy >> 20) - 0x3FF;
            if k > 20 {
                let j = ly >> (52 - k);
                if j << (52 - k) == ly {
                    yisint = 2 - (j & 1) as i32;
                }
            } else if ly == 0 {
                let j = (iy as u32) >> (20 - k);
                if j << (20 - k) == iy as u32 {
                    yisint = 2 - (j & 1) as i32;
                }
            }
        }
    }
    if ly == 0 {
        if iy == 0x7FF0_0000 {
            // y = +-inf
            return if (ix - 0x3FF0_0000) as u32 | lx == 0 {
                1.0
            } else if ix >= 0x3FF0_0000 {
                if hy >= 0 { y } else { 0.0 }
            } else if hy >= 0 {
                0.0
            } else {
                num::neg(y)
            };
        }
        if iy == 0x3FF0_0000 {
            return if hy >= 0 { x } else { num::div(1.0, x) };
        }
        if hy == 0x4000_0000 {
            return num::mul(x, x);
        }
        if hy == 0x3FE0_0000 && hx >= 0 {
            return num::sqrt(x);
        }
    }
    let mut ax = num::abs(x);
    if lx == 0 && (ix == 0x7FF0_0000 || ix == 0 || ix == 0x3FF0_0000) {
        // x is +-0, +-inf, +-1
        let mut z = ax;
        if hy < 0 {
            z = num::div(1.0, z);
        }
        if hx < 0 {
            if (ix - 0x3FF0_0000) | yisint == 0 {
                z = NAN;
            } else if yisint == 1 {
                z = num::neg(z);
            }
        }
        return z;
    }
    let mut s = ONE;
    if hx < 0 {
        if yisint == 0 {
            return NAN;
        }
        if yisint == 1 {
            s = F(-1.0);
        }
    }
    let (t1, t2);
    if iy > 0x41E0_0000 {
        // |y| > 2^31
        if iy > 0x43F0_0000 {
            // |y| > 2^64: over- or underflows
            if ix <= 0x3FEF_FFFF {
                return if hy < 0 { (HUGE * HUGE).0 } else { (TINY * TINY).0 };
            }
            if ix >= 0x3FF0_0000 {
                return if hy > 0 { (HUGE * HUGE).0 } else { (TINY * TINY).0 };
            }
        }
        if ix < 0x3FEF_FFFF {
            return if hy < 0 { (s * HUGE * HUGE).0 } else { (s * TINY * TINY).0 };
        }
        if ix > 0x3FF0_0000 {
            return if hy > 0 { (s * HUGE * HUGE).0 } else { (s * TINY * TINY).0 };
        }
        // |1 - x| <= 2^-20: log(x) by x - x^2/2 + x^3/3 - x^4/4
        let t = F(ax) - ONE;
        let w = (t * t) * (HALF - t * (F(0.3333333333333333333333) - t * F(0.25)));
        let u = IVLN2_H * t;
        let v = t * IVLN2_L - w * IVLN2;
        t1 = (u + v).with_lo(0);
        t2 = v - (t1 - u);
    } else {
        let mut n: i32 = 0;
        if ix < 0x0010_0000 {
            // subnormal x
            ax = num::mul(ax, TWO53);
            n -= 53;
            ix = (ax.to_bits() >> 32) as i32;
        }
        n += (ix >> 20) - 0x3FF;
        let j = ix & 0x000F_FFFF;
        ix = j | 0x3FF0_0000;
        let k: usize;
        if j <= 0x3988E {
            // |x| < sqrt(3/2)
            k = 0;
        } else if j < 0xBB67A {
            // |x| < sqrt(3)
            k = 1;
        } else {
            k = 0;
            n += 1;
            ix -= 0x0010_0000;
        }
        let ax = F(ax).with_hi(ix as u32);
        // ss = s_h + s_l = (x - 1) / (x + 1) or (x - 1.5) / (x + 1.5)
        let u = ax - BP[k];
        let v = ONE / (ax + BP[k]);
        let ss = u * v;
        let s_h = ss.with_lo(0);
        let t_h = F(0.0).with_hi((((ix >> 1) as u32) | 0x2000_0000) + 0x0008_0000 + ((k as u32) << 18));
        let t_l = ax - (t_h - BP[k]);
        let s_l = v * ((u - s_h * t_h) - s_h * t_l);
        // log(ax)
        let s2 = ss * ss;
        let mut r = s2 * s2 * (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
        r = r + s_l * (s_h + ss);
        let s2 = s_h * s_h;
        let t_h = (F(3.0) + s2 + r).with_lo(0);
        let t_l = r - ((t_h - F(3.0)) - s2);
        let u = s_h * t_h;
        let v = s_l * t_h + t_l * ss;
        // 2 / (3 log2) * (ss + ..)
        let p_h = (u + v).with_lo(0);
        let p_l = v - (p_h - u);
        let z_h = CP_H * p_h;
        let z_l = CP_L * p_h + p_l * CP + DP_L[k];
        // log2(ax) = n + dp_h + z_h + z_l
        let t = int(n);
        t1 = (((z_h + z_l) + DP_H[k]) + t).with_lo(0);
        t2 = z_l - (((t1 - t) - DP_H[k]) - z_h);
    }
    // (y1 + y2) * (t1 + t2) with y split in two
    let y = F(y);
    let y1 = y.with_lo(0);
    let p_l = (y - y1) * t1 + y * t2;
    let mut p_h = y1 * t1;
    let z = p_l + p_h;
    let (j, i) = words(z.0);
    if j >= 0x4090_0000 {
        // z >= 1024
        if (j - 0x4090_0000) as u32 | i != 0 || (z - p_h).lt(p_l + OVT) {
            return (s * HUGE * HUGE).0;
        }
    } else if j & 0x7FFF_FFFF >= 0x4090_CC00 {
        // z <= -1075
        if (j as u32).wrapping_sub(0xC090_CC00) | i != 0 || p_l.le(z - p_h) {
            return (s * TINY * TINY).0;
        }
    }
    // 2^(p_h + p_l)
    let i = j & 0x7FFF_FFFF;
    let mut k = (i >> 20) - 0x3FF;
    let mut n: i32 = 0;
    if i > 0x3FE0_0000 {
        // |z| > 0.5: n = [z + 0.5]
        n = j.wrapping_add(0x0010_0000 >> (k + 1));
        k = ((n & 0x7FFF_FFFF) >> 20) - 0x3FF;
        let t = F(0.0).with_hi((n & !(0x000F_FFFF >> k)) as u32);
        n = ((n & 0x000F_FFFF) | 0x0010_0000) >> (20 - k);
        if j < 0 {
            n = -n;
        }
        p_h = p_h - t;
    }
    let t = (p_l + p_h).with_lo(0);
    let u = t * LG2_H;
    let v = (p_l - (t - p_h)) * LG2P + t * LG2_L;
    let z = u + v;
    let w = v - (z - u);
    let t = z * z;
    let t1 = z - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    let r = (z * t1) / (t1 - TWO) - (w + z * w);
    let z = ONE - (r - z);
    let j = (z.hi() as i32).wrapping_add(n << 20);
    let z = if j >> 20 <= 0 { F(scalbn(z.0, n)) } else { z.with_hi(j as u32) };
    (s * z).0
}

// ---- sin, cos

const S1: F = F(-1.66666666666666324348e-01);
const S2: F = F(8.33333333332248946124e-03);
const S3: F = F(-1.98412698298579493134e-04);
const S4: F = F(2.75573137070700676789e-06);
const S5: F = F(-2.50507602534068634195e-08);
const S6: F = F(1.58969099521155010221e-10);

/// sin(x + y) for |x + y| <= pi/4, y the tail of the reduced argument (fdlibm's __sin)
fn ksin(x: F, y: F, tail: bool) -> F {
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    if tail { x - ((z * (HALF * y - v * r) - y) - v * S1) } else { x + v * (S1 + z * r) }
}

const C1: F = F(4.16666666666666019037e-02);
const C2: F = F(-1.38888888888741095749e-03);
const C3: F = F(2.48015872894767294178e-05);
const C4: F = F(-2.75573143513906633035e-07);
const C5: F = F(2.08757232129817482790e-09);
const C6: F = F(-1.13596475577881948265e-11);

/// cos(x + y) for |x + y| <= pi/4 (fdlibm's __cos)
fn kcos(x: F, y: F) -> F {
    let z = x * x;
    let w = z * z;
    let r = z * (C1 + z * (C2 + z * C3)) + w * w * (C4 + z * (C5 + z * C6));
    let hz = HALF * z;
    let w = ONE - hz;
    w + (((ONE - w) - hz) + (z * r - x * y))
}

/// the bits of 2/pi after the binary point, most significant first (1280 of them)
const TWO_OVER_PI: [u64; 20] = [
    0xA2F9836E4E441529, 0xFC2757D1F534DDC0, 0xDB6295993C439041, 0xFE5163ABDEBBC561,
    0xB7246E3A424DD2E0, 0x06492EEA09D1921C, 0xFE1DEB1CB129A73E, 0xE88235F52EBB4484,
    0xE99C7026B45F7E41, 0x3991D639835339F4, 0x9C845F8BBDF9283B, 0x1FF897FFDE05980F,
    0xEF2F118B5A0A6D1F, 0x6D367ECF27CB09B7, 0x4F463F669E5FEA2D, 0x7527BAC7EBE5F17B,
    0x3D0739F78A5292EA, 0x6BFB5FB11F8D5D08, 0x56033046FC7B6BAB, 0xF0CFBC209AF4361D,
];

/// pi/2 * 2^127, rounded
const PIO2_Q127: u128 = 0xC90F_DAA2_2168_C234_C4C6_628B_80DC_1CD1;

/// 256 bits of 2/pi from bit t on (bit 1 is the first after the point), as a little-endian
/// number
fn window(t: u32) -> [u64; 4] {
    let get = |i: u32| -> u64 {
        let pos = t - 1 + 64 * i;
        let (w, o) = ((pos / 64) as usize, pos % 64);
        let a = TWO_OVER_PI.get(w).copied().unwrap_or(0);
        let b = TWO_OVER_PI.get(w + 1).copied().unwrap_or(0);
        if o == 0 { a } else { (a << o) | (b >> (64 - o)) }
    };
    [get(3), get(2), get(1), get(0)]
}

/// bits lo .. lo + 128 of a little-endian number
fn bits128(p: &[u64; 6], lo: u32) -> u128 {
    let w = (lo / 64) as usize;
    let o = lo % 64;
    let g = |i: usize| -> u128 { p.get(i).copied().unwrap_or(0) as u128 };
    let a = g(w) | (g(w + 1) << 64);
    if o == 0 { a } else { (a >> o) | (g(w + 2) << (128 - o)) }
}

/// the full product of two u128
fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    let (a1, a0) = (a >> 64, a & u64::MAX as u128);
    let (b1, b0) = (b >> 64, b & u64::MAX as u128);
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = (p00 >> 64) + (p01 & u64::MAX as u128) + (p10 & u64::MAX as u128);
    let lo = (p00 & u64::MAX as u128) | (mid << 64);
    let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
    (hi, lo)
}

/// x = n * pi/2 + y0 + y1 with |y0 + y1| <= pi/4, for finite |x| > pi/4: the exact product
/// of x's 53-bit mantissa with a 256-bit window of 2/pi (the bits above it only add multiples
/// of 4), then the fraction times pi/2 as a double-double
fn rem_pio2(x: f64) -> (i32, F, F) {
    let b = x.to_bits();
    let neg = b >> 63 != 0;
    let m = (b & ((1 << 52) - 1)) | 1 << 52;
    let e = ((b >> 52) & 0x7FF) as i32 - 1075;
    let t = (e - 1).max(1);
    let w = window(t as u32);
    let mut p = [0u64; 6];
    let mut carry: u128 = 0;
    for i in 0..4 {
        let v = m as u128 * w[i] as u128 + carry;
        p[i] = v as u64;
        carry = v >> 64;
    }
    p[4] = carry as u64;
    // |x| * 2/pi = p * 2^(e - t - 255): sh fraction bits
    let sh = (t + 255 - e) as u32;
    let mut q = (bits128(&p, sh) & 3) as i32;
    let f = bits128(&p, sh - 128);
    let (neg_r, mag) = if f >> 127 != 0 {
        q += 1;
        (true, f.wrapping_neg())
    } else {
        (false, f)
    };
    // r = mag * 2^-128 * pi/2
    let (hi, lo) = mul_wide(mag, PIO2_Q127);
    let (y0, y1) = dd(hi, lo, -255);
    let (y0, y1) = if neg_r != neg { (-y0, -y1) } else { (y0, y1) };
    (if neg { -q } else { q }, y0, y1)
}

/// y0 + y1 = the 256-bit number hi * 2^128 + lo, times 2^p: y0 rounded to nearest, y1 the
/// rest to 63 bits; (0, 0) for 0
fn dd(hi: u128, lo: u128, p: i32) -> (F, F) {
    let (v, lz) = if hi != 0 {
        let lz = hi.leading_zeros();
        (if lz == 0 { hi } else { (hi << lz) | (lo >> (128 - lz)) }, lz as i32)
    } else if lo != 0 {
        let lz = lo.leading_zeros();
        (lo << lz, 128 + lz as i32)
    } else {
        return (F(0.0), F(0.0));
    };
    // the value is v * 2^q, v with bit 127 set
    let q = p + 128 - lz;
    let mut m53 = (v >> 75) as u64;
    let rest = v & ((1u128 << 75) - 1);
    let half = 1u128 << 74;
    let mut tail = rest as i128;
    if rest > half || (rest == half && m53 & 1 == 1) {
        m53 += 1;
        tail -= 1i128 << 75;
    }
    let y0 = scalbn(num::from_i64(m53 as i64), q + 75);
    let ta = tail.unsigned_abs();
    let drop = (128 - ta.leading_zeros()).saturating_sub(63);
    let y1 = scalbn(num::from_i64((ta >> drop) as i64), q + drop as i32);
    (F(y0), if tail < 0 { -F(y1) } else { F(y1) })
}

/// the sine
pub fn sin(x: f64) -> f64 {
    let ix = F(x).hi() & 0x7FFF_FFFF;
    if ix <= 0x3FE9_21FB {
        // |x| ~<= pi/4
        if ix < 0x3E50_0000 {
            // |x| < 2^-26
            return x;
        }
        return ksin(F(x), F(0.0), false).0;
    }
    if ix >= 0x7FF0_0000 {
        return NAN;
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => ksin(y0, y1, true).0,
        1 => kcos(y0, y1).0,
        2 => (-ksin(y0, y1, true)).0,
        _ => (-kcos(y0, y1)).0,
    }
}

/// the cosine
pub fn cos(x: f64) -> f64 {
    let ix = F(x).hi() & 0x7FFF_FFFF;
    if ix <= 0x3FE9_21FB {
        if ix < 0x3E46_A09E {
            // |x| < 2^-27 * sqrt(2)
            return 1.0;
        }
        return kcos(F(x), F(0.0)).0;
    }
    if ix >= 0x7FF0_0000 {
        return NAN;
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kcos(y0, y1).0,
        1 => (-ksin(y0, y1, true)).0,
        2 => (-kcos(y0, y1)).0,
        _ => ksin(y0, y1, true).0,
    }
}

// ---- tan

/// a float from its bits
const fn b(bits: u64) -> F {
    F(f64::from_bits(bits))
}

const PIO4: F = b(0x3FE9_21FB_5444_2D18);
const PIO4LO: F = b(0x3C81_A626_3314_5C07);
const T: [F; 13] = [
    b(0x3FD5_5555_5555_5563), b(0x3FC1_1111_1110_FE7A), b(0x3FAB_A1BA_1BB3_41FE), b(0x3F96_64F4_8406_D637),
    b(0x3F82_26E3_E96E_8493), b(0x3F6D_6D22_C956_0328), b(0x3F57_DBC8_FEE0_8315), b(0x3F43_44D8_F2F2_6501),
    b(0x3F30_26F7_1A8D_1068), b(0x3F14_7E88_A037_92A6), b(0x3F12_B80F_32F0_A7E9), b(0xBEF3_75CB_DB60_5373),
    b(0x3EFB_2A70_74BF_7AD4),
];

/// tan(x + y) (k = 1) or -1 / tan(x + y) (k = -1) for |x + y| <= pi/4, y the tail (fdlibm's
/// __kernel_tan, its 2004 version)
fn ktan(x: F, y: F, k: i32) -> F {
    let hx = x.hi() as i32;
    let ix = hx & 0x7FFF_FFFF;
    if ix < 0x3E30_0000 {
        // |x| < 2^-28
        if ix as u32 | x.lo() == 0 && k == -1 {
            return F(num::div(1.0, num::abs(x.0)));
        }
        if k == 1 {
            return x;
        }
        // -1 / (x + y), carefully
        let w = x + y;
        let z = w.with_lo(0);
        let v = y - (z - x);
        let a = F(-1.0) / w;
        let t = a.with_lo(0);
        let s = ONE + t * z;
        return t + a * (s + t * v);
    }
    let big = ix >= 0x3FE5_9428;
    let (mut x, mut y) = (x, y);
    if big {
        // |x| >= 0.6744: tan(pi/4 - x)
        if hx < 0 {
            x = -x;
            y = -y;
        }
        x = (PIO4 - x) + (PIO4LO - y);
        y = F(0.0);
    }
    let z = x * x;
    let w = z * z;
    // x^5 (T1 + x^2 T2 + ..) split into its odd and even halves
    let r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    let r = y + z * (s * (r + v) + y) + T[0] * s;
    let w = x + r;
    if big {
        let v = int(k);
        let t = v - TWO * (x - (w * w / (w + v) - r));
        return if hx < 0 { -t } else { t };
    }
    if k == 1 {
        return w;
    }
    // -1 / (x + r), carefully
    let z = w.with_lo(0);
    let v = r - (z - x);
    let a = F(-1.0) / w;
    let t = a.with_lo(0);
    let s = ONE + t * z;
    t + a * (s + t * v)
}

/// the tangent
pub fn tan(x: f64) -> f64 {
    let ix = F(x).hi() & 0x7FFF_FFFF;
    if ix <= 0x3FE9_21FB {
        return ktan(F(x), F(0.0), 1).0;
    }
    if ix >= 0x7FF0_0000 {
        return NAN;
    }
    let (n, y0, y1) = rem_pio2(x);
    ktan(y0, y1, 1 - ((n & 1) << 1)).0
}

// ---- atan, atan2

const ATANHI: [F; 4] = [b(0x3FDD_AC67_0561_BB4F), b(0x3FE9_21FB_5444_2D18), b(0x3FEF_730B_D281_F69B), b(0x3FF9_21FB_5444_2D18)];
const ATANLO: [F; 4] = [b(0x3C7A_2B7F_222F_65E2), b(0x3C81_A626_3314_5C07), b(0x3C70_0788_7AF0_CBBD), b(0x3C91_A626_3314_5C07)];
const AT: [F; 11] = [
    b(0x3FD5_5555_5555_550D), b(0xBFC9_9999_9998_EBC4), b(0x3FC2_4924_9200_83FF), b(0xBFBC_71C6_FE23_1671),
    b(0x3FB7_45CD_C54C_206E), b(0xBFB3_B0F2_AF74_9A6D), b(0x3FB1_0D66_A0D0_3D51), b(0xBFAD_DE2D_52DE_FD9A),
    b(0x3FA9_7B4B_2476_0DEB), b(0xBFA2_B444_2C6A_6C2F), b(0x3F90_AD3A_E322_DA11),
];

/// the arc tangent, in (-pi/2, pi/2)
pub fn atan(x: f64) -> f64 {
    let hx = F(x).hi() as i32;
    let ix = hx & 0x7FFF_FFFF;
    let mut x = F(x);
    let id: usize;
    if ix >= 0x4410_0000 {
        // |x| >= 2^66
        if num::is_nan(x.0) {
            return NAN;
        }
        return if hx > 0 { (ATANHI[3] + ATANLO[3]).0 } else { (-ATANHI[3] - ATANLO[3]).0 };
    }
    if ix < 0x3FDC_0000 {
        // |x| < 0.4375
        if ix < 0x3E40_0000 {
            // |x| < 2^-27
            return x.0;
        }
        id = 4;
    } else {
        x = F(num::abs(x.0));
        if ix < 0x3FF3_0000 {
            if ix < 0x3FE6_0000 {
                // 7/16 <= |x| < 11/16
                id = 0;
                x = (TWO * x - ONE) / (TWO + x);
            } else {
                // 11/16 <= |x| < 19/16
                id = 1;
                x = (x - ONE) / (x + ONE);
            }
        } else if ix < 0x4003_8000 {
            // |x| < 2.4375
            id = 2;
            x = (x - F(1.5)) / (ONE + F(1.5) * x);
        } else {
            id = 3;
            x = F(-1.0) / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id == 4 {
        return (x - x * (s1 + s2)).0;
    }
    let z = ATANHI[id] - ((x * (s1 + s2) - ATANLO[id]) - x);
    if hx < 0 { (-z).0 } else { z.0 }
}

const PI: F = b(0x4009_21FB_5444_2D18);
const PI_O_2: F = b(0x3FF9_21FB_5444_2D18);
const PI_O_4: F = b(0x3FE9_21FB_5444_2D18);
const PI_LO: F = b(0x3CA1_A626_3314_5C07);

/// the angle of the point (x, y) (FreeBSD's e_atan2.c): atan(y / x) in the right quadrant,
/// in [-pi, pi]; C99 for zeros and infinities (atan2(+-0, -0) = +-pi, atan2(+-0, +0) = +-0)
pub fn atan2(y: f64, x: f64) -> f64 {
    if num::is_nan(x) || num::is_nan(y) {
        return NAN;
    }
    let (hx, lx) = words(x);
    let (hy, ly) = words(y);
    let (ix, iy) = (hx & 0x7FFF_FFFF, hy & 0x7FFF_FFFF);
    if hx.wrapping_sub(0x3FF0_0000) as u32 | lx == 0 {
        // x = 1
        return atan(y);
    }
    // 2 * sign(x) + sign(y)
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if iy as u32 | ly == 0 {
        return match m {
            0 | 1 => y,
            2 => (PI + TINY).0,
            _ => (-PI - TINY).0,
        };
    }
    if ix as u32 | lx == 0 {
        return if hy < 0 { (-PI_O_2 - TINY).0 } else { (PI_O_2 + TINY).0 };
    }
    if ix == 0x7FF0_0000 {
        let r = if iy == 0x7FF0_0000 {
            match m {
                0 => PI_O_4 + TINY,
                1 => -PI_O_4 - TINY,
                2 => F(3.0) * PI_O_4 + TINY,
                _ => F(-3.0) * PI_O_4 - TINY,
            }
        } else {
            match m {
                0 => F(0.0),
                1 => F(-0.0),
                2 => PI + TINY,
                _ => -PI - TINY,
            }
        };
        return r.0;
    }
    if iy == 0x7FF0_0000 {
        return if hy < 0 { (-PI_O_2 - TINY).0 } else { (PI_O_2 + TINY).0 };
    }
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        // |y / x| > 2^60: pi/2 with the sign of y
        m &= 1;
        PI_O_2 + HALF * PI_LO
    } else if hx < 0 && k < -60 {
        // 0 > |y| / x > -2^-60
        F(0.0)
    } else {
        F(atan(num::abs(num::div(y, x))))
    };
    match m {
        0 => z.0,
        1 => (-z).0,
        2 => (PI - (z - PI_LO)).0,
        _ => ((z - PI_LO) - PI).0,
    }
}

// ---- asin, acos

const PIO2_HI: F = b(0x3FF9_21FB_5444_2D18);
const PIO2_LO: F = b(0x3C91_A626_3314_5C07);
const PS: [F; 6] = [
    b(0x3FC5_5555_5555_5555), b(0xBFD4_D612_03EB_6F7D), b(0x3FC9_C155_0E88_4455),
    b(0xBFA4_8228_B568_8F3B), b(0x3F49_EFE0_7501_B288), b(0x3F02_3DE1_0DFD_F709),
];
const QS: [F; 4] = [b(0xC003_3A27_1C8A_2D4B), b(0x4000_2AE5_9C59_8AC8), b(0xBFE6_066C_1B8D_0159), b(0x3FB3_B8C5_B12E_9282)];

/// the rational function of asin and acos: p(t) / q(t)
fn asin_r(t: F) -> F {
    let p = t * (PS[0] + t * (PS[1] + t * (PS[2] + t * (PS[3] + t * (PS[4] + t * PS[5])))));
    let q = ONE + t * (QS[0] + t * (QS[1] + t * (QS[2] + t * QS[3])));
    p / q
}

/// the arc sine, in [-pi/2, pi/2]; NaN outside [-1, 1]
pub fn asin(x: f64) -> f64 {
    let (hx, lx) = words(x);
    let ix = hx & 0x7FFF_FFFF;
    let x = F(x);
    if ix >= 0x3FF0_0000 {
        if (ix - 0x3FF0_0000) as u32 | lx == 0 {
            // +-1: +-pi/2
            return (x * PIO2_HI + x * PIO2_LO).0;
        }
        return NAN;
    }
    if ix < 0x3FE0_0000 {
        // |x| < 0.5
        if ix < 0x3E50_0000 {
            // |x| < 2^-26
            return x.0;
        }
        return (x + x * asin_r(x * x)).0;
    }
    // 0.5 <= |x| < 1
    let t = (ONE - F(num::abs(x.0))) * HALF;
    let r = asin_r(t);
    let s = F(num::sqrt(t.0));
    let t = if ix >= 0x3FEF_3333 {
        // |x| > 0.975
        PIO2_HI - (TWO * (s + s * r) - PIO2_LO)
    } else {
        let w = s.with_lo(0);
        let c = (t - w * w) / (s + w);
        let p = TWO * s * r - (PIO2_LO - TWO * c);
        let q = PIO4 - TWO * w;
        PIO4 - (p - q)
    };
    if hx > 0 { t.0 } else { (-t).0 }
}

/// the arc cosine, in [0, pi]; NaN outside [-1, 1]
pub fn acos(x: f64) -> f64 {
    let (hx, lx) = words(x);
    let ix = hx & 0x7FFF_FFFF;
    let x = F(x);
    if ix >= 0x3FF0_0000 {
        if (ix - 0x3FF0_0000) as u32 | lx == 0 {
            // acos(1) = 0, acos(-1) = pi
            return if hx > 0 { 0.0 } else { (PI + TWO * PIO2_LO).0 };
        }
        return NAN;
    }
    if ix < 0x3FE0_0000 {
        // |x| < 0.5
        if ix <= 0x3C60_0000 {
            // |x| < 2^-57
            return (PIO2_HI + PIO2_LO).0;
        }
        let r = asin_r(x * x);
        return (PIO2_HI - (x - (PIO2_LO - x * r))).0;
    }
    if hx < 0 {
        // x <= -0.5
        let z = (ONE + x) * HALF;
        let s = F(num::sqrt(z.0));
        let w = asin_r(z) * s - PIO2_LO;
        return (PI - TWO * (s + w)).0;
    }
    // x >= 0.5
    let z = (ONE - x) * HALF;
    let s = F(num::sqrt(z.0));
    let df = s.with_lo(0);
    let c = (z - df * df) / (s + df);
    let w = asin_r(z) * s + c;
    (TWO * (df + w)).0
}

// ---- sin, cos and tan of degrees

/// pi/180 * 2^133, rounded (bit 127 set)
const PI180_Q133: u128 = 0x8EFA_3512_94E9_C8AE_0EC5_F66E_9485_C4D9;

/// sqrt(1/2), correctly rounded: sin and cos of 45 degrees
const SQRT_HALF: f64 = f64::from_bits(0x3FE6_A09E_667F_3BCD);

/// x degrees = 360 j + 90 k + r exactly, with r in [-45, 45] (k: the quadrant, any sign; at
/// the ties r is 45 or -45); for finite x
fn deg_reduce(x: f64) -> (i32, f64) {
    // exact, |y| < 360, with the sign of x
    let y = num::rem(x, 360.0);
    let a = num::abs(y);
    let k = if num::le(a, 45.0) {
        0
    } else if num::le(a, 135.0) {
        1
    } else if num::le(a, 225.0) {
        2
    } else if num::le(a, 315.0) {
        3
    } else {
        4
    };
    // exact: a and 90 k are multiples of a's ulp, |r| <= 45
    let r = num::sub(a, num::from_i64(90 * k as i64));
    if is_neg(y) { (-k, num::neg(r)) } else { (k, r) }
}

/// r degrees in radians as y0 + y1, for 0 < |r| <= 45: r's mantissa times 128 bits of pi/180,
/// exactly
fn deg_rad(r: f64) -> (F, F) {
    let bits = r.to_bits();
    let be = ((bits >> 52) & 0x7FF) as i32;
    let frac = bits & ((1 << 52) - 1);
    let (m, e) = if be == 0 { (frac, -1074) } else { (frac | 1 << 52, be - 1075) };
    let (hi, lo) = mul_wide(m as u128, PI180_Q133);
    let (y0, y1) = dd(hi, lo, e - 133);
    if is_neg(r) { (-y0, -y1) } else { (y0, y1) }
}

fn is_zero(x: f64) -> bool {
    x.to_bits() << 1 == 0
}

fn is_neg(x: f64) -> bool {
    x.to_bits() >> 63 != 0
}

/// |x| is a, exactly
fn abs_is(x: f64, a: f64) -> bool {
    x.to_bits() << 1 == a.to_bits() << 1
}

/// sin of r degrees, |r| <= 45 (exact at 0, 30 and 45: 0, 1/2, sqrt(1/2) rounded)
fn sin_r(r: f64) -> f64 {
    if is_zero(r) {
        return r;
    }
    let v = if abs_is(r, 30.0) {
        0.5
    } else if abs_is(r, 45.0) {
        SQRT_HALF
    } else {
        let (y0, y1) = deg_rad(r);
        return ksin(y0, y1, true).0;
    };
    if is_neg(r) { -v } else { v }
}

/// cos of r degrees, |r| <= 45
fn cos_r(r: f64) -> f64 {
    if is_zero(r) {
        1.0
    } else if abs_is(r, 45.0) {
        SQRT_HALF
    } else {
        let (y0, y1) = deg_rad(r);
        kcos(y0, y1).0
    }
}

/// the sine of x degrees: exact where the result is a float (multiples of 30 degrees: 0,
/// +-1/2, +-1), a zero with the sign of x (sinPi of IEEE 754-2019); NaN for inf and NaN
pub fn sin_deg(x: f64) -> f64 {
    if !finite(x) {
        return NAN;
    }
    let (k, r) = deg_reduce(x);
    let v = match k.rem_euclid(4) {
        0 => sin_r(r),
        1 => cos_r(r),
        2 => num::neg(sin_r(r)),
        _ => num::neg(cos_r(r)),
    };
    if is_zero(v) { f64::from_bits(x.to_bits() & 1 << 63) } else { v }
}

/// the cosine of x degrees: exact where the result is a float, +0 at 90 + 180 n
pub fn cos_deg(x: f64) -> f64 {
    if !finite(x) {
        return NAN;
    }
    let (k, r) = deg_reduce(x);
    let v = match k.rem_euclid(4) {
        0 => cos_r(r),
        1 => num::neg(sin_r(r)),
        2 => num::neg(cos_r(r)),
        _ => sin_r(r),
    };
    if is_zero(v) { 0.0 } else { v }
}

/// the tangent of x degrees: exact at multiples of 45 degrees (0, +-1, +-inf); +inf at
/// 90 + 360 n, -inf at 270 + 360 n, the zeros signed as tanPi of IEEE 754-2019
pub fn tan_deg(x: f64) -> f64 {
    if !finite(x) {
        return NAN;
    }
    let (k, r) = deg_reduce(x);
    let q = k.rem_euclid(4);
    if is_zero(r) {
        return match q {
            1 => f64::INFINITY,
            3 => f64::NEG_INFINITY,
            // a multiple of 180: sin / cos with their signs
            _ => f64::from_bits((x.to_bits() & 1 << 63) ^ if q == 2 { 1 << 63 } else { 0 }),
        };
    }
    let odd = q & 1 != 0;
    if abs_is(r, 45.0) {
        // tan is 1 there, -1 / tan as well
        return if is_neg(r) != odd { -1.0 } else { 1.0 };
    }
    let (y0, y1) = deg_rad(r);
    ktan(y0, y1, if odd { -1 } else { 1 }).0
}

fn finite(x: f64) -> bool {
    x.to_bits() & 0x7FF0_0000_0000_0000 != 0x7FF0_0000_0000_0000
}
