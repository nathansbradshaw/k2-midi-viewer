#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cluster {
    Encoder,
    Alpha,
    AlphaLight,
    Nav,
    Arrow,
    Numpad,
}

#[derive(Debug, Clone)]
pub struct Key {
    pub id: KeyId,
    pub label: &'static str,
    pub sublabel: Option<&'static str>,
    pub col: f32,
    pub row: f32,
    pub w: f32,
    pub h: f32,
    pub cluster: Cluster,
    pub is_knob: bool,
}

impl Key {
    pub const fn new(
        id: u32,
        label: &'static str,
        col: f32,
        row: f32,
        cluster: Cluster,
    ) -> Self {
        Key {
            id: KeyId(id),
            label,
            sublabel: None,
            col,
            row,
            w: 1.0,
            h: 1.0,
            cluster,
            is_knob: false,
        }
    }

    pub const fn sub(mut self, s: &'static str) -> Self {
        self.sublabel = Some(s);
        self
    }

    pub const fn size(mut self, w: f32, h: f32) -> Self {
        self.w = w;
        self.h = h;
        self
    }

    pub const fn knob(mut self) -> Self {
        self.is_knob = true;
        self
    }
}
