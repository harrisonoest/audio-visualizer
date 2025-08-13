use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{BarChart, Block, Borders, Clear, Paragraph},
};
use std::time::{Duration, Instant};
use tokio::time;

mod audio;
mod config;

use audio::{AudioProcessor, get_input_devices};
use config::Config;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    // Install a custom panic hook to handle ALSA timestamp panics gracefully
    // This prevents the application from crashing due to cpal/ALSA issues
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let message = panic_info.to_string();

        // Check if this is an ALSA timestamp panic
        if message.contains("htstamp") || message.contains("trigger_htstamp") {
            eprintln!(
                "Warning: Audio backend encountered a timestamp issue. This is a known ALSA issue."
            );
            eprintln!(
                "The audio stream may have stopped, but the application will continue running."
            );
            eprintln!("Try switching audio sources with 's' key if audio stops working.");
            // Don't call the original hook to avoid terminating the process
            return;
        }

        // For other panics, use the original handler
        original_hook(panic_info);
    }));
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new()?.run(terminal).await;
    ratatui::restore();
    result
}

/// The main application which holds the state and logic of the application.
pub struct App {
    /// Is the application running?
    running: bool,
    /// Audio processor for capturing and analyzing audio
    audio_processor: Option<AudioProcessor>,
    /// Current configuration
    config: Config,
    /// Latest FFT data for visualization
    fft_data: Vec<f32>,
    /// Available audio input devices
    available_devices: Vec<(String, cpal::Device)>,
    /// Current device index
    current_device_index: usize,
    /// Last render time for FPS limiting
    last_render: Instant,
    /// Show help overlay
    show_help: bool,
}

impl App {
    /// Construct a new instance of [`App`].
    pub fn new() -> Result<Self> {
        let available_devices = get_input_devices().unwrap_or_default();
        let current_device_index = 0;

        // Try to initialize audio processor with default device
        let audio_processor = if !available_devices.is_empty() {
            match AudioProcessor::new(Some(available_devices[current_device_index].1.clone())) {
                Ok(processor) => Some(processor),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to initialize audio with selected device: {e}. Trying default device."
                    );
                    AudioProcessor::new(None).ok()
                }
            }
        } else {
            match AudioProcessor::new(None) {
                Ok(processor) => Some(processor),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to initialize audio: {e}. Visualizer will run without audio input."
                    );
                    None
                }
            }
        };

        Ok(Self {
            running: false,
            audio_processor,
            config: Config::default(),
            fft_data: vec![0.0; 512], // Initialize with zeros
            available_devices,
            current_device_index,
            last_render: Instant::now(),
            show_help: false,
        })
    }

    /// Run the application's main loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;
        let mut interval = time::interval(Duration::from_millis(self.config.refresh_rate));

        while self.running {
            interval.tick().await;

            // Update FFT data if audio processor is available
            if let Some(ref mut processor) = self.audio_processor {
                if let Some(data) = processor.get_fft_data().await {
                    self.fft_data = data;
                }
            }

            // Only render if enough time has passed for the configured refresh rate
            if self.last_render.elapsed() >= Duration::from_millis(self.config.refresh_rate) {
                terminal.draw(|frame| self.render(frame))?;
                self.last_render = Instant::now();
            }

            // Handle events with timeout to avoid blocking
            if crossterm::event::poll(Duration::from_millis(1))? {
                self.handle_crossterm_events()?;
            }
        }
        Ok(())
    }

    /// Renders the user interface.
    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title bar
                Constraint::Min(0),    // Main visualization
                Constraint::Length(3), // Status bar
            ])
            .split(frame.area());

        // Render title
        self.render_title(frame, chunks[0]);

        // Render main visualization
        self.render_visualizer(frame, chunks[1]);

        // Render status bar
        self.render_status(frame, chunks[2]);

        // Render help overlay if requested
        if self.show_help {
            self.render_help_overlay(frame);
        }
    }

    /// Render the title bar
    fn render_title(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let title = Line::from(vec![
            Span::styled(
                "Audio Visualizer ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("- Press 'h' for help", Style::default().fg(Color::Gray)),
        ]);

        let title_widget = Paragraph::new(title)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        frame.render_widget(title_widget, area);
    }

    /// Render the main audio visualizer
    fn render_visualizer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Prepare bar data for visualization
        let bar_data = self.prepare_bar_data();

        // Create bar chart with color based on current scheme
        let bar_color = self.get_bar_color();
        let bar_chart = BarChart::default()
            .block(Block::default().borders(Borders::ALL).title(format!(
                    "Frequency Spectrum ({}Hz) - {} bars - {} scheme", 
                    self.audio_processor.as_ref().map(|p| p.sample_rate()).unwrap_or(44100),
                    self.config.bar_count,
                    self.config.color_scheme.name()
                )))
            .data(&bar_data)
            .bar_width(std::cmp::max(
                1u16,
                ((area.width as usize - 2) / self.config.bar_count) as u16,
            ))
            .bar_gap(0)
            .bar_style(Style::default().fg(bar_color))
            .value_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(bar_chart, area);
    }

    /// Get the primary color for bars based on the color scheme
    fn get_bar_color(&self) -> Color {
        use config::ColorScheme;
        match self.config.color_scheme {
            ColorScheme::Rainbow => Color::Magenta, // Use magenta as base for rainbow
            ColorScheme::Blue => Color::Blue,
            ColorScheme::Green => Color::Green,
            ColorScheme::Red => Color::Red,
            ColorScheme::Purple => Color::Magenta,
            ColorScheme::Cyan => Color::Cyan,
            ColorScheme::Yellow => Color::Yellow,
        }
    }

    /// Render the status bar
    fn render_status(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let device_name = if !self.available_devices.is_empty()
            && self.current_device_index < self.available_devices.len()
        {
            &self.available_devices[self.current_device_index].0
        } else {
            "No Device"
        };

        let status_text = format!(
            "Device: {} | Bars: {} | FPS: {} | Sensitivity: {:.1} | Press 'q' to quit, 'h' for help",
            device_name,
            self.config.bar_count,
            1000 / self.config.refresh_rate,
            self.config.sensitivity
        );

        let status_widget = Paragraph::new(status_text)
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Green))
            .alignment(Alignment::Center);

        frame.render_widget(status_widget, area);
    }

    /// Render help overlay
    fn render_help_overlay(&self, frame: &mut Frame) {
        let area = frame.area();
        let popup_area = ratatui::layout::Rect {
            x: area.width / 4,
            y: area.height / 4,
            width: area.width / 2,
            height: area.height / 2,
        };

        let help_text = "\nKeyboard Controls:\n\n\
            h - Toggle this help\n\
            q, Esc, Ctrl+C - Quit\n\
            c - Change color scheme\n\
            + / = - Increase bars\n\
            - / _ - Decrease bars\n\
            r - Increase refresh rate\n\
            R - Decrease refresh rate\n\
            s - Switch audio source\n\
            [ - Decrease sensitivity\n\
            ] - Increase sensitivity\n\n\
            Press any key to close help";

        let help_widget = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .style(Style::default().bg(Color::Black).fg(Color::White)),
            )
            .style(Style::default().bg(Color::Black).fg(Color::White));

        frame.render_widget(Clear, popup_area);
        frame.render_widget(help_widget, popup_area);
    }

    /// Prepare bar data for the bar chart widget with colored bars
    fn prepare_bar_data(&self) -> Vec<(&str, u64)> {
        let mut bar_data = Vec::with_capacity(self.config.bar_count);

        // Calculate how many FFT bins to group per bar
        let bins_per_bar = std::cmp::max(1, self.fft_data.len() / self.config.bar_count);

        for i in 0..self.config.bar_count {
            let start_idx = i * bins_per_bar;
            let end_idx = std::cmp::min(start_idx + bins_per_bar, self.fft_data.len());

            // Average the magnitude values in this frequency range
            let avg_magnitude = if start_idx < self.fft_data.len() {
                self.fft_data[start_idx..end_idx].iter().sum::<f32>() / (end_idx - start_idx) as f32
            } else {
                0.0
            };

            // Apply logarithmic scaling for better visual representation
            let log_magnitude = if avg_magnitude > 0.0 {
                (avg_magnitude.ln() + 10.0).max(0.0)
            } else {
                0.0
            };

            // Scale by sensitivity and convert to bar height (0-100)
            let height = ((log_magnitude * self.config.sensitivity * 10.0) as u64).min(100);

            // Use empty string for labels to save space
            bar_data.push(("", height));
        }

        bar_data
    }

    /// Reads the crossterm events and updates the state of [`App`].
    ///
    /// If your application needs to perform work in between handling events, you can use the
    /// [`event::poll`] function to check if there are any events available with a timeout.
    fn handle_crossterm_events(&mut self) -> Result<()> {
        match event::read()? {
            // it's important to check KeyEventKind::Press to avoid handling key release events
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn on_key_event(&mut self, key: KeyEvent) {
        // Close help if it's open
        if self.show_help {
            self.show_help = false;
            return;
        }

        match (key.modifiers, key.code) {
            // Quit commands
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),

            // Help
            (_, KeyCode::Char('h') | KeyCode::Char('H')) => {
                self.show_help = true;
            }

            // Color scheme cycling
            (_, KeyCode::Char('c') | KeyCode::Char('C')) => {
                self.config.next_color_scheme();
            }

            // Bar count adjustment
            (_, KeyCode::Char('+') | KeyCode::Char('=')) => {
                self.config.increase_bar_count();
            }
            (_, KeyCode::Char('-') | KeyCode::Char('_')) => {
                self.config.decrease_bar_count();
            }

            // Refresh rate adjustment
            (_, KeyCode::Char('r')) => {
                self.config.increase_refresh_rate();
            }
            (_, KeyCode::Char('R')) => {
                self.config.decrease_refresh_rate();
            }

            // Sensitivity adjustment
            (_, KeyCode::Char('[')) => {
                self.config.decrease_sensitivity();
            }
            (_, KeyCode::Char(']')) => {
                self.config.increase_sensitivity();
            }

            // Audio source switching
            (_, KeyCode::Char('s') | KeyCode::Char('S')) => {
                self.switch_audio_source();
            }

            _ => {}
        }
    }

    /// Switch to the next available audio source
    fn switch_audio_source(&mut self) {
        if self.available_devices.is_empty() {
            eprintln!("No audio devices available to switch to.");
            return;
        }

        let old_device_index = self.current_device_index;
        self.current_device_index = (self.current_device_index + 1) % self.available_devices.len();

        // Try to create new audio processor with selected device
        let device_clone = self.available_devices[self.current_device_index].1.clone();
        let device_name = self.available_devices[self.current_device_index].0.clone();

        // Drop the old audio processor first to ensure cleanup
        self.audio_processor = None;

        match AudioProcessor::new(Some(device_clone)) {
            Ok(new_processor) => {
                self.audio_processor = Some(new_processor);
                eprintln!("Switched to audio device: {device_name}");
            }
            Err(e) => {
                eprintln!(
                    "Failed to switch to device '{device_name}': {e}. Trying to restart with previous device."
                );
                self.current_device_index = old_device_index;

                // Try to recreate the old device
                if let Some((_, old_device)) = self.available_devices.get(old_device_index).cloned()
                {
                    match AudioProcessor::new(Some(old_device)) {
                        Ok(processor) => {
                            self.audio_processor = Some(processor);
                            eprintln!("Restored previous audio device.");
                        }
                        Err(_) => {
                            eprintln!(
                                "Could not restore previous audio device. Audio may not be available."
                            );
                        }
                    }
                }
            }
        }
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}
