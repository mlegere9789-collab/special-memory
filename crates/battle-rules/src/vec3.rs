//! 3D fixed-point vectors for projectile flight (SPEC-005 section 2).

use core::ops::{Add, Neg, Sub};
use core_math::Fx;

/// A 3D vector of fixed-point numbers. `x` and `y` are horizontal, `z` is height; metres or
/// metres per second depending on use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Vec3Fx {
    /// First horizontal component.
    pub x: Fx,
    /// Second horizontal component.
    pub y: Fx,
    /// Height component.
    pub z: Fx,
}

impl Vec3Fx {
    /// The zero vector.
    pub const ZERO: Vec3Fx = Vec3Fx {
        x: Fx::ZERO,
        y: Fx::ZERO,
        z: Fx::ZERO,
    };

    /// Builds a vector from its components.
    pub const fn new(x: Fx, y: Fx, z: Fx) -> Vec3Fx {
        Vec3Fx { x, y, z }
    }

    /// Every component times `k` (each product rounded toward negative infinity, saturating).
    pub fn scale(self, k: Fx) -> Vec3Fx {
        Vec3Fx {
            x: self.x * k,
            y: self.y * k,
            z: self.z * k,
        }
    }

    /// Dot product, accumulated in `i128` and shifted once, so only the final value rounds
    /// (toward negative infinity) and saturates.
    pub fn dot(self, other: Vec3Fx) -> Fx {
        let sum = i128::from(self.x.raw()) * i128::from(other.x.raw())
            + i128::from(self.y.raw()) * i128::from(other.y.raw())
            + i128::from(self.z.raw()) * i128::from(other.z.raw());
        let v = sum >> Fx::FRAC_BITS;
        Fx::from_raw(i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX }))
    }

    /// Squared length, saturating at [`Fx::MAX`] beyond about 46,000 per component.
    pub fn length_sq(self) -> Fx {
        self.dot(self)
    }

    /// Length: the integer square root of [`Vec3Fx::length_sq`].
    pub fn length(self) -> Fx {
        self.length_sq().sqrt()
    }
}

impl Add for Vec3Fx {
    type Output = Vec3Fx;
    fn add(self, rhs: Vec3Fx) -> Vec3Fx {
        Vec3Fx::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3Fx {
    type Output = Vec3Fx;
    fn sub(self, rhs: Vec3Fx) -> Vec3Fx {
        Vec3Fx::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Neg for Vec3Fx {
    type Output = Vec3Fx;
    fn neg(self) -> Vec3Fx {
        Vec3Fx::new(-self.x, -self.y, -self.z)
    }
}
