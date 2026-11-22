//! Where the painting sits in the window: a 2D affine from canvas to window
//! pixels, as in the web app's CanvasView (`x' = a x + b y + c, y' = d x + e y + f`).

#[derive(Clone, Copy, Debug)]
pub struct Mat(pub [f32; 6]);

impl Default for Mat {
    fn default() -> Self {
        Mat([1.0, 0.0, 0.0, 0.0, 1.0, 0.0])
    }
}

impl Mat {
    /// self = t ∘ self
    pub fn post(&mut self, t: [f32; 6]) {
        let [a, b, c, d, e, f] = self.0;
        let [aa, bb, cc, dd, ee, ff] = t;
        self.0 = [aa * a + bb * d, aa * b + bb * e, aa * c + bb * f + cc, dd * a + ee * d, dd * b + ee * e, dd * c + ee * f + ff];
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.post([1.0, 0.0, x, 0.0, 1.0, y]);
    }
    pub fn rotate(&mut self, deg: f32) {
        let (s, c) = deg.to_radians().sin_cos();
        self.post([c, -s, 0.0, s, c, 0.0]);
    }
    /// Scales by `k` about window point (x, y).
    pub fn zoom_at(&mut self, k: f32, x: f32, y: f32) {
        self.translate(-x, -y);
        self.post([k, 0.0, 0.0, 0.0, k, 0.0]);
        self.translate(x, y);
    }
    pub fn invert(&self) -> Mat {
        let [a, b, c, d, e, f] = self.0;
        let det = a * e - b * d;
        let det = if det.abs() < 1e-12 { 1e-12 } else { det };
        Mat([e / det, -b / det, (b * f - c * e) / det, -d / det, a / det, (c * d - a * f) / det])
    }
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + b * y + c, d * x + e * y + f)
    }
    pub fn scale(&self) -> f32 {
        let [a, b, _, d, e, _] = self.0;
        (a * e - b * d).abs().sqrt()
    }
}

/// The painting's centre at (cx, cy), `s` window pixels per canvas pixel, turned `deg`.
pub fn place(cw: f32, ch: f32, cx: f32, cy: f32, s: f32, deg: f32) -> Mat {
    let mut m = Mat([s, 0.0, -cw * s / 2.0, 0.0, s, -ch * s / 2.0]);
    m.rotate(deg);
    m.translate(cx, cy);
    m
}
