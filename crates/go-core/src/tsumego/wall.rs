use super::{TsumegoKoThreat, TsumegoOutput};
use crate::{BoardSize, Color, Point};

pub(super) struct WallBoard<'a> {
    size: BoardSize,
    result: &'a mut TsumegoOutput,
}

/// Transform one of Java's four area cases to a left-facing problem. Keep
/// original coordinates for parity and iteration: Java has observable ordering.
#[derive(Clone, Copy)]
struct Frame {
    direction: usize,
    width: i64,
    height: i64,
    u_size: i64,
    v_size: i64,
    left: i64,
    right: i64,
    top: i64,
    bottom: i64,
}

impl Frame {
    fn point(self, x: i64, y: i64) -> (i64, i64) {
        match self.direction {
            0 => (x, y),
            1 => (self.width - 1 - x, y),
            2 => (y, x),
            _ => (self.height - 1 - y, x),
        }
    }
}

impl<'a> WallBoard<'a> {
    pub(super) fn new(size: BoardSize, result: &'a mut TsumegoOutput) -> Self {
        Self { size, result }
    }

    fn points(&mut self, mut visit: impl FnMut(&mut Self, i64, i64)) {
        for x in 0..i64::from(self.size.width) {
            for y in 0..i64::from(self.size.height) {
                visit(self, x, y);
            }
        }
    }

    fn put(&mut self, x: i64, y: i64, color: Color) {
        let index = y as usize * usize::from(self.size.width) + x as usize;
        if self.result.stones[index].is_none() {
            self.result.stones[index] = Some(color);
            self.result.added_stones.push((
                color,
                Point {
                    x: x as u8,
                    y: y as u8,
                },
            ));
        }
    }

    fn replace(&mut self, x: i64, y: i64, color: Option<Color>) {
        let index = y as usize * usize::from(self.size.width) + x as usize;
        if color.is_none() && self.result.stones[index].is_none() {
            return;
        }
        self.result.stones[index] = color;
        let point = Point {
            x: x as u8,
            y: y as u8,
        };
        if let Some(index) = self
            .result
            .added_stones
            .iter()
            .position(|(_, p)| *p == point)
        {
            self.result.added_stones.remove(index);
        }
        if let Some(color) = color {
            self.result.added_stones.push((color, point));
        }
    }

    pub(super) fn add_wall(&mut self, left: i64, right: i64, top: i64, bottom: i64) {
        let side = self.result.attacker;
        self.points(|board, x, y| {
            if ((x == left || x == right) && y >= top && y <= bottom)
                || ((y == top || y == bottom) && x >= left && x <= right)
            {
                board.put(x, y, side);
            }
        });
    }

    pub(super) fn fill(
        &mut self,
        left: i64,
        right: i64,
        top: i64,
        bottom: i64,
        ko: TsumegoKoThreat,
    ) {
        let width = i64::from(self.size.width);
        let height = i64::from(self.size.height);
        let areas = [
            (right + 1) * height,
            (width - left) * height,
            (bottom + 1) * width,
            (height - top) * width,
        ];
        let mut direction = 0;
        for i in 1..4 {
            if areas[i] < areas[direction] {
                direction = i;
            }
        }
        let (u_size, v_size, l, r, t, b) = match direction {
            0 => (width, height, left, right, top, bottom),
            1 => (
                width,
                height,
                width - 1 - right,
                width - 1 - left,
                top,
                bottom,
            ),
            2 => (height, width, top, bottom, left, right),
            _ => (
                height,
                width,
                height - 1 - bottom,
                height - 1 - top,
                left,
                right,
            ),
        };
        let frame = Frame {
            direction,
            width,
            height,
            u_size,
            v_size,
            left: l,
            right: r,
            top: t,
            bottom: b,
        };
        let half = (width * height + 1) / 2;
        let with_komi = half + 4;
        let defender_ko = matches!(ko, TsumegoKoThreat::Both | TsumegoKoThreat::Defender);
        let attacker_ko = matches!(ko, TsumegoKoThreat::Both | TsumegoKoThreat::Attacker);
        let (mut komi, no_defender_ko, no_attacker_ko) = if areas[direction] <= with_komi {
            self.half_board(frame, with_komi, defender_ko, attacker_ko)
        } else {
            self.corner_area(frame, half, defender_ko, attacker_ko)
        };
        if self.result.attacker == Color::White {
            komi = -komi;
        }
        self.result.komi = Some(komi.clamp(-150.0, 150.0));
        if no_attacker_ko {
            self.result.ko_warnings.push(self.result.attacker);
        }
        if no_defender_ko {
            self.result
                .ko_warnings
                .push(self.result.attacker.opponent());
        }
    }

    fn half_board(
        &mut self,
        f: Frame,
        half: i64,
        defender_ko: bool,
        attacker_ko: bool,
    ) -> (f64, bool, bool) {
        let side = self.result.attacker;
        let other = side.opponent();
        // Preserve Java's right/bottom index formula and height multiplier in
        // top/bottom remainder calculations, including rectangular boards.
        let boundary = if f.direction == 0 || f.direction == 2 {
            half / f.v_size - 1
        } else {
            f.u_size - 2 - half / f.v_size
        };
        let remain = half - (boundary + 1) * if f.direction >= 2 { f.u_size } else { f.v_size };
        self.points(|board, x, y| {
            let (u, v) = f.point(x, y);
            let even = (x + y) % 2 == 0;
            if u < boundary && (u < f.left || u > f.right || v < f.top || v > f.bottom) && even {
                board.put(x, y, side);
            }
            if u == boundary {
                board.put(x, y, side);
            }
            if remain > 0 {
                if u == boundary + 1 {
                    board.put(x, y, if v < remain { side } else { other });
                }
                if u == boundary + 2 {
                    board.put(x, y, other);
                }
                let beyond = if f.direction == 1 {
                    u > boundary
                } else {
                    u > boundary + 2
                };
                if beyond && even {
                    board.put(x, y, other);
                }
            } else {
                if u == boundary + 1 {
                    board.put(x, y, other);
                }
                // Java case 2 uses x > rightSideIndex - 1 in this branch.
                let beyond = if f.direction == 1 {
                    u < boundary + 1
                } else {
                    u > boundary + 1
                };
                if beyond && even {
                    board.put(x, y, other);
                }
            }
        });
        let mut no_defender = false;
        if defender_ko {
            if boundary < 4 {
                no_defender = true;
            } else if f.top >= 2 {
                self.ko_shape(f, false, false, side);
            } else if f.v_size - f.bottom > 2 {
                self.ko_shape(f, false, true, side);
            } else {
                no_defender = true;
            }
        }
        let mut no_attacker = false;
        if attacker_ko {
            let room = f.u_size - boundary - 2;
            if room < 5 {
                no_attacker = true;
            } else {
                self.ko_shape(f, true, false, other);
            }
        }
        (7.5, no_defender, no_attacker)
    }

    fn corner_area(
        &mut self,
        f: Frame,
        half: i64,
        defender_ko: bool,
        attacker_ko: bool,
    ) -> (f64, bool, bool) {
        let side = self.result.attacker;
        let other = side.opponent();
        let first_area = f.u_size.min(f.right + 1) * f.v_size.min(f.bottom + 1);
        let second_area = f.u_size.min(f.right + 1) * (f.v_size - 0.max(f.top));
        let first = (first_area - half).abs() < (second_area - half).abs();
        self.points(|board, x, y| {
            let (u, v) = f.point(x, y);
            let even = (x + y) % 2 == 0;
            let (outside, boundary, beyond) = if first {
                (
                    v < f.top || (u < f.left && v <= f.bottom),
                    f.bottom,
                    v > f.bottom + 1,
                )
            } else {
                (
                    v > f.bottom || (u < f.left && v >= f.top),
                    f.top,
                    v < f.top - 1,
                )
            };
            if u < f.right && outside && (v == boundary || even) {
                board.put(x, y, side);
            }
            if u == f.right && if first { v < f.top } else { v > f.bottom } {
                board.put(x, y, side);
            }
            if u <= f.right {
                if v == boundary + if first { 1 } else { -1 } {
                    board.put(x, y, other);
                }
                if beyond && even {
                    board.put(x, y, other);
                }
            }
            if u == f.right + 1 {
                board.put(x, y, other);
            }
            if u > f.right + 1 && even {
                board.put(x, y, other);
            }
        });
        let mut no_defender = false;
        if defender_ko {
            // Java bottom branch has an extra early test against the original
            // bottomIndex; keep its warning even when the shape later fits.
            no_defender = f.direction == 3 && f.height - 1 - f.left < 4;
            if f.right < 4 {
                no_defender = true;
            } else if first && f.top >= 2 {
                self.ko_shape(f, false, false, side);
            } else if !first && f.v_size - f.bottom > 2 {
                self.ko_shape(f, false, true, side);
            } else {
                no_defender = true;
            }
        }
        let mut no_attacker = false;
        if attacker_ko {
            if f.u_size - f.right - 1 < 5 {
                no_attacker = true;
            } else {
                self.ko_shape(f, true, false, other);
            }
        }
        (
            2.0 * ((if first { first_area } else { second_area }) - half) as f64,
            no_defender,
            no_attacker,
        )
    }

    fn ko_shape(&mut self, f: Frame, far: bool, bottom: bool, base: Color) {
        self.points(|board, x, y| {
            let (mut u, mut v) = f.point(x, y);
            if far {
                u = f.u_size - 1 - u;
            }
            if bottom {
                v = f.v_size - 1 - v;
            }
            if (v == 0 && (u == 0 || u == 3)) || (v == 1 && u == 3) {
                board.replace(x, y, None);
            }
            if v == 0 && u == 1 {
                board.replace(x, y, Some(base.opponent()));
            }
            if v == 0 && u == 2 {
                board.replace(x, y, Some(base));
            }
            if v == 1 && (0..=2).contains(&u) {
                board.replace(x, y, Some(base.opponent()));
            }
        });
    }
}
