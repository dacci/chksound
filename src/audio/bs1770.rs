#![allow(unused)]

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::{Add, AddAssign, Div, Mul, Sub};

#[derive(Debug, Copy, Clone)]
pub struct Power(f64);

impl Power {
    pub const MIN: Power = Power(1.1724653045822964e-7);
    pub const MAX: Power = Power(3.7076608400031104);

    fn gate(self, gate: f64) -> Self {
        Self(self.0 * 10.0_f64.powf(0.1 * gate))
    }
}

impl From<Power> for f64 {
    fn from(p: Power) -> Self {
        p.0
    }
}

impl From<Loudness> for Power {
    fn from(l: Loudness) -> Self {
        Self(10.0_f64.powf(0.1 * (0.691 + l.0)))
    }
}

impl PartialEq for Power {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() < f64::EPSILON
    }
}

impl Eq for Power {}

impl PartialOrd for Power {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Power {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0 < other.0 {
            Ordering::Less
        } else if self.0 > other.0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl Add<Self> for Power {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Power {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

impl Sub<Self> for Power {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f64> for Power {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Mul<usize> for Power {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self::Output {
        self.mul(rhs as f64)
    }
}

impl Mul<Self> for Power {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        self.mul(rhs.0)
    }
}

impl Div<usize> for Power {
    type Output = Self;

    fn div(self, rhs: usize) -> Self::Output {
        Self(self.0 / rhs as f64)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Loudness(f64);

impl Loudness {
    pub const MIN: Loudness = Loudness(-70.0);
    pub const MAX: Loudness = Loudness(5.0);

    pub fn to_gain(self) -> f64 {
        -18.0 - self.0
    }
}

impl From<Loudness> for f64 {
    fn from(l: Loudness) -> Self {
        l.0
    }
}

impl From<Power> for Loudness {
    fn from(p: Power) -> Self {
        Self(-0.691 + 10.0 * p.0.log10())
    }
}

impl PartialEq for Loudness {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() < f64::EPSILON
    }
}

impl Eq for Loudness {}

impl PartialOrd for Loudness {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Loudness {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0 < other.0 {
            Ordering::Less
        } else if self.0 > other.0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl Add<f64> for Loudness {
    type Output = Self;

    fn add(self, rhs: f64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<Self> for Loudness {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

struct Biquad {
    sample_rate: u32,
    a1: f64,
    a2: f64,
    b0: f64,
    b1: f64,
    b2: f64,
}

struct BiquadPs {
    k: f64,
    q: f64,
    vb: f64,
    vl: f64,
    vh: f64,
}

impl Biquad {
    fn get_ps(&self) -> BiquadPs {
        let x11 = self.a1 - 2.0;
        let x12 = self.a1;
        let x1 = -self.a1 - 2.0;

        let x21 = self.a2 - 1.0;
        let x22 = self.a2 + 1.0;
        let x2 = -self.a2 + 1.0;

        let dx = x22 * x11 - x12 * x21;
        let k_sq = (x22 * x1 - x12 * x2) / dx;
        let k_by_q = (x11 * x2 - x21 * x1) / dx;
        let a0 = 1.0 + k_by_q + k_sq;

        let k = k_sq.sqrt();

        BiquadPs {
            k,
            q: k / k_by_q,
            vb: 0.5 * a0 * (self.b0 - self.b2) / k_by_q,
            vl: 0.25 * a0 * (self.b0 + self.b1 + self.b2) / k_sq,
            vh: 0.25 * a0 * (self.b0 - self.b1 + self.b2),
        }
    }

    fn re_quantize(mut self, sample_rate: u32) -> Self {
        if self.sample_rate != sample_rate {
            let ps = self.get_ps();
            let k = ((self.sample_rate as f64 / sample_rate as f64) * ps.k.atan()).tan();
            let k_sq = k * k;
            let k_by_q = k / ps.q;
            let a0 = 1.0 + k_by_q + k_sq;

            self.a1 = (2.0 * (k_sq - 1.0)) / a0;
            self.a2 = (1.0 - k_by_q + k_sq) / a0;
            self.b0 = (ps.vh + ps.vb * k_by_q + ps.vl * k_sq) / a0;
            self.b1 = (2.0 * (ps.vl * k_sq - ps.vh)) / a0;
            self.b2 = (ps.vh - ps.vb * k_by_q + ps.vl * k_sq) / a0;
        }

        self
    }

    fn f1_48000() -> Self {
        Self {
            sample_rate: 48000,
            a1: -1.69065929318241,
            a2: 0.73248077421585,
            b0: 1.53512485958697,
            b1: -2.69169618940638,
            b2: 1.19839281085285,
        }
    }

    fn f2_48000() -> Self {
        Self {
            sample_rate: 48000,
            a1: -1.99004745483398,
            a2: 0.99007225036621,
            b0: 1.0,
            b1: -2.0,
            b2: 1.0,
        }
    }
}

struct Bin {
    db: Loudness,
    count: usize,
}

pub struct Stats {
    max_wmsq: Power,

    pass1_wmsq: Power,  // cumulative moving average.
    pass1_count: usize, // number of blocks processed.

    bins: BTreeMap<Power, Bin>,
}

impl Stats {
    const GRAIN: f64 = 100.0;
    const BIN_COUNT: usize = (Self::GRAIN * (Loudness::MAX.0 - Loudness::MIN.0) + 1.0) as usize;

    pub fn new() -> Self {
        let step = 1.0 / Self::GRAIN;
        let mut bins = BTreeMap::new();
        for i in 0..Self::BIN_COUNT {
            let db = Loudness::MIN + i as f64 * step;
            bins.insert(db.into(), Bin { db, count: 0 });
        }

        Stats {
            max_wmsq: Power::MIN,
            pass1_wmsq: Power(0.0),
            pass1_count: 0,
            bins,
        }
    }

    pub fn merge(&mut self, rhs: &Self) {
        if self.max_wmsq < rhs.max_wmsq {
            self.max_wmsq = rhs.max_wmsq;
        }

        let count = self.pass1_count + rhs.pass1_count;
        if 0 < count {
            let q1 = self.pass1_count as f64 / count as f64;
            let q2 = rhs.pass1_count as f64 / count as f64;
            self.pass1_count = count;
            self.pass1_wmsq = self.pass1_wmsq * q1 + rhs.pass1_wmsq * q2;

            for ((_, l), (_, r)) in self.bins.iter_mut().zip(&rhs.bins) {
                l.count += r.count;
            }
        }
    }

    fn add_sqs(&mut self, wmsq: Power) {
        if self.max_wmsq < wmsq {
            self.max_wmsq = wmsq;
        }

        if let Some((_, bin)) = self.bins.range_mut(..=wmsq).last() {
            self.pass1_count += 1;
            self.pass1_wmsq += (wmsq - self.pass1_wmsq) / self.pass1_count;
            bin.count += 1;
        }
    }

    pub fn get_max(&self) -> Loudness {
        self.max_wmsq.into()
    }

    pub fn get_mean(&self, gate: f64) -> Loudness {
        let threshold = self.pass1_wmsq.gate(gate);
        let (wmsq, count) = self
            .bins
            .iter()
            .filter(|(x, bin)| 0 < bin.count && threshold < **x)
            .fold((Power(0.0), 0), |a, (x, bin)| {
                (a.0 + *x * bin.count, a.1 + bin.count)
            });

        if 0 < count {
            (wmsq / count).into()
        } else {
            Loudness::MIN
        }
    }

    pub fn get_range(&self, gate: f64, lower: f64, upper: f64) -> Loudness {
        let threshold = self.pass1_wmsq.gate(gate);
        let count = self
            .bins
            .iter()
            .filter(|(x, bin)| 0 < bin.count && threshold < **x)
            .map(|(_, bin)| bin.count)
            .sum::<usize>();
        if count == 0 {
            return Loudness(0.0);
        }

        let (lower, upper) = (lower.min(upper).max(0.0), upper.max(lower).min(1.0));
        let lower_count = (count as f64 * lower) as usize;
        let upper_count = (count as f64 * upper) as usize;

        let (_, min, max) = self.bins.iter().filter(|(x, _)| threshold < **x).fold(
            (0, Loudness(0.0), Loudness(0.0)),
            |(prev_count, min, max), (_, bin)| {
                let count = prev_count + bin.count;

                let min = if prev_count < lower_count && lower_count <= count {
                    bin.db
                } else {
                    min
                };

                let max = if prev_count < upper_count && upper_count <= count {
                    bin.db
                } else {
                    max
                };

                (count, min, max)
            },
        );

        max - min
    }
}

// ITU BS.1770 sliding block (aggregator).
struct Block {
    stats: Stats,

    gate: Power,         // ITU BS.1770 silence gate.
    overlap_size: usize, // depends on sample_rate
    scale: f64,          // depends on block size, i.e. on sample_rate

    ring_size: usize,      // number of blocks in ring buffer.
    ring_used: usize,      // number of blocks used in ring buffer.
    ring_count: usize,     // number of samples processed in front block.
    ring_offs: usize,      // offset of front block.
    ring_wmsq: Vec<Power>, // allocated blocks.
}

impl Block {
    fn new(overlap_size: usize, partition: usize) -> Self {
        Self {
            stats: Stats::new(),

            gate: Power::MIN,
            overlap_size,
            scale: 1.0 / (partition * overlap_size) as f64,

            ring_size: partition,
            ring_used: 1,
            ring_count: 0,
            ring_offs: 0,
            ring_wmsq: vec![Power(0.0); partition],
        }
    }

    fn add_sqs(&mut self, wssqs: Power) {
        let wssqs_scaled = wssqs * self.scale;
        for i in 0..self.ring_used {
            self.ring_wmsq[i] += wssqs_scaled;
        }

        self.ring_count += 1;
        if self.ring_count == self.overlap_size {
            let next_offs = if self.ring_offs + 1 < self.ring_size {
                self.ring_offs + 1
            } else {
                0
            };

            if self.ring_used == self.ring_size {
                let prev_wmsq = self.ring_wmsq[next_offs];
                if self.gate < prev_wmsq {
                    self.stats.add_sqs(prev_wmsq);
                }
            }

            self.ring_wmsq[next_offs] = Power(0.0);
            self.ring_count = 0;
            self.ring_offs = next_offs;

            if self.ring_used < self.ring_size {
                self.ring_used += 1;
            }
        }
    }
}

// ITU BS.1770 pre-filter.
pub struct PreFilter {
    block: Vec<Block>,

    sample_rate: u32,
    channels: usize,

    f1: Biquad,
    f2: Biquad,

    ring_offs: isize,
    ring_size: usize,
    ring_buf: Vec<[Power; Self::BUF_SIZE]>,
}

impl PreFilter {
    const BUF_SIZE: usize = 9;
    const MAX_CHANNELS: usize = 5;
    const CHANNEL_WEIGHTS: [f64; Self::MAX_CHANNELS] = [1.0, 1.0, 1.0, 1.41, 1.41];

    pub fn new(sample_rate: u32, channels: usize) -> Self {
        let channels = channels.min(Self::MAX_CHANNELS);
        Self {
            block: Vec::new(),
            sample_rate,
            channels,

            f1: Biquad::f1_48000().re_quantize(sample_rate),
            f2: Biquad::f2_48000().re_quantize(sample_rate),

            ring_offs: 1,
            ring_size: 1,
            ring_buf: vec![[Power(0.0); Self::BUF_SIZE]; channels],
        }
    }

    pub fn add_block(&mut self, length: f64, partition: usize) {
        let overlap_size = (length * self.sample_rate as f64 / partition as f64).round() as usize;
        self.block.push(Block::new(overlap_size, partition));
    }

    pub fn add_sample(&mut self, sample: &[f64]) {
        #[inline]
        fn x_(offs: isize, i: isize) -> usize {
            if offs + i < 0 {
                (PreFilter::BUF_SIZE as isize + offs + i) as usize
            } else {
                (offs + i) as usize
            }
        }
        #[inline]
        fn y_(offs: isize, i: isize) -> usize {
            x_(offs - 6, i)
        }
        #[inline]
        fn z_(offs: isize, i: isize) -> usize {
            x_(offs - 3, i)
        }

        let f1 = &self.f1;
        let f2 = &self.f2;
        let offs = self.ring_offs;

        let mut wssqs = Power(0.0);

        for (ch, sample) in sample.iter().enumerate().take(self.channels) {
            let buf = &mut self.ring_buf[ch];

            buf[x_(offs, 0)] = Power(*sample);
            let x = buf[x_(offs, 0)];

            if 1 < self.ring_size {
                buf[y_(offs, 0)] =
                    x * f1.b0 + buf[x_(offs, -1)] * f1.b1 + buf[x_(offs, -2)] * f1.b2
                        - buf[y_(offs, -1)] * f1.a1
                        - buf[y_(offs, -2)] * f1.a2;
                let y = buf[y_(offs, 0)];

                buf[z_(offs, 0)] =
                    y * f2.b0 + buf[y_(offs, -1)] * f2.b1 + buf[y_(offs, -2)] * f2.b2
                        - buf[z_(offs, -1)] * f2.a1
                        - buf[z_(offs, -2)] * f2.a2;
                let z = buf[z_(offs, 0)];

                wssqs += z * z * Self::CHANNEL_WEIGHTS[ch];
            }
        }

        for block in &mut self.block {
            block.add_sqs(wssqs);
        }

        if self.ring_size < 2 {
            self.ring_size += 1;
        }

        self.ring_offs += 1;
        if self.ring_offs == Self::BUF_SIZE as isize {
            self.ring_offs = 0;
        }
    }

    pub fn flush(mut self) -> Vec<Stats> {
        if 1 < self.ring_size {
            self.add_sample(&[0.0; Self::MAX_CHANNELS]);
        }

        self.block.into_iter().map(|b| b.stats).collect()
    }
}
