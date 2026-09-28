// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use avcore::timeline::ShapePreset;
use eframe::egui::{self, RichText};

use crate::app::App;
use crate::components;
use crate::i18n::Text;
use crate::theme;

fn shape_preset_label(preset: ShapePreset, locale: crate::i18n::Locale) -> String {
    match preset {
        ShapePreset::Ellipse => Text::ShapePresetEllipse.tr(locale).to_string(),
        ShapePreset::Rectangle => Text::ShapePresetRectangle.tr(locale).to_string(),
        ShapePreset::Triangle => Text::ShapePresetTriangle.tr(locale).to_string(),
        ShapePreset::Trapezoid => Text::ShapePresetTrapezoid.tr(locale).to_string(),
        ShapePreset::Arrow => Text::ShapePresetArrow.tr(locale).to_string(),
        ShapePreset::Custom => Text::ShapePresetCustom.tr(locale).to_string(),
    }
}

#[cfg(test)]
#[path = "shape_clip/shape_clip_test.rs"]
mod tests;

/// Renders the properties panel content for a selected shape overlay clip. Shows controls for
/// the shape preset, RGBA color, center X/Y, width/height, rotation, outline thickness, start
/// time, and duration. Applies changes immediately by mutating the clip through the active
/// project's timeline — mirrors [`text_clip_properties`]'s clone-mutate-writeback shape.
pub(super) fn shape_clip_properties(
    app: &mut App,
    ui: &mut egui::Ui,
    sc_id: u64,
    locale: crate::i18n::Locale,
) {
    components::section_label(ui, Text::SelectedShapeClip.tr(locale));

    let current = app
        .active_project()
        .timeline()
        .tracks
        .iter()
        .filter(|t| t.kind == avcore::timeline::TrackKind::Shape)
        .flat_map(|t| &t.shape_clips)
        .find(|sc| sc.id == sc_id)
        .cloned();

    let Some(mut sc) = current else {
        ui.label(RichText::new(Text::NoShapeClipSelected.tr(locale)).color(theme::TEXT_MUTED));
        return;
    };

    let mut changed = false;
    let mut structural_changed = false;

    // Shape preset
    components::property_row(ui, Text::PropShapeKind.tr(locale));
    let mut preset = ShapePreset::from_kind(&sc.shape_kind);
    let mut preset_options = ShapePreset::PICKABLE.to_vec();
    if preset == ShapePreset::Custom {
        preset_options.push(ShapePreset::Custom);
    }
    if components::enum_combo(ui, "shape_kind", &preset_options, &mut preset, |p| {
        shape_preset_label(p, locale)
    }) {
        // `Custom` has no fixed geometry (and is only listed when already current, so picking
        // it again isn't a change `enum_combo` reports) -- keep the existing polygon.
        if let Some(kind) = preset.to_kind() {
            sc.shape_kind = kind;
        }
        changed = true;
    }
    ui.add_space(4.0);

    // Vertex editor — every preset except Ellipse is already a Polygon under the hood (see
    // ShapeKind::rectangle() etc.), so this lets the user hand-edit any of them into a custom
    // shape, not just a dedicated "Custom" starting point.
    let polygon_vertices = match &sc.shape_kind {
        avcore::timeline::ShapeKind::Polygon(vertices) => Some(vertices.clone()),
        avcore::timeline::ShapeKind::Ellipse => None,
    };
    if let Some(vertices) = polygon_vertices {
        let mut new_vertices = None;
        if components::property_section(
            ui,
            sc_id,
            Text::PropShapeVertices.tr(locale),
            Text::ShapeVerticesHint.tr(locale),
            true,
            |ui| {
                new_vertices =
                    super::keyframe_editors::polygon_vertex_editor(ui, &vertices, locale);
                new_vertices.is_some()
            },
        ) {
            if let Some(v) = new_vertices {
                sc.shape_kind = avcore::timeline::ShapeKind::Polygon(v);
                changed = true;
            }
        }
    }

    // Color picker (RGBA — alpha controlled via the color picker's alpha channel).
    ui.horizontal(|ui| {
        components::property_row(ui, Text::PropShapeColor.tr(locale));
        let mut color = egui::Color32::from_rgba_premultiplied(
            sc.color_rgba[0],
            sc.color_rgba[1],
            sc.color_rgba[2],
            sc.color_rgba[3],
        );
        if ui.color_edit_button_srgba(&mut color).changed() {
            sc.color_rgba = [color.r(), color.g(), color.b(), color.a()];
            changed = true;
        }
    });

    // Center position
    components::property_row(ui, Text::PropShapePosX.tr(locale));
    if ui
        .add(
            egui::Slider::new(&mut sc.center_x, 0.0..=1.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        )
        .changed()
    {
        changed = true;
    }
    components::property_row(ui, Text::PropShapePosY.tr(locale));
    if ui
        .add(
            egui::Slider::new(&mut sc.center_y, 0.0..=1.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        )
        .changed()
    {
        changed = true;
    }

    // Center position keyframes -- animate a pan/reveal over the shape's own on-timeline
    // duration, overriding center_x/center_y above when non-empty (see
    // ShapeClip::center_x_keyframes' doc comment).
    let mut new_center_x_keyframes = None;
    if components::property_section(
        ui,
        sc_id,
        Text::PropShapePosXKeyframes.tr(locale),
        Text::ShapePositionKeyframesExportNote.tr(locale),
        !sc.center_x_keyframes.is_empty(),
        |ui| {
            new_center_x_keyframes = super::keyframe_editors::f32_keyframe_editor(
                ui,
                &sc.center_x_keyframes,
                0.0..=1.0,
                0.5,
                locale,
            );
            new_center_x_keyframes.is_some()
        },
    ) {
        if let Some(kfs) = new_center_x_keyframes {
            sc.center_x_keyframes = kfs;
            changed = true;
        }
    }
    let mut new_center_y_keyframes = None;
    if components::property_section(
        ui,
        sc_id,
        Text::PropShapePosYKeyframes.tr(locale),
        Text::ShapePositionKeyframesExportNote.tr(locale),
        !sc.center_y_keyframes.is_empty(),
        |ui| {
            new_center_y_keyframes = super::keyframe_editors::f32_keyframe_editor(
                ui,
                &sc.center_y_keyframes,
                0.0..=1.0,
                0.5,
                locale,
            );
            new_center_y_keyframes.is_some()
        },
    ) {
        if let Some(kfs) = new_center_y_keyframes {
            sc.center_y_keyframes = kfs;
            changed = true;
        }
    }

    // Size
    components::property_row(ui, Text::PropShapeWidth.tr(locale));
    if ui
        .add(
            egui::Slider::new(&mut sc.width, 0.01..=1.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        )
        .changed()
    {
        changed = true;
    }
    components::property_row(ui, Text::PropShapeHeight.tr(locale));
    if ui
        .add(
            egui::Slider::new(&mut sc.height, 0.01..=1.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        )
        .changed()
    {
        changed = true;
    }

    // Size keyframes -- animate a grow/shrink over the shape's own on-timeline duration,
    // overriding width/height above when non-empty (see ShapeClip::width_keyframes' doc
    // comment).
    let mut new_width_keyframes = None;
    if components::property_section(
        ui,
        sc_id,
        Text::PropShapeWidthKeyframes.tr(locale),
        Text::ShapeSizeKeyframesExportNote.tr(locale),
        !sc.width_keyframes.is_empty(),
        |ui| {
            new_width_keyframes = super::keyframe_editors::f32_keyframe_editor(
                ui,
                &sc.width_keyframes,
                0.01..=1.0,
                sc.width,
                locale,
            );
            new_width_keyframes.is_some()
        },
    ) {
        if let Some(kfs) = new_width_keyframes {
            sc.width_keyframes = kfs;
            changed = true;
        }
    }
    let mut new_height_keyframes = None;
    if components::property_section(
        ui,
        sc_id,
        Text::PropShapeHeightKeyframes.tr(locale),
        Text::ShapeSizeKeyframesExportNote.tr(locale),
        !sc.height_keyframes.is_empty(),
        |ui| {
            new_height_keyframes = super::keyframe_editors::f32_keyframe_editor(
                ui,
                &sc.height_keyframes,
                0.01..=1.0,
                sc.height,
                locale,
            );
            new_height_keyframes.is_some()
        },
    ) {
        if let Some(kfs) = new_height_keyframes {
            sc.height_keyframes = kfs;
            changed = true;
        }
    }

    // Rotation
    components::property_row(ui, Text::PropShapeRotation.tr(locale));
    if ui
        .add(egui::Slider::new(&mut sc.rotation_deg, 0.0..=360.0).suffix(" deg"))
        .changed()
    {
        changed = true;
    }

    // Rotation keyframes -- animate a spin over the shape's own on-timeline duration,
    // overriding rotation_deg above when non-empty (see ShapeClip::rotation_keyframes' doc
    // comment).
    let mut new_rotation_keyframes = None;
    if components::property_section(
        ui,
        sc_id,
        Text::PropShapeRotationKeyframes.tr(locale),
        Text::ShapeRotationKeyframesExportNote.tr(locale),
        !sc.rotation_keyframes.is_empty(),
        |ui| {
            new_rotation_keyframes = super::keyframe_editors::f32_keyframe_editor(
                ui,
                &sc.rotation_keyframes,
                0.0..=360.0,
                sc.rotation_deg,
                locale,
            );
            new_rotation_keyframes.is_some()
        },
    ) {
        if let Some(kfs) = new_rotation_keyframes {
            sc.rotation_keyframes = kfs;
            changed = true;
        }
    }

    // Outline thickness
    components::property_row(ui, Text::PropShapeStroke.tr(locale));
    if ui
        .add(
            egui::DragValue::new(&mut sc.stroke_thickness_px)
                .range(0.0..=f32::MAX)
                .speed(0.5)
                .suffix(" px"),
        )
        .changed()
    {
        changed = true;
    }
    ui.label(
        RichText::new(Text::PropShapeStrokeHint.tr(locale))
            .size(10.0)
            .color(theme::TEXT_MUTED),
    );

    // Start and duration
    components::property_row(ui, Text::PropShapeStart.tr(locale));
    if ui
        .add(
            egui::DragValue::new(&mut sc.start_secs)
                .range(0.0..=f64::MAX)
                .speed(0.1)
                .suffix(" s"),
        )
        .changed()
    {
        changed = true;
        structural_changed = true;
    }
    components::property_row(ui, Text::PropShapeDuration.tr(locale));
    if ui
        .add(
            egui::DragValue::new(&mut sc.duration_secs)
                .range(0.1..=f64::MAX)
                .speed(0.1)
                .suffix(" s"),
        )
        .changed()
    {
        changed = true;
        structural_changed = true;
    }

    ui.add_space(4.0);
    ui.label(
        RichText::new(Text::ShapeExportNote.tr(locale))
            .size(10.5)
            .color(theme::TEXT_MUTED),
    );

    // Apply changes back to the clip in the active project. `start_secs`/`duration_secs`
    // (structural_changed) can change which clips cover the playhead, so those still force a
    // full pipeline reopen via `ensure_preview_loaded`'s normal id-diffing path. Every other
    // field here (kind/color/position/size/rotation/stroke) only changes this clip's own
    // rasterized look — if it's already part of the currently open composited preview,
    // `refresh_preview_shape_content` pushes a fresh buffer into its existing `appsrc` branch
    // instead of tearing down and rebuilding the whole GStreamer pipeline on every dragged
    // slider frame.
    if changed {
        app.push_undo_snapshot_for_drag();
        let timeline = app.active_project_mut().timeline_mut();
        for track in &mut timeline.tracks {
            if track.kind == avcore::timeline::TrackKind::Shape {
                if let Some(existing) = track.shape_clips.iter_mut().find(|c| c.id == sc_id) {
                    *existing = sc;
                    break;
                }
            }
        }
        if structural_changed {
            app.invalidate_preview_rendering();
        } else {
            app.refresh_preview_shape_content(sc_id);
        }
    }
}
