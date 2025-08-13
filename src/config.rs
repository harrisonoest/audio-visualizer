/// Configuration for the audio visualizer
#[derive(Debug, Clone)]
pub struct Config {
    /// Number of frequency bars to display
    pub bar_count: usize,
    /// Color scheme for the bars
    pub color_scheme: ColorScheme,
    /// Refresh rate in milliseconds
    pub refresh_rate: u64,
    /// Sensitivity/gain for the visualizer
    pub sensitivity: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bar_count: 32,
            color_scheme: ColorScheme::Rainbow,
            refresh_rate: 16, // ~60 FPS
            sensitivity: 1.0,
        }
    }
}

impl Config {
    /// Increase bar count
    pub fn increase_bar_count(&mut self) {
        if self.bar_count < 128 {
            self.bar_count = (self.bar_count + 8).min(128);
        }
    }

    /// Decrease bar count
    pub fn decrease_bar_count(&mut self) {
        if self.bar_count > 8 {
            self.bar_count = (self.bar_count - 8).max(8);
        }
    }

    /// Increase refresh rate (decrease delay)
    pub fn increase_refresh_rate(&mut self) {
        if self.refresh_rate > 8 {
            self.refresh_rate = (self.refresh_rate - 4).max(8);
        }
    }

    /// Decrease refresh rate (increase delay)
    pub fn decrease_refresh_rate(&mut self) {
        if self.refresh_rate < 100 {
            self.refresh_rate = (self.refresh_rate + 4).min(100);
        }
    }

    /// Cycle to next color scheme
    pub fn next_color_scheme(&mut self) {
        self.color_scheme = match self.color_scheme {
            ColorScheme::Rainbow => ColorScheme::Blue,
            ColorScheme::Blue => ColorScheme::Green,
            ColorScheme::Green => ColorScheme::Red,
            ColorScheme::Red => ColorScheme::Purple,
            ColorScheme::Purple => ColorScheme::Cyan,
            ColorScheme::Cyan => ColorScheme::Yellow,
            ColorScheme::Yellow => ColorScheme::Rainbow,
        };
    }

    /// Increase sensitivity
    pub fn increase_sensitivity(&mut self) {
        self.sensitivity = (self.sensitivity * 1.2).min(10.0);
    }

    /// Decrease sensitivity
    pub fn decrease_sensitivity(&mut self) {
        self.sensitivity = (self.sensitivity / 1.2).max(0.1);
    }
}

/// Available color schemes for the visualizer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Rainbow,
    Blue,
    Green,
    Red,
    Purple,
    Cyan,
    Yellow,
}

impl ColorScheme {
    /// Get the name of the color scheme for display
    pub fn name(self) -> &'static str {
        match self {
            ColorScheme::Rainbow => "Rainbow",
            ColorScheme::Blue => "Blue",
            ColorScheme::Green => "Green",
            ColorScheme::Red => "Red",
            ColorScheme::Purple => "Purple",
            ColorScheme::Cyan => "Cyan",
            ColorScheme::Yellow => "Yellow",
        }
    }
}
