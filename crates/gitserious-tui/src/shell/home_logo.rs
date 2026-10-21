use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::theme::JET_BLACK;

/// Width of the wordmark in terminal cells.
pub(super) const LOGO_WIDTH: u16 = 49;
/// Height of the wordmark in terminal rows.
pub(super) const LOGO_HEIGHT: u16 = 5;

const _: () = assert!(LOGO_WIDTH <= 58);
const _: () = assert!(LOGO_HEIGHT + 2 <= 12);

/// Compact 5-row `GITSERIOUS` wordmark built from full-block cells.
///
/// Spaces are transparent so the surrounding bordered frame shows through.
/// Every non-space cell is treated as foreground and animated by the active
/// [`LogoTheme`].
pub(super) const LOGO_LINES: [&str; 5] = [
    " ███  ███ █████  ███ ████ ███  ███  ██  █  █  ███",
    "█      █    █   █    █    █  █  █  █  █ █  █ █   ",
    "█ ██   █    █    ███ ███  ███   █  █  █ █  █  ███",
    "█   █  █    █      █ █    █ █   █  █  █ █  █    █",
    " ███  ███   █   ███  ████ █  █ ███  ██   ██  ███ ",
];

/// Ordered 4x4 Bayer matrix driving the dither portion of the shimmer.
const BAYER: [u64; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/// Sweep direction for the highlight band.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SweepAxis {
    Horizontal,
    Diagonal,
}

/// Saved wordmark animation.
///
/// Each theme is a pure palette + motion + glyph recipe over the same
/// `LOGO_LINES` shape, so themes are cheap to compare during dev. The first
/// entry is frozen: `yellow epileptic dreams` must stay byte-for-byte
/// identical to the original single-theme implementation.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(super) struct LogoTheme {
    /// Display name, e.g. `"yellow epileptic dreams"`.
    pub name: &'static str,
    /// Dev-only selector id, e.g. `"yellow-epileptic-dreams"`.
    pub id: &'static str,
    /// Time per shimmer step.
    pub frame_ms: u64,
    pub base: Color,
    pub highlight: Color,
    pub faded: Color,
    pub faint: Color,
    /// Bayer phase multiplier per tick. `0` disables glyph dissolve.
    pub dissolve_rate: u64,
    /// Highlight travel in cells per tick. `0` disables the sweep.
    pub sweep_speed: u64,
    /// Highlight band width in cells.
    pub sweep_band: u64,
    pub axis: SweepAxis,
    /// When true the sweep travels back-to-start (R→L / bottom→top).
    pub reverse: bool,
    /// When true the sweep bounces instead of wrapping.
    pub ping_pong: bool,
    /// Whether the highlight band is bold.
    pub bold_highlight: bool,
    /// When true the body buckets alternate between neighboring glyphs in a
    /// crawling `(x + y + tick)` checkerboard, so the texture sparkles
    /// instead of sitting solid.
    pub checkerboard: bool,
    /// Glyphs for dissolve buckets `[solid, mid, fine, ghost]`.
    ///
    /// `' '` means transparent (skip the cell) for true holes.
    pub glyphs: [char; 4],
}

impl LogoTheme {
    /// Foreground glyph + style for a wordmark cell.
    #[must_use]
    pub fn cell(&self, x: usize, y: usize, tick: u64) -> (char, Style) {
        let bayer = BAYER[(y % 4) * 4 + (x % 4)];
        let dissolve = if self.dissolve_rate == 0 {
            0
        } else {
            (bayer + tick.saturating_mul(self.dissolve_rate)) % 16
        };
        let in_band = self.in_band(x, y, tick);
        let checker_phase = x
            .saturating_add(y)
            .saturating_add(usize::try_from(tick).unwrap_or(0))
            % 2;

        let base_style = Style::default().fg(self.base).bg(JET_BLACK);
        let mut highlight_style = Style::default().fg(self.highlight).bg(JET_BLACK);
        if self.bold_highlight {
            highlight_style = highlight_style.add_modifier(Modifier::BOLD);
        }
        let faded_style = Style::default().fg(self.faded).bg(JET_BLACK);
        let faint_style = Style::default().fg(self.faint).bg(JET_BLACK);

        match dissolve {
            0..=10 => {
                let glyph = if self.checkerboard && checker_phase == 1 {
                    self.glyphs[1]
                } else {
                    self.glyphs[0]
                };
                if in_band {
                    (glyph, highlight_style)
                } else {
                    (glyph, base_style)
                }
            }
            11..=12 => {
                let glyph = if self.checkerboard && checker_phase == 1 {
                    self.glyphs[2]
                } else {
                    self.glyphs[1]
                };
                if in_band {
                    (glyph, highlight_style)
                } else {
                    (glyph, base_style)
                }
            }
            13 => (self.glyphs[2], faded_style),
            _ => (self.glyphs[3], faint_style),
        }
    }

    fn in_band(&self, x: usize, y: usize, tick: u64) -> bool {
        if self.sweep_speed == 0 || self.sweep_band == 0 {
            return false;
        }
        let span = match self.axis {
            SweepAxis::Horizontal => u64::from(LOGO_WIDTH).saturating_add(12),
            SweepAxis::Diagonal => u64::from(LOGO_WIDTH)
                .saturating_add(u64::from(LOGO_HEIGHT))
                .saturating_add(12),
        }
        .max(1);
        let period = if self.ping_pong {
            span.saturating_mul(2).max(1)
        } else {
            span
        };
        let raw = tick.saturating_mul(self.sweep_speed) % period;
        let mut pos = if self.ping_pong && raw >= span {
            span.saturating_mul(2).saturating_sub(raw)
        } else {
            raw
        };
        if self.reverse {
            pos = span.saturating_sub(pos);
        }
        let band_start = pos.saturating_sub(6);
        let x64 = u64::try_from(x).unwrap_or(u64::MAX);
        let y64 = u64::try_from(y).unwrap_or(u64::MAX);
        let coord = match self.axis {
            SweepAxis::Horizontal => x64,
            SweepAxis::Diagonal => x64.saturating_add(y64),
        };
        coord >= band_start && coord < band_start.saturating_add(self.sweep_band)
    }
}

/// The original implementation, frozen. Do not tweak.
pub(super) const YELLOW_EPILEPTIC_DREAMS: LogoTheme = LogoTheme {
    name: "yellow epileptic dreams",
    id: "yellow-epileptic-dreams",
    frame_ms: 90,
    base: Color::Yellow,
    highlight: Color::White,
    faded: Color::Gray,
    faint: Color::DarkGray,
    dissolve_rate: 2,
    sweep_speed: 3,
    sweep_band: 4,
    axis: SweepAxis::Horizontal,
    reverse: false,
    ping_pong: false,
    bold_highlight: true,
    checkerboard: false,
    glyphs: ['█', '▓', '▒', '░'],
};

/// Calm amber, solid blocks, slow narrow sweep, no dissolve.
pub(super) const AMBER_TERMINAL: LogoTheme = LogoTheme {
    name: "amber terminal",
    id: "amber-terminal",
    frame_ms: 160,
    base: Color::Rgb(255, 176, 0),
    highlight: Color::Rgb(255, 240, 200),
    faded: Color::Rgb(150, 120, 60),
    faint: Color::DarkGray,
    dissolve_rate: 0,
    sweep_speed: 1,
    sweep_band: 2,
    axis: SweepAxis::Horizontal,
    reverse: false,
    ping_pong: false,
    bold_highlight: false,
    checkerboard: false,
    glyphs: ['█', '█', '█', '█'],
};

/// Calm cyan drift, rare light dissolve, wide soft band travelling R→L.
pub(super) const GLACIER_DRIFT: LogoTheme = LogoTheme {
    name: "glacier drift",
    id: "glacier-drift",
    frame_ms: 180,
    base: Color::Rgb(120, 220, 255),
    highlight: Color::White,
    faded: Color::Rgb(150, 180, 190),
    faint: Color::DarkGray,
    dissolve_rate: 1,
    sweep_speed: 1,
    sweep_band: 6,
    axis: SweepAxis::Horizontal,
    reverse: true,
    ping_pong: false,
    bold_highlight: false,
    checkerboard: false,
    glyphs: ['█', '█', '▓', '░'],
};

/// Wild magenta/cyan diagonal ping-pong.
pub(super) const SYNTHWAVE_SUNSET: LogoTheme = LogoTheme {
    name: "synthwave sunset",
    id: "synthwave-sunset",
    frame_ms: 80,
    base: Color::Rgb(255, 45, 150),
    highlight: Color::Rgb(0, 229, 255),
    faded: Color::Gray,
    faint: Color::DarkGray,
    dissolve_rate: 2,
    sweep_speed: 2,
    sweep_band: 4,
    axis: SweepAxis::Diagonal,
    reverse: false,
    ping_pong: true,
    bold_highlight: true,
    checkerboard: false,
    glyphs: ['█', '▓', '▒', '░'],
};

/// Calm pixel grain: mostly solid `█` with a sparse `▓▒` sprinkle. Motion is
/// yellow epileptic dreams at one-eighth speed (same per-tick dissolve and
/// sweep, 8x the frame time), so the wordmark reads solid with a slow crawl.
pub(super) const STIPPLE_MIST: LogoTheme = LogoTheme {
    name: "stipple mist",
    id: "stipple-mist",
    frame_ms: 720,
    base: Color::White,
    highlight: Color::White,
    faded: Color::Gray,
    faint: Color::Gray,
    dissolve_rate: 2,
    sweep_speed: 3,
    sweep_band: 4,
    axis: SweepAxis::Horizontal,
    reverse: false,
    ping_pong: false,
    bold_highlight: true,
    checkerboard: false,
    glyphs: ['█', '█', '▓', '▒'],
};

/// Ultra-calm monochrome control, solid, slow faint sweep.
pub(super) const BONE_WHISPER: LogoTheme = LogoTheme {
    name: "bone whisper",
    id: "bone-whisper",
    frame_ms: 220,
    base: Color::Gray,
    highlight: Color::White,
    faded: Color::Gray,
    faint: Color::DarkGray,
    dissolve_rate: 0,
    sweep_speed: 1,
    sweep_band: 2,
    axis: SweepAxis::Horizontal,
    reverse: false,
    ping_pong: false,
    bold_highlight: false,
    checkerboard: false,
    glyphs: ['█', '█', '█', '█'],
};

/// Internal registry. Dev switches themes by id; no user-facing UI yet.
pub(super) const ALL_THEMES: [&LogoTheme; 6] = [
    &YELLOW_EPILEPTIC_DREAMS,
    &AMBER_TERMINAL,
    &GLACIER_DRIFT,
    &SYNTHWAVE_SUNSET,
    &STIPPLE_MIST,
    &BONE_WHISPER,
];

/// Default shown on launch. Change this line during dev to swap.
pub(super) const DEFAULT_THEME: &LogoTheme = &YELLOW_EPILEPTIC_DREAMS;

/// Dev-only lookup by id or name (case-insensitive).
#[must_use]
pub(super) fn theme_from_id(query: &str) -> Option<&'static LogoTheme> {
    ALL_THEMES.into_iter().find(|theme| {
        theme.id.eq_ignore_ascii_case(query) || theme.name.eq_ignore_ascii_case(query)
    })
}

/// Dev-only selector. `GITSERIOUS_LOGO_THEME=<id>` overrides the default for
/// quick A/B runs; unset or unknown falls back to [`DEFAULT_THEME`].
/// Session-only, no persistence, no Config UI.
#[must_use]
pub(super) fn resolve_dev_theme() -> &'static LogoTheme {
    #[cfg(test)]
    {
        DEFAULT_THEME
    }
    #[cfg(not(test))]
    {
        std::env::var("GITSERIOUS_LOGO_THEME")
            .ok()
            .as_deref()
            .and_then(theme_from_id)
            .unwrap_or(DEFAULT_THEME)
    }
}

/// Animation clock for the Home wordmark.
///
/// The shell drives this from `Shell::advance`; `frame` only moves in whole
/// per-theme `frame_ms` steps so `TestBackend` snapshots stay deterministic.
#[derive(Clone, Copy, Debug)]
pub(super) struct HomeLogo {
    elapsed: Duration,
    frame: u64,
    theme: &'static LogoTheme,
}

impl Default for HomeLogo {
    fn default() -> Self {
        Self {
            elapsed: Duration::ZERO,
            frame: 0,
            theme: DEFAULT_THEME,
        }
    }
}

impl HomeLogo {
    #[must_use]
    pub(super) fn with_theme(theme: &'static LogoTheme) -> Self {
        Self {
            elapsed: Duration::ZERO,
            frame: 0,
            theme,
        }
    }

    #[must_use]
    pub(super) fn theme(&self) -> &'static LogoTheme {
        self.theme
    }

    pub(super) fn advance(&mut self, delta: Duration) {
        let step_ms = self.theme.frame_ms.max(1);
        let step = Duration::from_millis(step_ms);
        self.elapsed += delta;
        while self.elapsed >= step {
            self.elapsed -= step;
            self.frame = self.frame.wrapping_add(1);
        }
    }

    #[must_use]
    pub(super) fn frame(&self) -> u64 {
        self.frame
    }
}

/// Renders the centered wordmark plus the version line below it.
///
/// `area` must already be the inside of the Home bordered block. The whole
/// composition (`LOGO_HEIGHT` + blank + version) is centered vertically; each
/// row is centered horizontally. Falls back to a single shimmering
/// `gitserious` line when space is too tight for the full graphic.
pub(super) fn render_home_logo(frame: &mut Frame<'_>, area: Rect, logo: &HomeLogo, version: &str) {
    const GAP: u16 = 1;
    const VERSION_HEIGHT: u16 = 1;
    let total_height = LOGO_HEIGHT
        .saturating_add(GAP)
        .saturating_add(VERSION_HEIGHT);

    if area.width < LOGO_WIDTH || area.height < total_height {
        render_fallback(frame, area, logo, version);
        return;
    }

    let theme = logo.theme();
    let tick = logo.frame();
    let logo_x = area
        .x
        .saturating_add(area.width.saturating_sub(LOGO_WIDTH) / 2);
    let start_y = area
        .y
        .saturating_add(area.height.saturating_sub(total_height) / 2);

    for (row, line) in LOGO_LINES.iter().enumerate() {
        let row_offset = u16::try_from(row).unwrap_or(0);
        let y = start_y.saturating_add(row_offset);
        for (col, cell) in line.chars().enumerate() {
            if cell == ' ' {
                continue;
            }
            let col_offset = u16::try_from(col).unwrap_or(u16::MAX);
            let x = logo_x.saturating_add(col_offset);
            if x >= area.right() || y >= area.bottom() {
                continue;
            }
            let (glyph, style) = theme.cell(col, row, tick);
            if glyph == ' ' {
                continue;
            }
            let buffer_cell = &mut frame.buffer_mut()[(x, y)];
            buffer_cell.set_char(glyph);
            buffer_cell.set_style(style);
        }
    }

    let version_y = start_y.saturating_add(LOGO_HEIGHT.saturating_add(GAP));
    let version_width = u16::try_from(Line::from(version).width()).unwrap_or(area.width);
    let version_width = version_width.min(area.width);
    let version_x = area
        .x
        .saturating_add(area.width.saturating_sub(version_width) / 2);
    let version_area = Rect::new(version_x, version_y, version_width, 1);
    frame.render_widget(Paragraph::new(version), version_area);
}

fn render_fallback(frame: &mut Frame<'_>, area: Rect, logo: &HomeLogo, version: &str) {
    let theme = logo.theme();
    let tick = logo.frame();
    let word = "gitserious";
    let word_width = u16::try_from(Line::from(word).width()).unwrap_or(area.width);
    let version_width = u16::try_from(Line::from(version).width()).unwrap_or(area.width);
    let show_both = area.height >= 3;
    let word_y = if show_both {
        area.y.saturating_add(area.height / 2).saturating_sub(1)
    } else {
        area.y.saturating_add(area.height / 2)
    };
    let word_width = word_width.min(area.width);
    let word_x = area
        .x
        .saturating_add(area.width.saturating_sub(word_width) / 2);
    let word_area = Rect::new(word_x, word_y, word_width, 1);
    for (col, ch) in word.chars().enumerate() {
        let col_offset = u16::try_from(col).unwrap_or(u16::MAX);
        let x = word_area.x.saturating_add(col_offset);
        if x >= word_area.right() {
            break;
        }
        let (_, style) = theme.cell(col, 0, tick);
        let cell = &mut frame.buffer_mut()[(x, word_area.y)];
        cell.set_char(ch);
        cell.set_style(style);
    }
    if show_both {
        let version_width = version_width.min(area.width);
        let version_x = area
            .x
            .saturating_add(area.width.saturating_sub(version_width) / 2);
        let version_area = Rect::new(version_x, word_y.saturating_add(2), version_width, 1);
        frame.render_widget(Paragraph::new(version), version_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_lines_match_declared_dimensions() {
        for line in LOGO_LINES {
            assert_eq!(
                u16::try_from(Line::from(line).width()).unwrap_or(99),
                LOGO_WIDTH
            );
        }
        assert_eq!(LOGO_LINES.len(), usize::from(LOGO_HEIGHT));
    }

    #[test]
    fn yellow_epileptic_dreams_matches_original_behavior() {
        let theme = &YELLOW_EPILEPTIC_DREAMS;
        assert_eq!(theme.frame_ms, 90);
        assert_eq!(theme.glyphs, ['█', '▓', '▒', '░']);
        let (glyph_a, _) = theme.cell(0, 0, 0);
        assert_eq!(glyph_a, '█');
        // Bayer 0 at (0,0): tick 1 gives dissolve 2 → still solid.
        let (glyph_b, _) = theme.cell(0, 0, 1);
        assert_eq!(glyph_b, '█');
        // Sweep must move colors across ticks/columns.
        let (_, style_a) = theme.cell(0, 0, 0);
        let (_, style_b) = theme.cell(10, 2, 4);
        assert_ne!(style_a, style_b);
    }

    #[test]
    fn all_themes_animate_and_stay_within_width() {
        use ratatui::text::Line as TextLine;
        for theme in ALL_THEMES {
            for glyph in theme.glyphs {
                if glyph != ' ' {
                    assert_eq!(
                        TextLine::from(glyph.to_string().as_str()).width(),
                        1,
                        "theme {} glyph {glyph:?} must be single-cell",
                        theme.name
                    );
                }
            }
            let logo = HomeLogo::with_theme(theme);
            assert_eq!(logo.frame(), 0);
            // Two frames must differ somewhere across the wordmark.
            let before: Vec<(char, Style)> = (0..LOGO_WIDTH)
                .flat_map(|x| {
                    (0..LOGO_HEIGHT)
                        .map(move |y| theme.cell(usize::from(x), usize::from(y), logo.frame()))
                })
                .collect();
            let after: Vec<(char, Style)> = (0..LOGO_WIDTH)
                .flat_map(|x| {
                    (0..LOGO_HEIGHT).map(move |y| {
                        theme.cell(
                            usize::from(x),
                            usize::from(y),
                            logo.frame().saturating_add(10),
                        )
                    })
                })
                .collect();
            assert_ne!(
                before, after,
                "theme {} must animate between frames",
                theme.name
            );
        }
    }

    #[test]
    fn theme_registry_has_unique_names_and_ids() {
        let mut names = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for theme in ALL_THEMES {
            assert!(names.insert(theme.name), "duplicate name {}", theme.name);
            assert!(ids.insert(theme.id), "duplicate id {}", theme.id);
            assert_eq!(
                theme_from_id(theme.id).map(|resolved| resolved.id),
                Some(theme.id)
            );
        }
        assert_eq!(DEFAULT_THEME.id, YELLOW_EPILEPTIC_DREAMS.id);
    }

    #[test]
    fn stipple_mist_is_pixelated_shade_texture() {
        let theme = theme_from_id("stipple-mist").unwrap_or(DEFAULT_THEME);
        assert_eq!(theme.glyphs, ['█', '█', '▓', '▒']);
        assert!(!theme.checkerboard);
        assert_eq!(theme.frame_ms, 720);
        // Same per-tick motion as yellow, double the frame time = half speed.
        assert_eq!(theme.dissolve_rate, YELLOW_EPILEPTIC_DREAMS.dissolve_rate);
        assert_eq!(theme.sweep_speed, YELLOW_EPILEPTIC_DREAMS.sweep_speed);
        assert_eq!(theme.sweep_band, YELLOW_EPILEPTIC_DREAMS.sweep_band);
        assert_eq!(theme.axis, YELLOW_EPILEPTIC_DREAMS.axis);
        assert_eq!(
            theme.frame_ms,
            YELLOW_EPILEPTIC_DREAMS.frame_ms.saturating_mul(8)
        );
        // No checker flip: neighbors and next tick stay solid like yellow.
        let (glyph_a, _) = STIPPLE_MIST.cell(0, 0, 0);
        let (glyph_b, _) = STIPPLE_MIST.cell(1, 0, 0);
        let (glyph_c, _) = STIPPLE_MIST.cell(0, 0, 1);
        assert_eq!(glyph_a, '█');
        assert_eq!(glyph_b, '█');
        assert_eq!(glyph_c, '█');
    }

    #[test]
    fn clock_quantizes_into_per_theme_frames() {
        let mut logo = HomeLogo::with_theme(&YELLOW_EPILEPTIC_DREAMS);
        assert_eq!(logo.frame(), 0);
        logo.advance(Duration::from_millis(
            YELLOW_EPILEPTIC_DREAMS.frame_ms.saturating_sub(1),
        ));
        assert_eq!(logo.frame(), 0);
        logo.advance(Duration::from_millis(1));
        assert_eq!(logo.frame(), 1);
        logo.advance(Duration::from_millis(
            YELLOW_EPILEPTIC_DREAMS.frame_ms.saturating_mul(3),
        ));
        assert_eq!(logo.frame(), 4);

        let mut slow = HomeLogo::with_theme(&BONE_WHISPER);
        slow.advance(Duration::from_millis(90));
        assert_eq!(slow.frame(), 0, "slow theme must not tick at 90ms");
        slow.advance(Duration::from_millis(130));
        assert_eq!(slow.frame(), 1);
    }

    #[test]
    fn all_themes_render_home_frame_without_panicking() -> Result<(), Box<dyn std::error::Error>> {
        use ratatui::{Terminal, backend::TestBackend};
        let version = "gitserious v0.0.0-test";
        for theme in ALL_THEMES {
            for (width, height) in [(100, 30), (60, 18)] {
                let mut logo = HomeLogo::with_theme(theme);
                logo.advance(Duration::from_millis(theme.frame_ms.saturating_mul(5)));
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|frame| {
                    let area = frame.area();
                    let block_inner = Rect::new(
                        area.x.saturating_add(1),
                        area.y.saturating_add(4),
                        area.width.saturating_sub(2),
                        area.height.saturating_sub(6),
                    );
                    render_home_logo(frame, block_inner, &logo, version);
                })?;
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content()
                    .iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect();
                assert!(
                    text.contains(version),
                    "theme {} must render version at {width}x{height}",
                    theme.name
                );
            }
        }
        Ok(())
    }
}
