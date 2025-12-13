//! Animated splash screen with particle assembly effect
//!
//! Shows the Infraware logo assembled from scattered particles

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::{backend::CrosstermBackend, layout::Rect, style::Color, Frame, Terminal};
use std::{
    io::Stdout,
    time::{Duration, Instant},
};

/// ASCII art representation of the Infraware logo - each '@' becomes a particle
/// Scaled down version (~25x14 characters) for better terminal fit
const LOGO_ART: &[&str] = &[
    "        @@              @",
    "      @@@@@@          @@@",
    "    @@@@@@@@@@      @@@@@",
    "  @@@@@@@@@@@@@@  @@@@@@@",
    " @@@@@@@@@@@@@@@@@@@@@@@@",
    "@@@@@@@@@@@@@@@@@@@@@@@@@",
    "@@@@@@@@@@@@@@@@@@@@@@@@@",
    "@@@@@@@@@@@@@@@@@@@@@@@@@",
    "@@@@@@@@@@@@@@@@@@@@@@@@@",
    "@@@@@@@@  @@@@@@@@@@@@@@ ",
    "@@@@@@      @@@@@@@@@@   ",
    "@@@@          @@@@@@     ",
    "@@              @@       ",
    "@                        ",
];

/// Height of the ASCII art
const ART_HEIGHT: usize = 14;

/// Animation phases
#[derive(Debug, Clone, Copy, PartialEq)]
enum AnimationPhase {
    Scatter,  // Particles scattered randomly
    Assembly, // Particles moving to target positions
    Pulse,    // Text complete, colors pulsing
    Hold,     // Hold the final logo in white
    FadeOut,  // Fading out before showing terminal
}

/// White color for final logo
const LOGO_COLOR: Color = Color::Rgb(255, 255, 255);
/// Slightly dimmer white for shimmer effect
const LOGO_COLOR_ALT: Color = Color::Rgb(200, 200, 200);

/// A single particle in the animation
#[derive(Debug, Clone)]
struct Particle {
    /// Current X position (floating point for smooth animation)
    x: f64,
    /// Current Y position (floating point for smooth animation)
    y: f64,
    /// Target X position (final position in the text)
    target_x: f64,
    /// Target Y position (final position in the text)
    target_y: f64,
    /// Starting X position (random scatter position)
    start_x: f64,
    /// Starting Y position (random scatter position)
    start_y: f64,
    /// Random offset for color animation
    color_offset: f64,
}

impl Particle {
    fn new(
        target_x: f64,
        target_y: f64,
        screen_width: u16,
        screen_height: u16,
        index: usize,
    ) -> Self {
        // Pseudo-random scatter position based on index
        let seed = index as f64;
        let start_x = ((seed * 7.3).sin() * 0.5 + 0.5) * screen_width as f64;
        let start_y = ((seed * 11.7).cos() * 0.5 + 0.5) * screen_height as f64;

        Self {
            x: start_x,
            y: start_y,
            target_x,
            target_y,
            start_x,
            start_y,
            color_offset: seed * 0.1,
        }
    }

    /// Update particle position based on animation progress (0.0 to 1.0)
    fn update(&mut self, progress: f64) {
        // Ease-out cubic for smooth deceleration
        let eased = 1.0 - (1.0 - progress).powi(3);

        self.x = self.start_x + (self.target_x - self.start_x) * eased;
        self.y = self.start_y + (self.target_y - self.start_y) * eased;
    }

    /// Get current color with shimmer effect between two shades of white
    fn get_color(&self, time: f64, phase: AnimationPhase) -> Color {
        match phase {
            AnimationPhase::Scatter => {
                // Fast shimmer between white shades during scatter
                let shimmer = (time * 5.0 + self.color_offset * 10.0).sin() * 0.5 + 0.5;
                lerp_color(LOGO_COLOR_ALT, LOGO_COLOR, shimmer)
            }
            AnimationPhase::Assembly => {
                // Slower shimmer during assembly
                let shimmer = (time * 3.0 + self.color_offset * 5.0).sin() * 0.5 + 0.5;
                lerp_color(LOGO_COLOR_ALT, LOGO_COLOR, shimmer)
            }
            AnimationPhase::Pulse => {
                // Pulsing brightness on white
                let pulse = ((time * 3.0 + self.color_offset).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
                brighten_color(LOGO_COLOR, pulse)
            }
            AnimationPhase::Hold => {
                // Solid white for final display
                LOGO_COLOR
            }
            AnimationPhase::FadeOut => {
                // Keep white during fade
                LOGO_COLOR
            }
        }
    }
}

/// Splash screen state
#[derive(Debug)]
pub struct SplashScreen {
    particles: Vec<Particle>,
    start_time: Instant,
}

impl SplashScreen {
    /// Create a new splash screen
    pub fn new(screen_width: u16, screen_height: u16) -> Self {
        let particles = Self::create_particles(screen_width, screen_height);
        Self {
            particles,
            start_time: Instant::now(),
        }
    }

    /// Create particles from ASCII art
    fn create_particles(screen_width: u16, screen_height: u16) -> Vec<Particle> {
        let mut particles = Vec::new();

        // Calculate centering offset
        let art_width = LOGO_ART.first().map_or(0, |s| s.len());
        let offset_x = (screen_width as i32 - art_width as i32) / 2;
        let offset_y = (screen_height as i32 - ART_HEIGHT as i32) / 2;

        let mut index = 0;
        for (row, line) in LOGO_ART.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if ch == '@' {
                    let target_x = (col as i32 + offset_x).max(0) as f64;
                    let target_y = (row as i32 + offset_y).max(0) as f64;

                    particles.push(Particle::new(
                        target_x,
                        target_y,
                        screen_width,
                        screen_height,
                        index,
                    ));
                    index += 1;
                }
            }
        }

        particles
    }

    /// Get current animation phase based on elapsed time
    fn get_phase(&self) -> AnimationPhase {
        let elapsed = self.start_time.elapsed().as_secs_f64();

        if elapsed < 0.1 {
            AnimationPhase::Scatter
        } else if elapsed < 0.6 {
            AnimationPhase::Assembly
        } else if elapsed < 1.1 {
            AnimationPhase::Pulse
        } else if elapsed < 2.6 {
            AnimationPhase::Hold // 1.5 seconds of solid white logo
        } else {
            AnimationPhase::FadeOut
        }
    }

    /// Get assembly progress (0.0 to 1.0)
    fn get_assembly_progress(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();

        if elapsed < 0.1 {
            0.0
        } else if elapsed < 0.6 {
            (elapsed - 0.1) / 0.5
        } else {
            1.0
        }
    }

    /// Check if animation is complete
    pub fn is_complete(&self) -> bool {
        self.start_time.elapsed().as_secs_f64() > 2.8 // Total: 2.8 seconds
    }

    /// Update animation state
    pub fn update(&mut self) {
        let progress = self.get_assembly_progress();
        for particle in &mut self.particles {
            particle.update(progress);
        }
    }

    /// Render the splash screen
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let time = self.start_time.elapsed().as_secs_f64();
        let phase = self.get_phase();

        // Calculate fade out opacity
        let opacity = if phase == AnimationPhase::FadeOut {
            let fade_progress = (time - 2.6) / 0.2; // 0.2s fade out
            (1.0 - fade_progress).clamp(0.0, 1.0)
        } else {
            1.0
        };

        // Render each particle
        for particle in &self.particles {
            if particle.x >= 0.0
                && particle.x < area.width as f64
                && particle.y >= 0.0
                && particle.y < area.height as f64
            {
                let color = if opacity < 1.0 {
                    fade_color(particle.get_color(time, phase), opacity)
                } else {
                    particle.get_color(time, phase)
                };

                let x = particle.x as u16;
                let y = particle.y as u16;

                // Use block character for particle
                let block = ratatui::text::Span::styled(
                    "\u{2588}", // Full block █
                    ratatui::style::Style::default().fg(color),
                );

                frame.render_widget(
                    ratatui::widgets::Paragraph::new(block),
                    Rect::new(x, y, 1, 1),
                );
            }
        }
    }

    /// Run the splash screen animation
    pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        let size = terminal.size()?;
        let mut splash = SplashScreen::new(size.width, size.height);

        loop {
            // Check for key press to skip
            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        // Any key skips the splash
                        break;
                    }
                }
            }

            // Check if animation is complete
            if splash.is_complete() {
                break;
            }

            // Update and render
            splash.update();
            terminal.draw(|frame| splash.render(frame))?;
        }

        Ok(())
    }
}

/// Brighten or darken a color
fn brighten_color(color: Color, factor: f64) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            ((r as f64 * factor).min(255.0)) as u8,
            ((g as f64 * factor).min(255.0)) as u8,
            ((b as f64 * factor).min(255.0)) as u8,
        ),
        _ => color,
    }
}

/// Fade color toward black
fn fade_color(color: Color, opacity: f64) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f64 * opacity) as u8,
            (g as f64 * opacity) as u8,
            (b as f64 * opacity) as u8,
        ),
        _ => color,
    }
}

/// Linear interpolation between two colors
fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (a, b) {
        Color::Rgb(
            (r1 as f64 + (r2 as f64 - r1 as f64) * t) as u8,
            (g1 as f64 + (g2 as f64 - g1 as f64) * t) as u8,
            (b1 as f64 + (b2 as f64 - b1 as f64) * t) as u8,
        )
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_creation() {
        let particles = SplashScreen::create_particles(100, 30);
        assert!(!particles.is_empty());
    }

    #[test]
    fn test_animation_phases() {
        let splash = SplashScreen::new(100, 30);
        assert_eq!(splash.get_phase(), AnimationPhase::Scatter);
    }

    // ==================== Particle Tests ====================

    #[test]
    fn test_particle_new() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        assert_eq!(particle.target_x, 50.0);
        assert_eq!(particle.target_y, 15.0);
        // Start position should be somewhere in the screen bounds
        assert!(particle.start_x >= 0.0 && particle.start_x <= 100.0);
        assert!(particle.start_y >= 0.0 && particle.start_y <= 30.0);
    }

    #[test]
    fn test_particle_update_progress_0() {
        let mut particle = Particle::new(50.0, 15.0, 100, 30, 0);
        particle.update(0.0);
        // At progress 0, particle should be at start position
        assert!((particle.x - particle.start_x).abs() < 0.01);
        assert!((particle.y - particle.start_y).abs() < 0.01);
    }

    #[test]
    fn test_particle_update_progress_1() {
        let mut particle = Particle::new(50.0, 15.0, 100, 30, 0);
        particle.update(1.0);
        // At progress 1, particle should be at target position
        assert!((particle.x - particle.target_x).abs() < 0.01);
        assert!((particle.y - particle.target_y).abs() < 0.01);
    }

    #[test]
    fn test_particle_update_midway() {
        let mut particle = Particle::new(100.0, 0.0, 100, 30, 0);
        particle.start_x = 0.0;
        particle.start_y = 0.0;
        particle.update(0.5);
        // Midway should be somewhere between start and target
        assert!(particle.x > 0.0 && particle.x < 100.0);
    }

    #[test]
    fn test_particle_get_color_scatter() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        let color = particle.get_color(0.0, AnimationPhase::Scatter);
        // Color should be an RGB color
        match color {
            Color::Rgb(r, g, b) => {
                // White-ish color (between LOGO_COLOR_ALT and LOGO_COLOR)
                assert!(r >= 200);
                assert!(g >= 200);
                assert!(b >= 200);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_particle_get_color_assembly() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        let color = particle.get_color(0.3, AnimationPhase::Assembly);
        match color {
            Color::Rgb(r, g, b) => {
                assert!(r >= 200);
                assert!(g >= 200);
                assert!(b >= 200);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_particle_get_color_pulse() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        let color = particle.get_color(0.8, AnimationPhase::Pulse);
        match color {
            Color::Rgb(r, g, b) => {
                // Pulsing brightness
                assert!(r > 0);
                assert!(g > 0);
                assert!(b > 0);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_particle_get_color_hold() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        let color = particle.get_color(1.5, AnimationPhase::Hold);
        assert_eq!(color, LOGO_COLOR);
    }

    #[test]
    fn test_particle_get_color_fadeout() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        let color = particle.get_color(2.7, AnimationPhase::FadeOut);
        assert_eq!(color, LOGO_COLOR);
    }

    // ==================== Color Helper Tests ====================

    #[test]
    fn test_brighten_color_rgb() {
        let color = Color::Rgb(100, 100, 100);
        let brightened = brighten_color(color, 1.5);
        match brightened {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 150);
                assert_eq!(g, 150);
                assert_eq!(b, 150);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_brighten_color_clamped() {
        let color = Color::Rgb(200, 200, 200);
        let brightened = brighten_color(color, 2.0);
        match brightened {
            Color::Rgb(r, g, b) => {
                // Should be clamped to 255
                assert_eq!(r, 255);
                assert_eq!(g, 255);
                assert_eq!(b, 255);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_brighten_color_non_rgb() {
        let color = Color::Red;
        let result = brighten_color(color, 1.5);
        assert_eq!(result, Color::Red); // Non-RGB colors unchanged
    }

    #[test]
    fn test_fade_color_rgb() {
        let color = Color::Rgb(100, 200, 150);
        let faded = fade_color(color, 0.5);
        match faded {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 50);
                assert_eq!(g, 100);
                assert_eq!(b, 75);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_fade_color_zero_opacity() {
        let color = Color::Rgb(100, 100, 100);
        let faded = fade_color(color, 0.0);
        match faded {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 0);
                assert_eq!(g, 0);
                assert_eq!(b, 0);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_fade_color_non_rgb() {
        let color = Color::Blue;
        let result = fade_color(color, 0.5);
        assert_eq!(result, Color::Blue); // Non-RGB colors unchanged
    }

    #[test]
    fn test_lerp_color_rgb() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(100, 200, 150);
        let lerped = lerp_color(a, b, 0.5);
        match lerped {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 50);
                assert_eq!(g, 100);
                assert_eq!(b, 75);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_lerp_color_t_zero() {
        let a = Color::Rgb(100, 100, 100);
        let b = Color::Rgb(200, 200, 200);
        let lerped = lerp_color(a, b, 0.0);
        match lerped {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 100);
                assert_eq!(g, 100);
                assert_eq!(b, 100);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_lerp_color_t_one() {
        let a = Color::Rgb(100, 100, 100);
        let b = Color::Rgb(200, 200, 200);
        let lerped = lerp_color(a, b, 1.0);
        match lerped {
            Color::Rgb(r, g, b) => {
                assert_eq!(r, 200);
                assert_eq!(g, 200);
                assert_eq!(b, 200);
            }
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_lerp_color_non_rgb() {
        let a = Color::Red;
        let b = Color::Blue;
        let result = lerp_color(a, b, 0.5);
        assert_eq!(result, Color::Blue); // Falls back to b
    }

    // ==================== SplashScreen Tests ====================

    #[test]
    fn test_splash_screen_new() {
        let splash = SplashScreen::new(100, 30);
        assert!(!splash.particles.is_empty());
    }

    #[test]
    fn test_splash_screen_is_complete_initial() {
        let splash = SplashScreen::new(100, 30);
        assert!(!splash.is_complete());
    }

    #[test]
    fn test_splash_screen_update() {
        let mut splash = SplashScreen::new(100, 30);
        splash.update();
        // Should not panic, particles should be updated
    }

    #[test]
    fn test_splash_screen_get_assembly_progress_initial() {
        let splash = SplashScreen::new(100, 30);
        let progress = splash.get_assembly_progress();
        // Initial progress should be 0 (in scatter phase)
        assert!(progress >= 0.0 && progress <= 1.0);
    }

    #[test]
    fn test_animation_phase_debug() {
        assert_eq!(format!("{:?}", AnimationPhase::Scatter), "Scatter");
        assert_eq!(format!("{:?}", AnimationPhase::Assembly), "Assembly");
        assert_eq!(format!("{:?}", AnimationPhase::Pulse), "Pulse");
        assert_eq!(format!("{:?}", AnimationPhase::Hold), "Hold");
        assert_eq!(format!("{:?}", AnimationPhase::FadeOut), "FadeOut");
    }

    #[test]
    fn test_animation_phase_clone() {
        let phase = AnimationPhase::Assembly;
        let cloned = phase.clone();
        assert_eq!(phase, cloned);
    }

    #[test]
    fn test_animation_phase_copy() {
        let phase = AnimationPhase::Pulse;
        let copied: AnimationPhase = phase;
        assert_eq!(copied, AnimationPhase::Pulse);
    }

    #[test]
    fn test_splash_screen_debug() {
        let splash = SplashScreen::new(100, 30);
        let debug_str = format!("{:?}", splash);
        assert!(debug_str.contains("SplashScreen"));
        assert!(debug_str.contains("particles"));
    }

    #[test]
    fn test_particle_debug() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        let debug_str = format!("{:?}", particle);
        assert!(debug_str.contains("Particle"));
        assert!(debug_str.contains("target_x"));
    }

    #[test]
    fn test_particle_clone() {
        let particle = Particle::new(50.0, 15.0, 100, 30, 0);
        let cloned = particle.clone();
        assert_eq!(particle.target_x, cloned.target_x);
        assert_eq!(particle.target_y, cloned.target_y);
    }

    #[test]
    fn test_logo_art_not_empty() {
        assert!(!LOGO_ART.is_empty());
        assert!(LOGO_ART.len() == ART_HEIGHT);
    }

    #[test]
    fn test_create_particles_small_screen() {
        // Test with very small screen
        let particles = SplashScreen::create_particles(10, 5);
        assert!(!particles.is_empty());
    }

    #[test]
    fn test_create_particles_large_screen() {
        // Test with large screen
        let particles = SplashScreen::create_particles(200, 100);
        assert!(!particles.is_empty());
    }

    #[test]
    fn test_particles_different_indices() {
        // Particles with different indices should have different start positions
        let p1 = Particle::new(50.0, 15.0, 100, 30, 0);
        let p2 = Particle::new(50.0, 15.0, 100, 30, 1);
        // They have the same target but different start positions
        assert_eq!(p1.target_x, p2.target_x);
        // Start positions should differ due to different index seeds
        assert!((p1.start_x - p2.start_x).abs() > 0.001 || (p1.start_y - p2.start_y).abs() > 0.001);
    }
}
