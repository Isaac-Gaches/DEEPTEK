const ROW_HEIGHT: f32 = 52.0;
const ROW_GAP: f32 = 8.0;
const DELETE_BUTTON_WIDTH: f32 = 112.0;

#[derive(Clone, Copy)]
pub(super) struct MenuLayout {
    pub(super) panel: Rect,
    pub(super) create_button: Rect,
    pub(super) rows_top: f32,
    pub(super) visible_rows: usize,
}

impl MenuLayout {
    pub(super) fn new(viewport: [f32; 2]) -> Self {
        let width = (viewport[0] - 32.0).clamp(420.0, 760.0);
        let height = (viewport[1] - 32.0).clamp(560.0, 760.0);
        let panel = Rect {
            centre: [viewport[0] * 0.5, viewport[1] * 0.5],
            size: [width, height],
        };
        let create_button = Rect {
            centre: [panel.centre[0], panel.top() + 111.0],
            size: [width - 48.0, 56.0],
        };
        let rows_top = panel.top() + 195.0;
        let rows_bottom = panel.bottom() - 70.0;
        let visible_rows = (((rows_bottom - rows_top + ROW_GAP) / (ROW_HEIGHT + ROW_GAP))
            .floor()
            .max(1.0)) as usize;
        Self {
            panel,
            create_button,
            rows_top,
            visible_rows,
        }
    }

    pub(super) fn entry_buttons(self, visible_index: usize) -> (Rect, Rect) {
        let row = Rect {
            centre: [
                self.panel.centre[0],
                self.rows_top + ROW_HEIGHT * 0.5 + visible_index as f32 * (ROW_HEIGHT + ROW_GAP),
            ],
            size: [self.panel.size[0] - 48.0, ROW_HEIGHT],
        };
        let delete = Rect {
            centre: [row.right() - DELETE_BUTTON_WIDTH * 0.5, row.centre[1]],
            size: [DELETE_BUTTON_WIDTH, ROW_HEIGHT],
        };
        let load = Rect {
            centre: [(row.left() + delete.left() - ROW_GAP) * 0.5, row.centre[1]],
            size: [delete.left() - ROW_GAP - row.left(), ROW_HEIGHT],
        };
        (load, delete)
    }
}

#[derive(Clone, Copy)]
pub(super) struct CreationLayout {
    pub(super) fields: [Rect; 2],
    pub(super) size_buttons: [Rect; 3],
    pub(super) create: Rect,
    pub(super) back: Rect,
}

impl CreationLayout {
    pub(super) fn new(panel: Rect) -> Self {
        let field_width = panel.size[0] - 210.0;
        let fields = std::array::from_fn(|index| Rect {
            centre: [
                panel.centre[0] + 55.0,
                panel.top() + 140.0 + index as f32 * 66.0,
            ],
            size: [field_width, 48.0],
        });
        let size_width = (field_width - 16.0) / 3.0;
        let size_buttons = std::array::from_fn(|index| Rect {
            centre: [
                panel.centre[0] + 55.0 - field_width * 0.5
                    + size_width * 0.5
                    + index as f32 * (size_width + 8.0),
                panel.top() + 272.0,
            ],
            size: [size_width, 54.0],
        });
        let button_width = (panel.size[0] - 72.0) * 0.5;
        Self {
            fields,
            size_buttons,
            back: Rect {
                centre: [
                    panel.centre[0] - button_width * 0.5 - 6.0,
                    panel.bottom() - 58.0,
                ],
                size: [button_width, 50.0],
            },
            create: Rect {
                centre: [
                    panel.centre[0] + button_width * 0.5 + 6.0,
                    panel.bottom() - 58.0,
                ],
                size: [button_width, 50.0],
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ConfirmationLayout {
    pub(super) delete: Rect,
    pub(super) cancel: Rect,
}

impl ConfirmationLayout {
    pub(super) fn new(panel: Rect) -> Self {
        let width = (panel.size[0] - 84.0) * 0.5;
        Self {
            cancel: Rect {
                centre: [panel.centre[0] - width * 0.5 - 8.0, panel.centre[1] + 95.0],
                size: [width, 54.0],
            },
            delete: Rect {
                centre: [panel.centre[0] + width * 0.5 + 8.0, panel.centre[1] + 95.0],
                size: [width, 54.0],
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Rect {
    pub(super) centre: [f32; 2],
    pub(super) size: [f32; 2],
}

impl Rect {
    pub(super) fn left(self) -> f32 {
        self.centre[0] - self.size[0] * 0.5
    }

    pub(super) fn right(self) -> f32 {
        self.centre[0] + self.size[0] * 0.5
    }

    pub(super) fn top(self) -> f32 {
        self.centre[1] - self.size[1] * 0.5
    }

    pub(super) fn bottom(self) -> f32 {
        self.centre[1] + self.size[1] * 0.5
    }

    pub(super) fn contains(self, point: [f32; 2]) -> bool {
        let half = [self.size[0] * 0.5, self.size[1] * 0.5];
        (self.centre[0] - half[0]..=self.centre[0] + half[0]).contains(&point[0])
            && (self.centre[1] - half[1]..=self.centre[1] + half[1]).contains(&point[1])
    }
}
