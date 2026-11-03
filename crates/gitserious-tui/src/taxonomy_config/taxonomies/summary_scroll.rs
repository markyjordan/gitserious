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

const HOLD: Duration = Duration::from_secs(3);
const STEP: Duration = Duration::from_millis(150);

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
pub(super) struct SummaryScroll {
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
    end: usize,
    travel: Duration,
    cycle: Duration,
}

impl Strip {
    fn new(summary: &str, width: usize) -> Self {
        let graphemes = summary
            .graphemes(true)
            .filter(|symbol| !symbol.contains(char::is_control))
            .map(|symbol| (symbol.to_owned(), Span::raw(symbol).width()))
            .filter(|(_, width)| *width > 0)
            .collect::<Vec<_>>();
        let mut remaining = graphemes.iter().map(|(_, width)| width).sum::<usize>();
        let end = graphemes
            .iter()
            .take_while(|(_, size)| {
                if remaining <= width {
                    return false;
                }
                remaining -= size;
                true
            })
            .count();
        let travel = STEP * u32::try_from(end.saturating_sub(1)).unwrap_or(u32::MAX);
        let cycle = HOLD * 2 + travel * 2;
        Self {
            graphemes,
            end,
            travel,
            cycle,
        }
    }

    fn phase(&self, elapsed: Duration) -> (usize, Duration) {
        if elapsed < HOLD {
            return (0, HOLD.saturating_sub(elapsed));
        }
        let forward = elapsed.saturating_sub(HOLD);
        if forward < self.travel {
            return (1 + steps(forward), STEP.saturating_sub(remainder(forward)));
        }
        let reverse_at = HOLD * 2 + self.travel;
        if elapsed < reverse_at {
            return (self.end, reverse_at.saturating_sub(elapsed));
        }
        let reverse = elapsed.saturating_sub(reverse_at);
        (
            self.end.saturating_sub(1 + steps(reverse)),
            STEP.saturating_sub(remainder(reverse)),
        )
    }

    fn paint(&self, buf: &mut Buffer, area: Rect, clip: Rect, elapsed: Duration, style: Style) {
        if clip.is_empty() {
            return;
        }
        for position in clip.positions() {
            buf[position].set_symbol(" ").set_style(style);
        }
        let (start, _) = self.phase(elapsed);
        let mut x = usize::from(area.x);
        for (symbol, width) in self.graphemes.iter().skip(start) {
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

fn steps(elapsed: Duration) -> usize {
    usize::try_from(elapsed.as_nanos() / STEP.as_nanos()).unwrap_or(0)
}

fn remainder(elapsed: Duration) -> Duration {
    Duration::from_nanos(u64::try_from(elapsed.as_nanos() % STEP.as_nanos()).unwrap_or(0))
}

impl SummaryScroll {
    pub(super) fn reset(&mut self) {
        self.active = None;
    }

    pub(super) fn advance(&mut self, elapsed: Duration) {
        let Some(active) = &mut self.active else {
            return;
        };
        let total = active.elapsed.as_nanos() + elapsed.as_nanos();
        active.elapsed =
            Duration::from_nanos(u64::try_from(total % active.strip.cycle.as_nanos()).unwrap_or(0));
    }

    pub(super) fn next_frame_in(&self) -> Option<Duration> {
        let active = self.active.as_ref()?;
        Some(active.strip.phase(active.elapsed).1)
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
                strip: Arc::new(Strip::new(&target.summary, usize::from(target.area.width))),
                target,
                elapsed: Duration::ZERO,
            });
        }
        let Some(active) = &self.active else {
            return;
        };
        let style = active.target.style;
        let timer = EffectTimer::new(active.strip.cycle, Interpolation::Linear);
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
            chip: None,
            viewport: Rect::new(0, 0, 20, 4),
            area: Rect::new(4, 2, width, 1),
            style: Style::new().fg(Color::Gray).bg(Color::Blue),
        }
    }

    fn draw(
        summary_scroll: &mut SummaryScroll,
        target: Option<Target>,
    ) -> Result<Buffer, Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(20, 4))?;
        terminal.draw(|frame| {
            for position in frame.area().positions() {
                frame.buffer_mut()[position]
                    .set_symbol("#")
                    .set_style(Style::new().fg(Color::Green).bg(Color::Red));
            }
            summary_scroll.render(frame.buffer_mut(), target);
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
    fn holds_reversal_endpoints_and_cycles_paint_only_summary_cells()
    -> Result<(), Box<dyn std::error::Error>> {
        let target = target("abcdefg", 4);
        let mut summary_scroll = SummaryScroll::default();
        let initial = draw(&mut summary_scroll, Some(target.clone()))?;
        assert_eq!(text(&initial, target.area), "abcd");
        assert_eq!(summary_scroll.next_frame_in(), Some(HOLD));
        summary_scroll.advance(HOLD.saturating_sub(Duration::from_nanos(1)));
        assert_eq!(draw(&mut summary_scroll, Some(target.clone()))?, initial);
        assert_eq!(
            summary_scroll.next_frame_in(),
            Some(Duration::from_nanos(1))
        );
        summary_scroll.advance(Duration::from_nanos(1));
        let moved = draw(&mut summary_scroll, Some(target.clone()))?;
        assert_eq!(text(&moved, target.area), "bcde");
        assert_eq!(summary_scroll.next_frame_in(), Some(STEP));
        outside_unchanged(&initial, &moved, target.area);
        summary_scroll.advance(STEP * 2);
        let end = draw(&mut summary_scroll, Some(target.clone()))?;
        assert_eq!(text(&end, target.area), "defg");
        assert_eq!(summary_scroll.next_frame_in(), Some(HOLD));
        summary_scroll.advance(HOLD.saturating_sub(Duration::from_nanos(1)));
        assert_eq!(draw(&mut summary_scroll, Some(target.clone()))?, end);
        summary_scroll.advance(Duration::from_nanos(1));
        assert_eq!(
            text(
                &draw(&mut summary_scroll, Some(target.clone()))?,
                target.area
            ),
            "cdef"
        );
        summary_scroll.advance(STEP * 2);
        assert_eq!(draw(&mut summary_scroll, Some(target.clone()))?, initial);
        assert_eq!(summary_scroll.next_frame_in(), Some(HOLD));
        summary_scroll.advance((HOLD * 2 + STEP * 4) * 100 + HOLD);
        assert_eq!(draw(&mut summary_scroll, Some(target))?, moved);
        Ok(())
    }

    #[test]
    fn finite_endpoints_fit_without_repeating_text() {
        for (summary, width, end) in [("abcde", 4, 1), ("界abc", 4, 1), ("a界e\u{301}👩‍💻z", 5, 2)]
        {
            let strip = Strip::new(summary, width);
            assert_eq!(strip.end, end);
            let suffix = &strip.graphemes[end..];
            assert!(suffix.iter().map(|(_, width)| width).sum::<usize>() <= width);
            for cycle in 0..3 {
                let mut scroll = SummaryScroll::default();
                let target = target(summary, u16::try_from(width).unwrap_or(0));
                let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 4));
                scroll.render(&mut buffer, Some(target.clone()));
                scroll.advance(strip.cycle * cycle + HOLD + strip.travel);
                scroll.render(&mut buffer, Some(target.clone()));
                let mut expected = String::new();
                for (symbol, width) in suffix {
                    expected.push_str(symbol);
                    expected.push_str(&" ".repeat(width.saturating_sub(1)));
                }
                expected.push_str(
                    &" ".repeat(width - suffix.iter().map(|(_, width)| width).sum::<usize>()),
                );
                assert_eq!(text(&buffer, target.area), expected);
                assert_eq!(scroll.next_frame_in(), Some(HOLD));
                scroll.advance(HOLD + strip.travel);
                assert_eq!(
                    scroll.active.as_ref().map(|active| active.elapsed),
                    Some(Duration::ZERO)
                );
            }
        }
    }

    #[test]
    fn elapsed_partitions_and_repeated_renders_are_identical()
    -> Result<(), Box<dyn std::error::Error>> {
        let target = target("a界e\u{301}👩‍💻z", 5);
        let mut once = SummaryScroll::default();
        let mut partitioned = SummaryScroll::default();
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
            let mut summary_scroll = SummaryScroll::default();
            let target = target(source, 5);
            let initial = draw(&mut summary_scroll, Some(target.clone()))?;
            assert_eq!(text(&initial, target.area), first, "{source}");
            summary_scroll.advance(HOLD);
            let moved = draw(&mut summary_scroll, Some(target.clone()))?;
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
        let mut summary_scroll = SummaryScroll::default();
        let single = target("界abcdef", 1);
        assert_eq!(
            text(
                &draw(&mut summary_scroll, Some(single.clone()))?,
                single.area
            ),
            " "
        );
        summary_scroll.advance(HOLD);
        assert_eq!(
            text(
                &draw(&mut summary_scroll, Some(single.clone()))?,
                single.area
            ),
            "a"
        );
        let sentinel = Cell::new("#");
        let mut buffer = Buffer::filled(Rect::new(5, 1, 5, 3), sentinel);
        let before = buffer.clone();
        let mut clipped = target("界abcdef", 5);
        clipped.area = Rect::new(4, 2, 5, 1);
        summary_scroll.render(&mut buffer, Some(clipped.clone()));
        assert_eq!(
            text(&buffer, clipped.area.intersection(buffer.area)),
            " abc"
        );
        outside_unchanged(&before, &buffer, clipped.area);
        clipped.area = Rect::new(8, 2, 4, 1);
        clipped.summary = "a界bcdef".into();
        let before = buffer.clone();
        summary_scroll.render(&mut buffer, Some(clipped.clone()));
        assert_eq!(text(&buffer, clipped.area.intersection(buffer.area)), "a ");
        outside_unchanged(&before, &buffer, clipped.area);
        let mut empty = single;
        empty.area.width = 0;
        summary_scroll.render(&mut buffer, Some(empty));
        assert_eq!(summary_scroll.next_frame_in(), None);
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
        changed.chip = Some(Chip::Invalid);
        variants.push(changed);
        let mut changed = original.clone();
        changed.area.width += 1;
        variants.push(changed);
        let mut changed = original.clone();
        changed.viewport.height += 1;
        variants.push(changed);
        for changed in variants {
            let mut summary_scroll = SummaryScroll::default();
            draw(&mut summary_scroll, Some(original.clone()))?;
            summary_scroll.advance(HOLD + STEP);
            draw(&mut summary_scroll, Some(original.clone()))?;
            draw(&mut summary_scroll, Some(changed.clone()))?;
            assert_eq!(summary_scroll.next_frame_in(), Some(HOLD));
            let fresh = draw(&mut SummaryScroll::default(), Some(changed.clone()))?;
            assert_eq!(draw(&mut summary_scroll, Some(changed))?, fresh);
            draw(&mut summary_scroll, None)?;
            assert_eq!(summary_scroll.next_frame_in(), None);
        }
        Ok(())
    }
}
