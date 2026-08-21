use super::GuiRenderer;

const MARGIN: f32 = 14.0;
const STATUS_HEIGHT: f32 = 140.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HudAction {
    OpenContracts,
    Pause,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MeterValue {
    pub current: u16,
    pub maximum: u16,
}

impl MeterValue {
    pub const fn new(current: u16, maximum: u16) -> Self {
        Self { current, maximum }
    }

    fn fraction(self) -> f32 {
        if self.maximum == 0 {
            0.0
        } else {
            f32::from(self.current.min(self.maximum)) / f32::from(self.maximum)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HudSnapshot {
    pub health: MeterValue,
    pub energy: MeterValue,
    pub money: u64,
    pub depth_decimetres: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HudGui;

impl HudGui {
    pub fn handle_click(self, cursor: [f32; 2], viewport: [f32; 2]) -> Option<HudAction> {
        let layout = HudLayout::new(viewport);
        if layout.contracts.contains(cursor) {
            Some(HudAction::OpenContracts)
        } else if layout.pause.contains(cursor) {
            Some(HudAction::Pause)
        } else {
            None
        }
    }

    pub fn captures_pointer(self, cursor: [f32; 2], viewport: [f32; 2]) -> bool {
        let layout = HudLayout::new(viewport);
        layout.status.contains(cursor)
            || layout.contracts.contains(cursor)
            || layout.pause.contains(cursor)
    }

    pub fn queue(
        self,
        renderer: &mut GuiRenderer,
        snapshot: HudSnapshot,
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) {
        let layout = HudLayout::new(viewport);
        renderer.queue_rect(
            layout.status.centre,
            layout.status.size,
            [0.025, 0.04, 0.07, 0.92],
        );

        let left = layout.status.left() + 12.0;
        let bar_left = left + 68.0;
        let bar_width = (layout.status.right() - 12.0 - bar_left).max(48.0);
        queue_meter(
            renderer,
            "HEALTH",
            snapshot.health,
            [bar_left, layout.status.top() + 18.0],
            bar_width,
            [0.78, 0.12, 0.15, 1.0],
        );
        queue_meter(
            renderer,
            "ENERGY",
            snapshot.energy,
            [bar_left, layout.status.top() + 52.0],
            bar_width,
            [0.08, 0.66, 0.82, 1.0],
        );
        renderer.queue_text(
            &format!("MONEY {}", snapshot.money),
            [left, layout.status.top() + 88.0],
            1.7,
            [1.0, 0.78, 0.25, 1.0],
        );
        renderer.queue_text(
            &format_depth(snapshot.depth_decimetres),
            [left, layout.status.top() + 112.0],
            1.5,
            [0.62, 0.84, 0.94, 1.0],
        );

        queue_button(
            renderer,
            layout.contracts,
            cursor,
            "CONTRACTS",
            [0.12, 0.30, 0.46, 0.96],
        );
        queue_button(
            renderer,
            layout.pause,
            cursor,
            "PAUSE",
            [0.16, 0.20, 0.29, 0.96],
        );
    }
}

fn format_depth(depth_decimetres: i32) -> String {
    let sign = if depth_decimetres >= 0 { '+' } else { '-' };
    let magnitude = depth_decimetres.unsigned_abs();
    format!("DEPTH {sign}{}.{:01}M", magnitude / 10, magnitude % 10)
}

fn queue_meter(
    renderer: &mut GuiRenderer,
    label: &str,
    value: MeterValue,
    bar_top_left: [f32; 2],
    width: f32,
    colour: [f32; 4],
) {
    renderer.queue_text(
        label,
        [bar_top_left[0] - 68.0, bar_top_left[1] + 2.0],
        1.5,
        [0.78, 0.84, 0.92, 1.0],
    );
    let centre = [bar_top_left[0] + width * 0.5, bar_top_left[1] + 9.0];
    renderer.queue_rect(centre, [width, 18.0], [0.055, 0.075, 0.11, 1.0]);
    let fill_width = width * value.fraction();
    if fill_width > 0.0 {
        renderer.queue_rect(
            [bar_top_left[0] + fill_width * 0.5, centre[1]],
            [fill_width, 18.0],
            colour,
        );
    }
    let text = format!("{}/{}", value.current, value.maximum);
    renderer.queue_text(
        &text,
        [
            centre[0] - GuiRenderer::text_width(&text, 1.0) * 0.5,
            centre[1] - 3.5,
        ],
        1.0,
        [1.0; 4],
    );
}

fn queue_button(
    renderer: &mut GuiRenderer,
    button: Rect,
    cursor: [f32; 2],
    label: &str,
    colour: [f32; 4],
) {
    let tint = if button.contains(cursor) {
        [
            colour[0] * 1.25,
            colour[1] * 1.25,
            colour[2] * 1.25,
            colour[3],
        ]
    } else {
        colour
    };
    renderer.queue_rect(button.centre, button.size, tint);
    renderer.queue_text(
        label,
        [
            button.centre[0] - GuiRenderer::text_width(label, 2.0) * 0.5,
            button.centre[1] - 7.0,
        ],
        2.0,
        [0.94, 0.97, 1.0, 1.0],
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HudLayout {
    status: Rect,
    contracts: Rect,
    pause: Rect,
}

impl HudLayout {
    fn new(viewport: [f32; 2]) -> Self {
        let button_width = if viewport[0] < 560.0 { 110.0 } else { 132.0 };
        let available_status_width = viewport[0] - MARGIN * 3.0 - button_width;
        let status_width = available_status_width.clamp(170.0, 280.0);
        let status = Rect {
            centre: [MARGIN + status_width * 0.5, MARGIN + STATUS_HEIGHT * 0.5],
            size: [status_width, STATUS_HEIGHT],
        };
        let button_x = (viewport[0] - MARGIN - button_width * 0.5)
            .max(status.right() + MARGIN + button_width * 0.5);
        Self {
            status,
            contracts: Rect {
                centre: [button_x, MARGIN + 19.0],
                size: [button_width, 38.0],
            },
            pause: Rect {
                centre: [button_x, MARGIN + 63.0],
                size: [button_width, 38.0],
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    centre: [f32; 2],
    size: [f32; 2],
}

impl Rect {
    fn left(self) -> f32 {
        self.centre[0] - self.size[0] * 0.5
    }

    fn right(self) -> f32 {
        self.centre[0] + self.size[0] * 0.5
    }

    fn top(self) -> f32 {
        self.centre[1] - self.size[1] * 0.5
    }

    fn contains(self, point: [f32; 2]) -> bool {
        let half = [self.size[0] * 0.5, self.size[1] * 0.5];
        (self.centre[0] - half[0]..=self.centre[0] + half[0]).contains(&point[0])
            && (self.centre[1] - half[1]..=self.centre[1] + half[1]).contains(&point[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_buttons_are_distinct_and_do_not_overlap_status_card() {
        let viewport = [800.0, 600.0];
        let layout = HudLayout::new(viewport);
        let hud = HudGui;
        assert_eq!(
            hud.handle_click(layout.contracts.centre, viewport),
            Some(HudAction::OpenContracts)
        );
        assert_eq!(
            hud.handle_click(layout.pause.centre, viewport),
            Some(HudAction::Pause)
        );
        assert!(layout.status.right() < layout.contracts.left());
    }

    #[test]
    fn meter_fraction_is_bounded_and_handles_zero_maximum() {
        assert_eq!(MeterValue::new(75, 100).fraction(), 0.75);
        assert_eq!(MeterValue::new(200, 100).fraction(), 1.0);
        assert_eq!(MeterValue::new(1, 0).fraction(), 0.0);
    }

    #[test]
    fn compact_hud_stays_inside_a_narrow_viewport() {
        let viewport = [360.0, 480.0];
        let layout = HudLayout::new(viewport);
        assert!(layout.status.left() >= 0.0);
        assert!(layout.pause.right() <= viewport[0]);
        assert!(layout.status.right() < layout.pause.left());
    }

    #[test]
    fn depth_text_includes_sign_and_single_decimal_place() {
        assert_eq!(format_depth(126), "DEPTH +12.6M");
        assert_eq!(format_depth(-7), "DEPTH -0.7M");
    }
}
