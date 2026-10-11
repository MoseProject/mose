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

use crate::keyframe::{self};

use super::effects::{ColorFilter, MaskShape};

use super::clip::ClipInstance;

impl ClipInstance {
    /// Builds this clip's crop/scale/rotation/opacity/color-balance keyframe avfilter fragment,
    /// spliced into the per-clip chain before [`ClipInstance::video_filter_chain`]'s own stages
    /// — the same position the old `zoom` stage used to occupy. `None` if none of the five are
    /// animated. Position keyframes aren't part of this — they apply to the *overlay*
    /// compositing stage, not a per-clip filter (see [`keyframe::position_overlay_xy_expr`] and
    /// `crate::render`). Crop keyframes run first (mirroring the static `crop` stage's own
    /// traditional "runs before every other effect" position in `video_filter_chain`, so a
    /// crop/pan animation reframes the source before scale/rotate/color-balance operate on it).
    /// Color-balance keyframes running here at all (rather than in their usual post-crop/
    /// deflicker/stabilization spot in `video_filter_chain`) is a real, narrow ordering caveat —
    /// see [`ClipInstance::brightness_keyframes`]'s doc comment.
    pub fn keyframe_video_filter_chain(
        &self,
        fps_num: u32,
        fps_den: u32,
        timeline_duration_secs: f64,
        zoompan_ok: bool,
    ) -> Option<String> {
        let mut stages = Vec::new();
        if self.has_crop_keyframes() {
            if let Some(crop) = keyframe::crop_filter_expr(
                &self.crop_x_keyframes,
                &self.crop_y_keyframes,
                &self.crop_w_keyframes,
                &self.crop_h_keyframes,
                self.crop_x,
                self.crop_y,
                self.crop_w,
                self.crop_h,
                fps_num,
                fps_den,
                timeline_duration_secs,
            ) {
                stages.push(crop);
            }
        }
        if let Some(scale) = keyframe::scale_filter_expr(
            &self.scale_keyframes,
            fps_num,
            fps_den,
            timeline_duration_secs,
            zoompan_ok,
        ) {
            stages.push(scale);
        }
        if let Some(angle_expr) =
            keyframe::rotation_filter_angle_expr(&self.rotation_keyframes, timeline_duration_secs)
        {
            stages.push(format!(
                "rotate=angle='{angle_expr}':ow=rotw('{angle_expr}'):oh=roth('{angle_expr}')"
            ));
        }
        if self.has_color_keyframes() {
            if let Some(color_stage) = keyframe::color_balance_filter_expr(
                &self.brightness_keyframes,
                &self.contrast_keyframes,
                &self.saturation_keyframes,
                self.brightness,
                self.contrast,
                self.saturation,
                timeline_duration_secs,
                self.id,
                fps_num as f64 / fps_den.max(1) as f64,
            ) {
                stages.push(color_stage);
            }
        }
        if let Some(alpha_expr) = keyframe::opacity_alpha_ramp_expr(
            &self.opacity_keyframes,
            fps_num,
            fps_den,
            timeline_duration_secs,
        ) {
            // The ramp does not depend on X or Y, so it is evaluated on a single pixel of a copy
            // of the stream (same frame numbers `N`), stretched back to the frame's size and
            // merged in as the alpha plane. A per-pixel `geq` over the whole frame is the same
            // result at about a fifteenth of the speed. Labels carry the clip id so two clips
            // in one graph do not collide.
            let id = self.id;
            stages.push(format!(
                "split=3[ko{id}a][ko{id}b][ko{id}c];\
                 [ko{id}b]scale=1:1:flags=neighbor,format=gray,geq=lum='255*({alpha_expr})'[ko{id}m];\
                 [ko{id}m][ko{id}c]scale=w=rw:h=rh:flags=neighbor[ko{id}s];\
                 [ko{id}a][ko{id}s]alphamerge"
            ));
        }
        if stages.is_empty() {
            None
        } else {
            Some(stages.join(","))
        }
    }

    /// Builds this clip's avfilter chain description for `core::render::render_timeline_export`
    /// — the subset of effect fields expressible as a static per-clip video filter (see
    /// `features/request.md`'s Fase 4 "Efeitos visuais" list): crop, deflicker, brightness/
    /// contrast/saturation, the black-and-white/sepia color filter, chroma key, mask shape,
    /// blur, sharpen, pixelize, shake, glitch, vignette, and horizontal flip. `gain_db` is audio, not
    /// video, and isn't part of this chain. `speed_factor` is handled in the native bridge;
    /// general keyframes are compiled separately by [`ClipInstance::keyframe_video_filter_chain`].
    /// `mask_shape`'s alpha only survives to the rendered
    /// output on an overlay track — see the caveat on its stage below. `transition_in` needs a
    /// materially different mechanism (cross-clip blending) and isn't covered here yet.
    /// `frozen` also needs a different mechanism (frame duplication) but is covered elsewhere —
    /// see [`ClipInstance::frozen`]'s doc.
    ///
    /// Returns `""` (no-op) if none of the covered effects deviate from neutral. Filters are
    /// comma-joined in a fixed order — crop first (so later filters see the cropped frame,
    /// not un-cropped coordinates), flip last (so it doesn't mirror crop/vignette geometry) —
    /// which is a judgment call, not something derivable from the field values themselves.
    pub fn video_filter_chain(&self) -> String {
        let mut stages = Vec::new();

        if self.is_cropped() && !self.has_crop_keyframes() {
            stages.push(format!(
                "crop=iw*{}:ih*{}:iw*{}:ih*{}",
                self.crop_w, self.crop_h, self.crop_x, self.crop_y
            ));
        }
        if self.deflicker_enabled {
            // Arithmetic-mean mode over a 5-frame temporal window (both FFmpeg's own
            // defaults) — smooths out frame-to-frame luminance variation from screen/gameplay
            // capture at certain refresh rates, before any other stage reshapes that luminance.
            stages.push("deflicker=mode=am:size=5".to_string());
        }
        if self.has_stabilization() {
            // rx/ry (search radius in pixels, deshake's valid range 0..64) scale with
            // intensity rather than a fixed radius — a small radius only corrects gentle
            // handheld wobble, a large one can also absorb bigger jolts, at the cost of more
            // aggressive cropping into the frame at the edges (deshake's own trade-off, not
            // something this stage compensates for). Runs right after deflicker, before any
            // color/stylistic stage reshapes the pixel data motion estimation reads.
            let radius = (4.0 + self.stabilization_intensity.clamp(0.0, 1.0) * 60.0).round() as i32;
            stages.push(format!("deshake=rx={radius}:ry={radius}:edge=mirror"));
        }
        if !self.has_color_keyframes()
            && (self.brightness != 0.0 || self.contrast != 1.0 || self.saturation != 1.0)
        {
            stages.extend(keyframe::color_balance_filter_expr(
                &[],
                &[],
                &[],
                self.brightness,
                self.contrast,
                self.saturation,
                0.0,
                self.id,
                0.0,
            ));
        }
        match self.color_filter {
            ColorFilter::None => {}
            ColorFilter::BlackAndWhite => stages.push("hue=s=0".to_string()),
            ColorFilter::Sepia => stages.push(
                "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131:0".to_string(),
            ),
        }
        if self.has_lut() {
            // Forward slashes even on Windows sidesteps avfilter's own backslash-escaping rules
            // inside a quoted option value (ffmpeg accepts `/`-separated paths on any platform);
            // a literal single quote in the path (the one character `'...'` quoting can't pass
            // through unescaped) is escaped avfilter-style.
            let escaped = self.lut_path.replace('\\', "/").replace('\'', "'\\''");
            stages.push(format!("lut3d=file='{escaped}'"));
        }
        if self.is_chroma_keyed() {
            let [r, g, b] = self.chroma_key_color;
            stages.push(format!(
                "colorkey=0x{r:02x}{g:02x}{b:02x}:{:.3}:0.1",
                self.chroma_key_tolerance
            ));
        }
        match self.mask_shape {
            MaskShape::None => {}
            MaskShape::Circle | MaskShape::RoundedRect => {
                // Promotes to an alpha-having pixel format, then geq's per-pixel expression
                // clips it to the mask shape by zeroing alpha() outside it — multiplying by
                // (rather than overwriting) the existing alpha(X,Y) so a chroma-keyed clip's
                // own transparency composes with the mask instead of being clobbered by it.
                // Note the same caveat as chroma_key's below: the final `format=yuv420p`
                // conform on a single (non-overlay) track drops this alpha again — the mask
                // only has a visible effect on a clip placed on an overlay track (see
                // `build_overlay_vfilter` in timeline_export_multi.c, which has no such conform between
                // its two per-track chains and the `overlay` filter that composites them).
                //
                // avfilter's filtergraph-level parser only treats a comma as a stage separator
                // outside quotes — every comma below sits inside the geq option's own `'...'`
                // quoting, so `pow`/`min`/`max`/`lte`'s comma-separated arguments are safe
                // (verified against a real ffmpeg build, not just read off the docs).
                let minwh = "min(W,H)";
                let alpha_expr = match self.mask_shape {
                    MaskShape::Circle => {
                        format!("alpha(X,Y)*lte(pow(X-W/2,2)+pow(Y-H/2,2),pow({minwh}/2,2))")
                    }
                    MaskShape::RoundedRect => {
                        // Rounded-rect signed-distance field: shrink the half-extents by the
                        // corner radius, measure how far outside that inner rect (X,Y) falls,
                        // then subtract the radius back out — <=0 is inside the rounded shape.
                        let radius = format!(
                            "min({:.4}*{minwh},{minwh}/2)",
                            self.mask_corner_radius.clamp(0.0, 1.0)
                        );
                        format!(
                            "alpha(X,Y)*lte(sqrt(pow(max(abs(X-W/2)-(W/2-{radius}),0),2)+pow(max(abs(Y-H/2)-(H/2-{radius}),0),2))-{radius},0)"
                        )
                    }
                    MaskShape::None => unreachable!("outer match already excludes None"),
                };
                if self.is_chroma_keyed() || !self.opacity_keyframes.is_empty() {
                    // The shape has to multiply an alpha the frame already carries.
                    stages.push(format!(
                        "format=yuva420p,geq=lum='p(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='{alpha_expr}'"
                    ));
                } else {
                    // A shape depends only on the frame's size, so it is drawn once, on the first
                    // frame, and merged in as the alpha plane of every frame (`alphamerge` repeats
                    // the last frame of its second input). Identical to the per-pixel `geq` over
                    // every frame, which is more than ten times slower. The plane is taken with
                    // `extractplanes`, not `format=gray`: a gray constraint on this branch would
                    // travel back through `split` and turn the picture itself gray.
                    let id = self.id;
                    let mask_expr = alpha_expr.replace("alpha(X,Y)*", "255*");
                    stages.push(format!(
                        "split[km{id}a][km{id}b];\
                         [km{id}b]trim=end_frame=1,geq=lum='{mask_expr}',extractplanes=y[km{id}m];\
                         [km{id}a][km{id}m]alphamerge"
                    ));
                }
            }
        }
        if self.blur_intensity > 0.0 {
            // avgblur is boxblur's equivalent (a box of 2r+1 pixels); boxblur is missing from the
            // shipped LGPL FFmpeg. Radius truncated to whole pixels, as the preview does.
            let radius = (self.blur_intensity * 10.0) as i32;
            if radius > 0 {
                stages.push(format!("avgblur=sizeX={radius}:sizeY={radius}"));
            }
        }
        if self.sharpen > 0.0 {
            stages.push(format!("unsharp=5:5:{:.2}:5:5:0.0", self.sharpen * 3.0));
        }
        if self.pixelize_intensity > 0.0 {
            // Scale down to block_size-pixel grid then back up — nearest-neighbor gives the
            // hard mosaic look. block ranges from 2px (subtle) to 50px (heavy censorship).
            let block = (2.0 + self.pixelize_intensity * 48.0).round() as u32;
            stages.push(format!(
                "scale=iw/{b}:ih/{b}:flags=neighbor,scale=iw*{b}:ih*{b}:flags=neighbor",
                b = block
            ));
        }
        if self.shake_intensity > 0.0 {
            // Crop away a margin on all sides (giving room to "shake" into), oscillate the crop
            // origin with two independent sinusoids, then scale back to the original frame size.
            // margin at full intensity: 8% of each dimension per side.
            let margin = self.shake_intensity * 0.08_f32;
            let keep = 1.0_f32 - 2.0 * margin;
            let scale_back = 1.0 / keep;
            stages.push(format!(
                "crop=iw*{keep:.4}:ih*{keep:.4}:iw*{m:.4}*(1+sin(n*0.31)):ih*{m:.4}*(1+cos(n*0.23)),scale=iw*{sb:.4}:ih*{sb:.4}",
                m = margin,
                sb = scale_back,
            ));
        }
        if self.glitch_intensity > 0.0 {
            // Temporal luma + chroma noise approximates digital glitch corruption.
            let ls = (self.glitch_intensity * 60.0).round() as u32;
            let cs = (self.glitch_intensity * 25.0).round() as u32;
            stages.push(format!(
                "noise=c0s={ls}:c0f=t:c1s={cs}:c1f=t:c2s={cs}:c2f=t"
            ));
        }
        if self.has_vignette() {
            stages.push(format!("vignette=PI/4*{:.3}", self.vignette_intensity));
        }
        if self.flipped_h {
            stages.push("hflip".to_string());
        }

        stages.join(",")
    }
}
