use super::creation::{CreationField, CreationForm, WorldSize};
use super::layout::{ConfirmationLayout, CreationLayout, MenuLayout, Rect};
use deep_tek::GuiRenderer;

pub(super) fn queue_creation(
    renderer: &mut GuiRenderer,
    layout: MenuLayout,
    form: &CreationForm,
    cursor: [f32; 2],
) {
    queue_title(renderer, layout, "CREATE WORLD");
    let creation = CreationLayout::new(layout.panel);
    let labels = ["NAME", "SEED"];
    let values = [&form.name, &form.seed];
    for (index, field) in creation.fields.iter().copied().enumerate() {
        let active = CreationField::ALL[index] == form.active;
        renderer.queue_text(
            labels[index],
            [layout.panel.left() + 30.0, field.centre[1] - 7.0],
            2.0,
            [0.65, 0.74, 0.88, 1.0],
        );
        renderer.queue_rect(
            field.centre,
            field.size,
            if active {
                [0.16, 0.29, 0.48, 1.0]
            } else {
                [0.08, 0.13, 0.23, 1.0]
            },
        );
        let text_position = [field.left() + 14.0, field.centre[1] - 9.0];
        renderer.queue_text(values[index], text_position, 2.5, [0.96, 0.98, 1.0, 1.0]);
        if active {
            renderer.queue_text(
                "-",
                [
                    text_position[0] + GuiRenderer::text_width(values[index], 2.5),
                    text_position[1],
                ],
                2.5,
                [0.96, 0.98, 1.0, 1.0],
            );
        }
    }
    renderer.queue_text(
        "WORLD SIZE",
        [
            layout.panel.left() + 30.0,
            creation.size_buttons[0].centre[1] - 7.0,
        ],
        2.0,
        [0.65, 0.74, 0.88, 1.0],
    );
    for (size, button) in WorldSize::ALL
        .iter()
        .copied()
        .zip(creation.size_buttons.iter().copied())
    {
        let selected = size == form.size;
        let hovered = button.contains(cursor);
        renderer.queue_rect(
            button.centre,
            button.size,
            if selected {
                [0.12, 0.42, 0.34, 1.0]
            } else if hovered {
                [0.16, 0.29, 0.48, 1.0]
            } else {
                [0.08, 0.13, 0.23, 1.0]
            },
        );
        queue_centred_text(
            renderer,
            size.label(),
            button.centre[0],
            button.top() + 8.0,
            2.0,
            [0.96, 0.98, 1.0, 1.0],
        );
        let [width, height] = size.dimensions();
        queue_centred_text(
            renderer,
            &format!("{}K X {}K", width / 1_000, height / 1_000),
            button.centre[0],
            button.top() + 31.0,
            1.0,
            [0.62, 0.72, 0.84, 1.0],
        );
    }
    renderer.queue_text(
        "LEFT / RIGHT CHANGES SIZE",
        [
            layout.panel.left() + 30.0,
            creation.size_buttons[0].bottom() + 20.0,
        ],
        1.5,
        [0.50, 0.60, 0.75, 1.0],
    );
    renderer.queue_text(
        "WIDTH X DEPTH IN TILES",
        [
            layout.panel.left() + 30.0,
            creation.size_buttons[0].bottom() + 42.0,
        ],
        1.5,
        [0.62, 0.70, 0.82, 1.0],
    );
    if let Some(error) = &form.error {
        queue_centred_text(
            renderer,
            error,
            layout.panel.centre[0],
            creation.create.top() - 34.0,
            1.5,
            [1.0, 0.35, 0.30, 1.0],
        );
    }
    queue_button(
        renderer,
        creation.back,
        cursor,
        "BACK",
        [0.20, 0.25, 0.34, 1.0],
    );
    queue_button(
        renderer,
        creation.create,
        cursor,
        "CREATE",
        [0.10, 0.42, 0.25, 1.0],
    );
}

pub(super) fn queue_confirmation(
    renderer: &mut GuiRenderer,
    layout: MenuLayout,
    label: &str,
    cursor: [f32; 2],
) {
    queue_title(renderer, layout, "DELETE WORLD");
    queue_centred_text(
        renderer,
        "PERMANENTLY DELETE",
        layout.panel.centre[0],
        layout.panel.centre[1] - 80.0,
        2.0,
        [1.0, 0.55, 0.48, 1.0],
    );
    queue_centred_text(
        renderer,
        &truncate_label(label, 28),
        layout.panel.centre[0],
        layout.panel.centre[1] - 34.0,
        3.0,
        [1.0; 4],
    );
    queue_centred_text(
        renderer,
        "THIS CANNOT BE UNDONE",
        layout.panel.centre[0],
        layout.panel.centre[1] + 18.0,
        1.5,
        [0.75, 0.68, 0.68, 1.0],
    );
    let confirmation = ConfirmationLayout::new(layout.panel);
    queue_button(
        renderer,
        confirmation.cancel,
        cursor,
        "CANCEL",
        [0.20, 0.25, 0.34, 1.0],
    );
    queue_button(
        renderer,
        confirmation.delete,
        cursor,
        "DELETE",
        [0.52, 0.10, 0.11, 1.0],
    );
}

pub(super) fn queue_title(renderer: &mut GuiRenderer, layout: MenuLayout, title: &str) {
    queue_centred_text(
        renderer,
        title,
        layout.panel.centre[0],
        layout.panel.top() + 24.0,
        4.0,
        [0.80, 0.90, 1.0, 1.0],
    );
}

pub(super) fn queue_button(
    renderer: &mut GuiRenderer,
    button: Rect,
    cursor: [f32; 2],
    text: &str,
    colour: [f32; 4],
) {
    let tint = if button.contains(cursor) {
        [colour[0] * 1.3, colour[1] * 1.3, colour[2] * 1.3, colour[3]]
    } else {
        colour
    };
    renderer.queue_rect(button.centre, button.size, tint);
    let scale = if button.size[0] < 140.0 { 2.0 } else { 3.0 };
    queue_centred_text(
        renderer,
        text,
        button.centre[0],
        button.centre[1] - 3.5 * scale,
        scale,
        [1.0; 4],
    );
}

pub(super) fn queue_centred_text(
    renderer: &mut GuiRenderer,
    text: &str,
    centre_x: f32,
    top: f32,
    scale: f32,
    tint: [f32; 4],
) {
    let width = GuiRenderer::text_width(text, scale);
    renderer.queue_text(text, [centre_x - width * 0.5, top], scale, tint);
}

pub(super) fn truncate_label(label: &str, maximum_characters: usize) -> String {
    if label.chars().count() <= maximum_characters {
        return label.to_owned();
    }
    if maximum_characters <= 3 {
        return label.chars().take(maximum_characters).collect();
    }
    let mut truncated: String = label.chars().take(maximum_characters - 3).collect();
    truncated.push_str("...");
    truncated
}
