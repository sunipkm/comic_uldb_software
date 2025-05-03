use num_traits::{Inv, Num, NumAssignOps, NumAssignRef, NumCast, NumOps, ToPrimitive};
use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// A type representing a value with an associated uncertainty.
pub struct Uncertain<T>(pub T, pub T);

impl<T: Num + NumCast + ToPrimitive + NumOps + NumAssignOps + NumAssignRef + Neg + Copy>
    Uncertain<T>
{
    /// Create a new uncertain value.
    pub fn new(value: T, uncertainty: T) -> Self {
        Uncertain(value, uncertainty)
    }

    /// Get the value of the uncertain value.
    pub fn value(&self) -> T {
        self.0
    }

    /// Get the uncertainty of the uncertain value.
    pub fn error(&self) -> T {
        self.1
    }
}

impl<T: Num> From<T> for Uncertain<T> {
    fn from(value: T) -> Self {
        Uncertain(value, T::zero())
    }
}

impl<T: NumCast + Copy> Uncertain<T> {
    /// Cast a known value to an uncertain value.
    pub fn cast_into<U>(&self) -> Uncertain<U>
    where
        U: NumCast + Copy,
    {
        Uncertain(
            NumCast::from(self.0).unwrap(),
            NumCast::from(self.1).unwrap(),
        )
    }
}

impl<T: Num + ToPrimitive + NumOps + NumCast> Add for Uncertain<T> {
    type Output = Uncertain<T>;

    fn add(self, other: Uncertain<T>) -> Uncertain<T> {
        let stdev1: f64 = NumCast::from(self.1).unwrap();
        let stdev2: f64 = NumCast::from(other.1).unwrap();
        let stdev = (stdev1 * stdev1 + stdev2 * stdev2).sqrt();
        Uncertain(self.0 + other.0, NumCast::from(stdev).unwrap())
    }
}

impl<T: Num + NumOps + Sub> Neg for Uncertain<T> {
    type Output = Uncertain<T>;

    fn neg(self) -> Uncertain<T> {
        Uncertain(T::zero() - self.0, self.1)
    }
}

impl<T: Num + ToPrimitive + NumOps + NumCast> Sub for Uncertain<T> {
    type Output = Uncertain<T>;

    fn sub(self, other: Uncertain<T>) -> Uncertain<T> {
        self + (-other)
    }
}

impl<T: Num + ToPrimitive + NumOps + NumCast + Copy> Mul for Uncertain<T> {
    type Output = Uncertain<T>;

    fn mul(self, other: Uncertain<T>) -> Uncertain<T> {
        let v1: f64 = NumCast::from(self.0).unwrap();
        let v2: f64 = NumCast::from(other.0).unwrap();
        let u1: f64 = NumCast::from(self.1).unwrap();
        let u2: f64 = NumCast::from(other.1).unwrap();
        let err = ((u1 / v1) * (u1 / v1) + (u2 / v2) * (u2 / v2)).sqrt();
        Uncertain(self.0 * other.0, NumCast::from(err).unwrap())
    }
}

impl<T: Num + ToPrimitive + NumOps + NumCast + Copy> Inv for Uncertain<T> {
    type Output = Uncertain<T>;

    fn inv(self) -> Uncertain<T> {
        let v: f64 = NumCast::from(self.0).unwrap();
        let u: f64 = NumCast::from(self.1).unwrap();
        let err = (u / (v * v)).abs();
        Uncertain(T::one() / self.0, NumCast::from(err).unwrap())
    }
}

impl<T: Num + ToPrimitive + NumOps + NumCast + Copy> Div for Uncertain<T> {
    type Output = Uncertain<T>;

    fn div(self, other: Uncertain<T>) -> Uncertain<T> {
        let v1: f64 = NumCast::from(self.0).unwrap();
        let v2: f64 = NumCast::from(other.0).unwrap();
        let u1: f64 = NumCast::from(self.1).unwrap();
        let u2: f64 = NumCast::from(other.1).unwrap();
        let err = ((u1 / v1) * (u1 / v1) + (u2 / v2) * (u2 / v2)).sqrt();
        Uncertain(self.0 / other.0, NumCast::from(err).unwrap())
    }
}
