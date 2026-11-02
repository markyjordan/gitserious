use std::sync::Arc;
use std::time::Duration;

use gitserious_app::NodeId;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use tachyonfx::{EffectTimer, Interpolation, fx};
use unicode_segmentation::UnicodeSegmentation;

use super::Chip;

const HOLD: Duration = Duration::from_millis(800);
const STEP: Duration = Duration::from_millis(150);
const GAP: &str = "   ";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Target {
    pub(super) node: NodeId,
    pub(super) prefix: String,
    pub(super) summary: String,
    pub(super) chip: Option<Chip>,
    pub(super) viewport: Rect,
    pub(super) area: Rect,
    pub(super) style: Style,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Carousel {
    active: Option<Active>,
}

#[derive(Clone, Debug)]
struct Active {
    target: Target,
    strip: Arc<Strip>,
    elapsed: Duration,
}

#[derive(Debug)]
struct Strip {
    graphemes: Vec<(String, usize)>,
    cycle: Duration,
}

impl Strip {
    fn new(summary: &str) -> Self {
        let graphemes = format!("{summary}{GAP}")
            .graphemes(true)
            .filter(|symbol| !symbol.contains(char::is_control))
            .map(|symbol| (symbol.to_owned(), Span::raw(symbol).width()))
            .filter(|(_, width)| *width > 0)
            .collect::<Vec<_>>();
        let cycle = STEP * u32::try_from(graphemes.len()).unwrap_or(u32::MAX);
        Self { graphemes, cycle }
    }

    fn paint(&self, buf: &mut Buffer, area: Rect, clip: Rect, elapsed: Duration, style: Style) {
        if clip.is_empty() {
            return;
        }
        for position in clip.positions() {
            buf[position].set_symbol(" ").set_style(style);
        }
        let start = if elapsed < HOLD {
            0
        } else {
            usize::try_from(elapsed.saturating_sub(HOLD).as_nanos() / STEP.as_nanos() + 1)
                .unwrap_or(0)
                % self.graphemes.len()
        };
        let mut x = usize::from(area.x);
        for (symbol, width) in self.graphemes.iter().cycle().skip(start) {
            let end = x + width;
            if end > usize::from(area.right()) {
                break;
            }
            if x >= usize::from(clip.x) && end <= usize::from(clip.right()) {
                buf[(u16::try_from(x).unwrap_or(area.right()), area.y)].set_symbol(symbol);
            }
            x = end;
        }
    }
}

impl Carousel {
    pub(super) fn reset(&mut self) {
        self.active = None;
    }

    pub(super) fn advance(&mut self, elapsed: Duration) {
        let Some(active) = &mut self.active else {
            return;
        };
        let total = active.elapsed.as_nanos() + elapsed.as_nanos();
        active.elapsed = if total < HOLD.as_nanos() {
            Duration::from_nanos(u64::try_from(total).unwrap_or(0))
        } else {
            HOLD + Duration::from_nanos(
                u64::try_from((total - HOLD.as_nanos()) % active.strip.cycle.as_nanos())
                    .unwrap_or(0),
            )
        };
    }

    pub(super) fn next_frame_in(&self) -> Option<Duration> {
        let elapsed = self.active.as_ref()?.elapsed;
        Some(if elapsed < HOLD {
            HOLD.saturating_sub(elapsed)
        } else {
            STEP.saturating_sub(Duration::from_nanos(
                u64::try_from(elapsed.saturating_sub(HOLD).as_nanos() % STEP.as_nanos())
                    .unwrap_or(0),
            ))
        })
    }

    pub(super) fn render(&mut self, buf: &mut Buffer, target: Option<Target>) {
        let Some(target) = target.filter(|target| !target.area.is_empty()) else {
            self.reset();
            return;
        };
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.target != target)
        {
            self.active = Some(Active {
                strip: Arc::new(Strip::new(&target.summary)),
                target,
                elapsed: Duration::ZERO,
            });
        }
        let Some(active) = &self.active else {
            return;
        };
        let style = active.target.style;
        let timer = EffectTimer::new(HOLD + active.strip.cycle, Interpolation::Linear);
        let mut effect = fx::effect_fn_buf(
            Arc::clone(&active.strip),
            timer,
            move |strip, context, buf| {
                let elapsed = context
                    .timer
                    .duration()
                    .saturating_sub(context.timer.remaining());
                let clip = context.area.intersection(buf.area);
                strip.paint(buf, context.area, clip, elapsed, style);
            },
        );
        effect.process(active.elapsed, buf, active.target.area);
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use ratatui::{Terminal, backend::TestBackend, buffer::Cell, style::Color};

    use super::*;

    fn target(summary: &str, width: u16) -> Target {
        Target {
            node: NodeId::Local(NonZeroU32::MIN),
            prefix: "▌  ▸ type  ".into(),
            summary: summary.into(),
            chip: Some(Chip::Modified),
            viewport: Rect::new(0, 0, 20, 4),
            area: Rect::new(4, 2, width, 1),
            style: Style::new().fg(Color::Gray).bg(Color::Blue),
        }
    }

    fn draw(
        carousel: &mut Carousel,
        target: Option<Target>,
    ) -> Result<Buffer, Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(20, 4))?;
        terminal.draw(|frame| {
            for position in frame.area().positions() {
                frame.buffer_mut()[position]
                    .set_symbol("#")
                    .set_style(Style::new().fg(Color::Green).bg(Color::Red));
            }
            carousel.render(frame.buffer_mut(), target);
        })?;
        Ok(terminal.backend().buffer().clone())
    }

    fn text(buffer: &Buffer, area: Rect) -> String {
        area.positions()
            .map(|position| buffer[position].symbol())
            .collect()
    }

    fn outside_unchanged(before: &Buffer, after: &Buffer, area: Rect) {
        for position in before
            .area
            .positions()
            .filter(|position| !area.contains(*position))
        {
            assert_eq!(
                before[position], after[position],
                "outside summary at {position:?}"
            );
        }
    }

    #[test]
    fn hold_steps_and_wrap_paint_only_summary_cells() -> Result<(), Box<dyn std::error::Error>> {
        let target = target("abcdefg", 4);
        let mut carousel = Carousel::default();
        let initial = draw(&mut carousel, Some(target.clone()))?;
        assert_eq!(text(&initial, target.area), "abcd");
        assert_eq!(carousel.next_frame_in(), Some(HOLD));
        carousel.advance(HOLD.saturating_sub(Duration::from_nanos(1)));
        assert_eq!(draw(&mut carousel, Some(target.clone()))?, initial);
        assert_eq!(carousel.next_frame_in(), Some(Duration::from_nanos(1)));
        carousel.advance(Duration::from_nanos(1));
        let moved = draw(&mut carousel, Some(target.clone()))?;
        assert_eq!(text(&moved, target.area), "bcde");
        assert_eq!(carousel.next_frame_in(), Some(STEP));
        outside_unchanged(&initial, &moved, target.area);
        carousel.advance(STEP * 6);
        assert_eq!(
            text(&draw(&mut carousel, Some(target.clone()))?, target.area),
            "   a"
        );
        carousel.advance(STEP);
        assert_eq!(
            text(&draw(&mut carousel, Some(target.clone()))?, target.area),
            "  ab"
        );
        carousel.advance(STEP * 2);
        assert_eq!(draw(&mut carousel, Some(target.clone()))?, initial);
        carousel.advance(STEP);
        assert_eq!(draw(&mut carousel, Some(target.clone()))?, moved);
        assert_eq!(draw(&mut carousel, Some(target))?, moved);
        Ok(())
    }

    #[test]
    fn elapsed_partitions_and_repeated_renders_are_identical()
    -> Result<(), Box<dyn std::error::Error>> {
        let target = target("a界e\u{301}👩‍💻z", 5);
        let mut once = Carousel::default();
        let mut partitioned = Carousel::default();
        draw(&mut once, Some(target.clone()))?;
        draw(&mut partitioned, Some(target.clone()))?;
        let parts = [
            HOLD / 3,
            Duration::from_nanos(13),
            STEP * 29,
            HOLD,
            STEP / 7,
        ];
        once.advance(parts.into_iter().sum());
        for part in parts {
            partitioned.advance(part);
            draw(&mut partitioned, Some(target.clone()))?;
        }
        let expected = draw(&mut once, Some(target.clone()))?;
        assert_eq!(draw(&mut partitioned, Some(target.clone()))?, expected);
        assert_eq!(draw(&mut partitioned, Some(target))?, expected);
        assert_eq!(once.next_frame_in(), partitioned.next_frame_in());
        Ok(())
    }

    #[test]
    fn unicode_steps_keep_complete_graphemes_and_wide_cells()
    -> Result<(), Box<dyn std::error::Error>> {
        for (source, first, next) in [
            ("abcdefg", "abcde", "bcdef"),
            ("界日本語ab", "界 日  ", "日 本  "),
            ("e\u{301}a👩‍💻界z", "e\u{301}a👩‍💻  ", "a👩‍💻 界 "),
            ("👩‍💻界e\u{301}xyz", "👩‍💻 界 e\u{301}", "界 e\u{301}xy"),
        ] {
            let mut carousel = Carousel::default();
            let target = target(source, 5);
            let initial = draw(&mut carousel, Some(target.clone()))?;
            assert_eq!(text(&initial, target.area), first, "{source}");
            carousel.advance(HOLD);
            let moved = draw(&mut carousel, Some(target.clone()))?;
            assert_eq!(text(&moved, target.area), next, "{source}");
            outside_unchanged(&initial, &moved, target.area);
            for position in target.area.positions() {
                if moved[position].symbol() != " " {
                    assert_eq!(moved[position].bg, Color::Blue);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn one_cell_and_clipped_buffer_edges_never_split_a_wide_grapheme()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut carousel = Carousel::default();
        let single = target("界abcdef", 1);
        assert_eq!(
            text(&draw(&mut carousel, Some(single.clone()))?, single.area),
            " "
        );
        carousel.advance(HOLD);
        assert_eq!(
            text(&draw(&mut carousel, Some(single.clone()))?, single.area),
            "a"
        );
        let sentinel = Cell::new("#");
        let mut buffer = Buffer::filled(Rect::new(5, 1, 5, 3), sentinel);
        let before = buffer.clone();
        let mut clipped = target("界abcdef", 5);
        clipped.area = Rect::new(4, 2, 5, 1);
        carousel.render(&mut buffer, Some(clipped.clone()));
        assert_eq!(
            text(&buffer, clipped.area.intersection(buffer.area)),
            " abc"
        );
        outside_unchanged(&before, &buffer, clipped.area);
        clipped.area = Rect::new(8, 2, 4, 1);
        clipped.summary = "a界bcdef".into();
        let before = buffer.clone();
        carousel.render(&mut buffer, Some(clipped.clone()));
        assert_eq!(text(&buffer, clipped.area.intersection(buffer.area)), "a ");
        outside_unchanged(&before, &buffer, clipped.area);
        let mut empty = single;
        empty.area.width = 0;
        carousel.render(&mut buffer, Some(empty));
        assert_eq!(carousel.next_frame_in(), None);
        Ok(())
    }

    #[test]
    fn changed_identity_content_badge_or_viewport_restarts_the_hold()
    -> Result<(), Box<dyn std::error::Error>> {
        let original = target("abcdefghijk", 4);
        let mut variants = Vec::new();
        let mut changed = original.clone();
        changed.node =
            NodeId::Local(NonZeroU32::new(2).ok_or_else(|| std::io::Error::other("nonzero id"))?);
        variants.push(changed);
        let mut changed = original.clone();
        changed.prefix.push('x');
        variants.push(changed);
        let mut changed = original.clone();
        changed.summary.push('z');
        variants.push(changed);
        let mut changed = original.clone();
        changed.chip = Some(Chip::Invalid(1));
        variants.push(changed);
        let mut changed = original.clone();
        changed.area.width += 1;
        variants.push(changed);
        let mut changed = original.clone();
        changed.viewport.height += 1;
        variants.push(changed);
        for changed in variants {
            let mut carousel = Carousel::default();
            draw(&mut carousel, Some(original.clone()))?;
            carousel.advance(HOLD + STEP);
            draw(&mut carousel, Some(original.clone()))?;
            draw(&mut carousel, Some(changed.clone()))?;
            assert_eq!(carousel.next_frame_in(), Some(HOLD));
            let fresh = draw(&mut Carousel::default(), Some(changed.clone()))?;
            assert_eq!(draw(&mut carousel, Some(changed))?, fresh);
            draw(&mut carousel, None)?;
            assert_eq!(carousel.next_frame_in(), None);
        }
        Ok(())
    }
}
