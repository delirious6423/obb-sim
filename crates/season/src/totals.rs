use sim_core::{BatLine, PitchLine};

/// A batter's accumulated line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatTotals {
    pub g: u32,
    pub pa: u32,
    pub ab: u32,
    pub r: u32,
    pub h: u32,
    pub doubles: u32,
    pub triples: u32,
    pub hr: u32,
    pub rbi: u32,
    pub bb: u32,
    pub so: u32,
    pub hbp: u32,
    pub sf: u32,
    pub sb: u32,
    pub cs: u32,
}

impl BatTotals {
    pub(crate) fn add(&mut self, b: &BatLine) {
        self.g += 1;
        self.pa += u32::from(b.pa);
        self.ab += u32::from(b.ab);
        self.r += u32::from(b.r);
        self.h += u32::from(b.h);
        self.doubles += u32::from(b.doubles);
        self.triples += u32::from(b.triples);
        self.hr += u32::from(b.hr);
        self.rbi += u32::from(b.rbi);
        self.bb += u32::from(b.bb);
        self.so += u32::from(b.so);
        self.hbp += u32::from(b.hbp);
        self.sf += u32::from(b.sf);
        self.sb += u32::from(b.sb);
        self.cs += u32::from(b.cs);
    }

    #[must_use]
    pub fn avg(&self) -> f64 {
        ratio(self.h, self.ab)
    }

    #[must_use]
    pub fn obp(&self) -> f64 {
        ratio(
            self.h + self.bb + self.hbp,
            self.ab + self.bb + self.hbp + self.sf,
        )
    }

    #[must_use]
    pub fn slg(&self) -> f64 {
        let singles = self.h - self.doubles - self.triples - self.hr;
        ratio(
            singles + 2 * self.doubles + 3 * self.triples + 4 * self.hr,
            self.ab,
        )
    }
}

/// A pitcher's accumulated line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PitchTotals {
    pub g: u32,
    pub outs: u32,
    pub h: u32,
    pub r: u32,
    pub er: u32,
    pub bb: u32,
    pub so: u32,
    pub hr: u32,
    pub hbp: u32,
    pub bf: u32,
    pub pitches: u32,
}

impl PitchTotals {
    pub(crate) fn add(&mut self, p: &PitchLine) {
        self.g += 1;
        self.outs += u32::from(p.outs);
        self.h += u32::from(p.h);
        self.r += u32::from(p.r);
        self.er += u32::from(p.er);
        self.bb += u32::from(p.bb);
        self.so += u32::from(p.so);
        self.hr += u32::from(p.hr);
        self.hbp += u32::from(p.hbp);
        self.bf += u32::from(p.bf);
        self.pitches += u32::from(p.pitches);
    }

    #[must_use]
    pub fn era(&self) -> f64 {
        27.0 * ratio(self.er, self.outs)
    }
}

pub(crate) fn ratio(n: u32, d: u32) -> f64 {
    if d == 0 {
        0.0
    } else {
        f64::from(n) / f64::from(d)
    }
}
