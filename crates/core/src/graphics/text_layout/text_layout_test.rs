use super::*;

#[test]
fn try_shape_rejects_unbounded_or_nonfinite_inputs() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let too_long = "a".repeat(MAX_TEXT_LAYOUT_BYTES + 1);
    assert!(matches!(
        engine.try_shape(
            &too_long,
            TextFontFamily::Lato,
            TextFontStyle::Regular,
            None,
            32.0,
            None,
            (0.0, 0.0),
            TextDirection::Auto,
            TextAlign::Auto,
        ),
        Err(TextLayoutError::TextTooLong)
    ));
    assert!(matches!(
        engine.try_shape(
            "caption",
            TextFontFamily::Lato,
            TextFontStyle::Regular,
            None,
            f32::NAN,
            Some(640.0),
            (0.0, 0.0),
            TextDirection::Auto,
            TextAlign::Auto,
        ),
        Err(TextLayoutError::InvalidGeometry)
    ));
}

#[test]
fn origin_shifts_every_glyph_by_exactly_that_offset() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let at_zero = engine.shape(
        "Hi",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let shifted = engine.shape(
        "Hi",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (10.0, 5.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let g0 = &at_zero.lines[0].glyphs[0];
    let g1 = &shifted.lines[0].glyphs[0];
    assert!((g1.x - g0.x - 10.0).abs() < 0.01);
    assert!((g1.y - g0.y - 5.0).abs() < 0.01);
    // The physical (rasterization-ready) integer position must also reflect the shift, since
    // overlay_render.rs paints from `physical.x`/`physical.y`, not from the float `x`/`y` fields.
    assert!(g1.physical.x > g0.physical.x);
}

#[test]
fn with_shared_engine_reuses_the_same_engine_across_calls() {
    let count_a = with_shared_engine(|engine, _cache| engine.loaded_face_count());
    let count_b = with_shared_engine(|engine, _cache| engine.loaded_face_count());
    assert_eq!(count_a, count_b);
    assert_eq!(count_a, font_catalog::locked_face_bytes().len());
}

#[test]
fn loads_exactly_the_locked_catalog_faces_and_nothing_else() {
    let engine = TextLayoutEngine::new_from_locked_catalog();
    assert_eq!(
        engine.loaded_face_count(),
        font_catalog::locked_face_bytes().len(),
        "a system font leaked into the database, or a locked face failed to load"
    );
}

#[test]
fn every_bundled_family_shapes_ordinary_latin_text_without_missing_glyphs() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    for family in TextFontFamily::ALL {
        let shaped = engine.shape(
            "Ação, coração, você — 0123456789",
            family.clone(),
            TextFontStyle::Regular,
            None,
            32.0,
            None,
            (0.0, 0.0),
            TextDirection::Auto,
            TextAlign::Auto,
        );
        assert!(
            shaped.glyph_count() > 0,
            "{family:?} produced no glyphs at all"
        );
        // glyph_id 0 is the .notdef glyph in every OpenType font -- a hit here means this
        // family's bundled file is missing coverage for the GF Latin Core acceptance corpus
        // characters the font_catalog doc requires (spec/architecture/built-in-font-catalog.md).
        let notdef_hits = shaped
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .filter(|g| g.glyph_id == 0)
            .count();
        assert_eq!(notdef_hits, 0, "{family:?} hit .notdef on the Latin corpus");
    }
}

#[test]
fn fi_and_fl_ligatures_form_a_single_multi_character_cluster() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let text = "difficult waffle";
    let shaped = engine.shape(
        text,
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let has_multi_char_cluster = shaped
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter())
        .any(|g| g.cluster.end - g.cluster.start > 1);
    assert!(
        has_multi_char_cluster,
        "expected at least one ligature cluster spanning more than one source character"
    );
    // Fewer glyphs than characters is the real proof a ligature merged two characters into one
    // glyph, not just that a multi-byte cluster range exists.
    let glyph_count = shaped.glyph_count();
    assert!(
        glyph_count < text.chars().count(),
        "glyph_count={glyph_count} did not shrink below char_count={}",
        text.chars().count()
    );
}

#[test]
fn bold_style_selects_a_different_font_face_than_regular_for_a_two_weight_family() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    assert!(TextFontFamily::Lato.supports_bold());
    let regular = engine.shape(
        "Test",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let bold = engine.shape(
        "Test",
        TextFontFamily::Lato,
        TextFontStyle::Bold,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let regular_font_id = regular.lines[0].glyphs[0].font_id;
    let bold_font_id = bold.lines[0].glyphs[0].font_id;
    assert_ne!(
        regular_font_id, bold_font_id,
        "Lato Bold should resolve to a different loaded face than Lato Regular"
    );
}

#[test]
fn a_variable_family_renders_measurably_wider_glyphs_at_a_higher_weight() {
    // TEXT-01C: an explicit `wght` axis override reaches cosmic-text's own variable-font
    // instancing (verified for real against the bundled Inter file in a scratch crate this
    // session, see spec/ROADMAP.md's TEXT-01C entry) -- this asserts the same real effect
    // inline, so a future regression in the plumbing (not the underlying library, already
    // proven) is caught here too.
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let light = engine.shape(
        "Weight",
        TextFontFamily::Inter,
        TextFontStyle::Regular,
        Some(100),
        48.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let heavy = engine.shape(
        "Weight",
        TextFontFamily::Inter,
        TextFontStyle::Regular,
        Some(900),
        48.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    // Same underlying variable font file both times -- proves genuine axis instancing of one
    // face, not a silent fallback to two different static faces.
    assert_eq!(
        light.lines[0].glyphs[0].font_id,
        heavy.lines[0].glyphs[0].font_id
    );
    assert!(
        heavy.width > light.width,
        "expected weight 900 ({}) to render wider than weight 100 ({})",
        heavy.width,
        light.width
    );
}

#[test]
fn no_weight_override_keeps_the_existing_regular_bold_behavior() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let via_style = engine.shape(
        "Weight",
        TextFontFamily::Lato,
        TextFontStyle::Bold,
        None,
        48.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let via_explicit_700 = engine.shape(
        "Weight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        Some(700),
        48.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    assert_eq!(via_style.width, via_explicit_700.width);
}

#[test]
fn a_static_familys_weight_override_never_produces_an_empty_shape() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let shaped = engine.shape(
        "Weight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        Some(550),
        48.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    assert!(!shaped.lines[0].glyphs.is_empty());
}

#[test]
fn single_weight_family_ignores_bold_request_without_producing_notdef() {
    // Bebas Neue only ships one designed weight -- requesting Bold must still shape cleanly by
    // falling back to the one available face, not fail or emit .notdef.
    assert!(!TextFontFamily::BebasNeue.supports_bold());
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let shaped = engine.shape(
        "Title",
        TextFontFamily::BebasNeue,
        TextFontStyle::Bold,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    assert!(shaped.glyph_count() > 0);
    assert!(shaped
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter())
        .all(|g| g.glyph_id != 0));
}

fn shape_has_any_notdef(text: &str, family: TextFontFamily) -> bool {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let shaped = engine.shape(
        text,
        family,
        TextFontStyle::Regular,
        None,
        48.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    shaped
        .lines
        .iter()
        .flat_map(|l| l.glyphs.iter())
        .any(|g| g.glyph_id == 0)
}

#[test]
fn oca_fallback_shapes_embedded_arabic_through_a_latin_family_with_no_notdef() {
    // Lato has zero Arabic coverage -- OcaFallback's own `script_fallback` points this cluster
    // at the bundled "Noto Sans Arabic" face instead.
    assert!(!shape_has_any_notdef("مرحبا", TextFontFamily::Lato));
}

#[test]
fn oca_fallback_shapes_embedded_hebrew_through_a_latin_family_with_no_notdef() {
    assert!(!shape_has_any_notdef("שלום", TextFontFamily::Lato));
}

#[test]
fn oca_fallback_shapes_embedded_devanagari_through_a_latin_family_with_no_notdef() {
    assert!(!shape_has_any_notdef("नमस्ते", TextFontFamily::Lato));
}

#[test]
fn oca_fallback_shapes_embedded_bengali_through_a_latin_family_with_no_notdef() {
    assert!(!shape_has_any_notdef("নমস্কার", TextFontFamily::Lato));
}

#[test]
fn oca_fallback_shapes_embedded_tamil_through_a_latin_family_with_no_notdef() {
    assert!(!shape_has_any_notdef("வணக்கம்", TextFontFamily::Lato));
}

#[test]
fn oca_fallback_shapes_embedded_thai_through_a_latin_family_with_no_notdef() {
    assert!(!shape_has_any_notdef("สวัสดี", TextFontFamily::Lato));
}

#[test]
fn oca_fallback_handles_mixed_latin_and_arabic_in_one_string() {
    assert!(!shape_has_any_notdef(
        "Hello مرحبا World",
        TextFontFamily::Lato
    ));
}

#[test]
fn oca_fallback_resolves_arabic_to_the_same_face_as_selecting_it_directly() {
    // The real value OcaFallback adds over cosmic-text's own undirected last-resort "try every
    // other loaded font" fallback tier: a deterministic, name-directed match to exactly the
    // intended "Noto Sans Arabic" face, confirmed here by comparing resolved font ids -- not
    // merely "some font in the catalog happened to not-notdef this text" (see this test module's
    // own scratch-crate verification note for the real control that found cosmic-text's
    // undirected tier already avoids notdef by exhaustive trial in this small a catalog, which
    // this determinism check is what actually distinguishes OcaFallback from doing nothing).
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let via_lato_fallback = engine
        .shape(
            "مرحبا",
            TextFontFamily::Lato,
            TextFontStyle::Regular,
            None,
            48.0,
            None,
            (0.0, 0.0),
            TextDirection::Auto,
            TextAlign::Auto,
        )
        .lines[0]
        .glyphs[0]
        .font_id;
    let via_direct_pick = engine
        .shape(
            "مرحبا",
            TextFontFamily::NotoSansArabic,
            TextFontStyle::Regular,
            None,
            48.0,
            None,
            (0.0, 0.0),
            TextDirection::Auto,
            TextAlign::Auto,
        )
        .lines[0]
        .glyphs[0]
        .font_id;
    assert_eq!(via_lato_fallback, via_direct_pick);
}

#[test]
fn text_width_px_is_zero_for_empty_text_and_positive_for_nonempty_text() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let empty = engine.text_width_px("", TextFontFamily::Lato, TextFontStyle::Regular, 32.0);
    let nonempty = engine.text_width_px(
        "PacoPaçoca",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        32.0,
    );
    assert_eq!(empty, 0.0);
    assert!(nonempty > 0.0);
}

#[test]
fn text_width_px_grows_with_longer_text() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let short = engine.text_width_px("Paco", TextFontFamily::Lato, TextFontStyle::Regular, 32.0);
    let long = engine.text_width_px(
        "PacoPaçoca gameplay highlights",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        32.0,
    );
    assert!(long > short);
}

#[test]
fn wrapping_at_a_narrow_width_produces_more_than_one_line() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let shaped = engine.shape(
        "This is a fairly long caption that should wrap across several lines",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        24.0,
        Some(120.0),
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    assert!(
        shaped.lines.len() > 1,
        "expected wrapping to produce multiple lines, got {}",
        shaped.lines.len()
    );
    for line in &shaped.lines {
        assert!(
            line.width <= 130.0,
            "line width {} exceeded the requested wrap width by more than a reasonable slop",
            line.width
        );
    }
}

#[test]
fn unwrapped_shaping_produces_exactly_one_line_for_single_line_text() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let shaped = engine.shape(
        "One short line",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    assert_eq!(shaped.lines.len(), 1);
}

#[test]
fn forced_direction_never_leaks_the_bidi_mark_into_the_glyph_list_or_cluster_ranges() {
    // Same real proof `fi_and_fl_ligatures_form_a_single_multi_character_cluster` uses for
    // ligatures: comparing shaped output against Auto for the exact same source text is a real
    // check the internal LRM/RLM prefix this override uses is fully invisible to callers, not
    // just that the byte-offset arithmetic type-checks.
    //
    // Real finding from running this against actual cosmic-text: an RTL-leveled run's glyphs
    // come back from `layout_runs()` in HarfBuzz's own RTL shaping-buffer order (rightmost pen
    // position first, advancing leftward) — the exact reverse of Auto/Ltr's iteration order for
    // the same text, even though every glyph's *final pixel position* still reads correctly left
    // to right. So this compares cluster *sets* (order-independent), not iteration order, and
    // separately proves final positions stay in reading order below.
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let text = "PacoPaçoca";
    let auto = engine.shape(
        text,
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let ltr = engine.shape(
        text,
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Ltr,
        TextAlign::Auto,
    );
    let rtl = engine.shape(
        text,
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Rtl,
        TextAlign::Auto,
    );
    for shaped in [&auto, &ltr, &rtl] {
        assert_eq!(
            shaped.glyph_count(),
            auto.glyph_count(),
            "forcing a direction must not drop or add a glyph for the caller's own text"
        );
    }
    let mut auto_clusters: Vec<_> = auto.lines[0]
        .glyphs
        .iter()
        .map(|g| g.cluster.clone())
        .collect();
    let mut ltr_clusters: Vec<_> = ltr.lines[0]
        .glyphs
        .iter()
        .map(|g| g.cluster.clone())
        .collect();
    let mut rtl_clusters: Vec<_> = rtl.lines[0]
        .glyphs
        .iter()
        .map(|g| g.cluster.clone())
        .collect();
    for clusters in [&mut auto_clusters, &mut ltr_clusters, &mut rtl_clusters] {
        clusters.sort_by_key(|c| c.start);
    }
    assert_eq!(
        auto_clusters, ltr_clusters,
        "the set of cluster byte ranges must be relative to the caller's text, not the internal LRM-prefixed one"
    );
    assert_eq!(
        auto_clusters, rtl_clusters,
        "the set of cluster byte ranges must be relative to the caller's text, not the internal RLM-prefixed one"
    );
    // No glyph's cluster should ever reach beyond the caller's own text length.
    for shaped in [&auto, &ltr, &rtl] {
        for glyph in &shaped.lines[0].glyphs {
            assert!(glyph.cluster.end <= text.len());
        }
    }
    // Forcing Rtl still must not scramble the actual rendered pixels: sorted by logical cluster
    // start, each glyph's final `x` position must still increase monotonically, proving a pure
    // left-to-right script reads correctly left to right even when the paragraph itself is
    // anchored/ordered as RTL (it lands as one right-anchored block, not character soup).
    let mut by_cluster_start: Vec<_> = rtl.lines[0].glyphs.iter().collect();
    by_cluster_start.sort_by_key(|g| g.cluster.start);
    for pair in by_cluster_start.windows(2) {
        assert!(
            pair[1].x > pair[0].x,
            "RTL-forced Latin text must still render each character left-to-right in final pixel position"
        );
    }
}

#[test]
fn forced_direction_does_not_panic_on_empty_text() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    for direction in [TextDirection::Auto, TextDirection::Ltr, TextDirection::Rtl] {
        let shaped = engine.shape(
            "",
            TextFontFamily::Lato,
            TextFontStyle::Regular,
            None,
            32.0,
            None,
            (0.0, 0.0),
            direction,
            TextAlign::Auto,
        );
        assert_eq!(shaped.glyph_count(), 0);
    }
}

#[test]
fn auto_detects_a_pure_hebrew_paragraph_as_rtl() {
    // No Hebrew face is bundled yet (TEXT-01C) so this only checks the computed bidi level, not
    // glyph coverage -- `unicode-bidi` resolves levels straight from the Unicode text,
    // independent of which font ends up rasterizing (or missing) the glyph.
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let hebrew = "שלום";
    let auto = engine.shape(
        hebrew,
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    assert!(
        !auto.lines[0].glyphs.is_empty(),
        "expected at least one glyph even without a bundled Hebrew face"
    );
    assert!(
        auto.lines[0].glyphs.iter().all(|g| g.rtl),
        "Hebrew text should auto-detect as an RTL paragraph"
    );
}

#[test]
fn forced_rtl_direction_flips_which_side_of_the_line_each_scripts_run_lands_on() {
    // A real proof the override actually changes paragraph-level *run ordering* (TEXT-01B's own
    // goal 2), not just that it compiles or that it's a no-op on a single-script string: a
    // strong-R Hebrew character (bidi class R) forces the paragraph to Auto-detect RTL on its
    // own already whenever it leads, so this instead starts with a Latin-leading mixed string --
    // Auto picks LTR from the leading "AB", forcing Rtl overrides that same P2/P3 detection.
    // Individual per-character bidi levels correctly stay intrinsic to each script either way
    // (UAX #9 I1/I2 -- a strong-R character is always an odd/RTL level regardless of paragraph
    // base direction), so what actually flips between the two runs is *which side of the line*
    // each script's run lands on.
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let text = "AB שב"; // Latin "AB", a space, two Hebrew letters
    let auto = engine.shape(
        text,
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let rtl = engine.shape(
        text,
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Rtl,
        TextAlign::Auto,
    );

    let mean_x = |shaped: &ShapedText, rtl: bool| -> f32 {
        let xs: Vec<f32> = shaped.lines[0]
            .glyphs
            .iter()
            .filter(|g| g.rtl == rtl)
            .map(|g| g.x)
            .collect();
        assert!(!xs.is_empty());
        xs.iter().sum::<f32>() / xs.len() as f32
    };

    let auto_latin_x = mean_x(&auto, false);
    let auto_hebrew_x = mean_x(&auto, true);
    let rtl_latin_x = mean_x(&rtl, false);
    let rtl_hebrew_x = mean_x(&rtl, true);

    assert!(
        auto_latin_x < auto_hebrew_x,
        "Auto (LTR-detected from the leading Latin run) should place Latin left of Hebrew"
    );
    assert!(
        rtl_latin_x > rtl_hebrew_x,
        "forcing Rtl should place the Latin run right of the Hebrew run instead"
    );
}

#[test]
fn scan_bidi_controls_is_clean_for_plain_text() {
    let warning = scan_bidi_controls("Just an ordinary caption, nothing hidden here.");
    assert!(!warning.any());
    assert!(!warning.has_override);
    assert!(!warning.has_unmatched_control);
}

#[test]
fn scan_bidi_controls_flags_a_well_formed_override() {
    // A perfectly matched RLO...PDF pair still forces every character between them to render
    // right-to-left regardless of script -- worth a warning on its own, per the architecture
    // doc's own "RLO/LRO ... receive a non-blocking warning because they can make text visually
    // misleading" note, even when well-formed.
    let warning = scan_bidi_controls("safe\u{202E}txt.exe\u{202C}");
    assert!(warning.has_override);
    assert!(!warning.has_unmatched_control);
    assert!(warning.any());
}

#[test]
fn scan_bidi_controls_flags_an_unmatched_embedding() {
    let warning = scan_bidi_controls("open\u{202A}never closed");
    assert!(!warning.has_override);
    assert!(warning.has_unmatched_control);
}

#[test]
fn scan_bidi_controls_flags_a_stray_close_with_nothing_open() {
    let warning = scan_bidi_controls("nothing open here\u{202C}");
    assert!(warning.has_unmatched_control);
}

#[test]
fn scan_bidi_controls_flags_an_unmatched_isolate() {
    let warning = scan_bidi_controls("\u{2066}never closed");
    assert!(warning.has_unmatched_control);
}

#[test]
fn scan_bidi_controls_accepts_properly_nested_isolates_and_embeddings() {
    let warning = scan_bidi_controls("a\u{2066}b\u{202A}c\u{202C}d\u{2069}e");
    assert!(!warning.any());
}

#[test]
fn scan_bidi_controls_a_stray_pdf_does_not_incorrectly_close_an_outer_isolate() {
    // A PDF with no matching embedding open must be ignored (UAX #9 rule X7's own "if there is
    // no matching code, do nothing"), not accidentally pop the isolate underneath it -- so the
    // isolate's own later PDI still matches correctly and this is NOT reported as unmatched
    // beyond the stray PDF itself.
    let warning = scan_bidi_controls("\u{2066}open isolate\u{202C}stray pdf\u{2069}");
    assert!(
        warning.has_unmatched_control,
        "the stray PDF itself should still be flagged"
    );

    // Confirm the isolate really did stay open for its own PDI by checking a case with no stray
    // control at all resolves clean.
    let clean = scan_bidi_controls("\u{2066}open isolate\u{2069}");
    assert!(!clean.any());
}

#[test]
fn scan_bidi_controls_left_to_right_and_right_to_left_marks_are_not_flagged() {
    // LRM/RLM (this module's own TextDirection-forcing prefix marks, see LEFT_TO_RIGHT_MARK/
    // RIGHT_TO_LEFT_MARK) are plain strong-directional characters, not explicit formatting
    // controls -- they must never trigger this warning on their own.
    let warning = scan_bidi_controls("plain\u{200E}text\u{200F}here");
    assert!(!warning.any());
}

#[test]
fn resolve_paragraph_direction_detects_pure_latin_as_ltr() {
    assert_eq!(
        resolve_paragraph_direction("Hello world", TextDirection::Auto),
        ResolvedDirection::Ltr
    );
}

#[test]
fn resolve_paragraph_direction_detects_pure_hebrew_as_rtl() {
    assert_eq!(
        resolve_paragraph_direction("שלום עולם", TextDirection::Auto),
        ResolvedDirection::Rtl
    );
}

#[test]
fn resolve_paragraph_direction_defaults_all_neutral_text_to_ltr() {
    // UAX #9 P3: no strong character at all (digits/punctuation only) defaults to LTR.
    assert_eq!(
        resolve_paragraph_direction("123 456!", TextDirection::Auto),
        ResolvedDirection::Ltr
    );
}

#[test]
fn resolve_paragraph_direction_explicit_override_wins_over_script() {
    assert_eq!(
        resolve_paragraph_direction("Hello world", TextDirection::Rtl),
        ResolvedDirection::Rtl
    );
    assert_eq!(
        resolve_paragraph_direction("שלום עולם", TextDirection::Ltr),
        ResolvedDirection::Ltr
    );
}

#[test]
fn resolve_paragraph_direction_first_strong_character_decides_mixed_script_text() {
    assert_eq!(
        resolve_paragraph_direction("שלום Hello", TextDirection::Auto),
        ResolvedDirection::Rtl
    );
    assert_eq!(
        resolve_paragraph_direction("Hello שלום", TextDirection::Auto),
        ResolvedDirection::Ltr
    );
}

#[test]
fn resolve_text_align_start_end_flip_physical_edge_by_direction() {
    assert_eq!(
        resolve_text_align("Hello", TextDirection::Ltr, TextAlign::Start),
        TextAlign::Left
    );
    assert_eq!(
        resolve_text_align("Hello", TextDirection::Ltr, TextAlign::End),
        TextAlign::Right
    );
    assert_eq!(
        resolve_text_align("שלום", TextDirection::Rtl, TextAlign::Start),
        TextAlign::Right
    );
    assert_eq!(
        resolve_text_align("שלום", TextDirection::Rtl, TextAlign::End),
        TextAlign::Left
    );
}

#[test]
fn resolve_text_align_start_follows_auto_detected_direction_too() {
    assert_eq!(
        resolve_text_align("שלום עולם", TextDirection::Auto, TextAlign::Start),
        TextAlign::Right
    );
}

#[test]
fn resolve_text_align_passes_non_logical_variants_through_unchanged() {
    for align in [
        TextAlign::Auto,
        TextAlign::Left,
        TextAlign::Center,
        TextAlign::Right,
    ] {
        assert_eq!(
            resolve_text_align("Hello", TextDirection::Auto, align),
            align
        );
    }
}

#[test]
fn shape_cache_hits_on_a_repeated_identical_call() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let (hits0, misses0) = engine.shape_cache_stats();
    assert_eq!((hits0, misses0), (0, 0));

    engine.shape(
        "Boss fight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (10.0, 20.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let (hits1, misses1) = engine.shape_cache_stats();
    assert_eq!((hits1, misses1), (0, 1), "first call must be a miss");

    let shaped_again = engine.shape(
        "Boss fight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (10.0, 20.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let (hits2, misses2) = engine.shape_cache_stats();
    assert_eq!(
        (hits2, misses2),
        (1, 1),
        "identical repeated call must be a cache hit"
    );
    assert!(shaped_again.glyph_count() > 0);
}

#[test]
fn shape_cache_result_matches_a_fresh_uncached_shape() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    let first = engine.shape(
        "Victory screen",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        24.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let cached = engine.shape(
        "Victory screen",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        24.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    assert_eq!(first.width, cached.width);
    assert_eq!(first.height, cached.height);
    assert_eq!(first.glyph_count(), cached.glyph_count());
    for (a, b) in first.lines[0]
        .glyphs
        .iter()
        .zip(cached.lines[0].glyphs.iter())
    {
        assert_eq!(a.glyph_id, b.glyph_id);
        assert_eq!(a.x, b.x);
        assert_eq!(a.y, b.y);
    }
}

#[test]
fn shape_cache_misses_when_any_input_differs() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    engine.shape(
        "Boss fight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    // Different text.
    engine.shape(
        "Boss fight!",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    // Different origin.
    engine.shape(
        "Boss fight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (5.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    // Different font size.
    engine.shape(
        "Boss fight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        40.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    // Different weight_override (TEXT-01C) -- must be its own cache entry, since two different
    // wght values render visibly different glyphs on a variable family.
    engine.shape(
        "Boss fight",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        Some(600),
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let (hits, misses) = engine.shape_cache_stats();
    assert_eq!(hits, 0, "every call above has at least one differing input");
    assert_eq!(misses, 5);
}

#[test]
fn shape_cache_evicts_the_least_recently_used_entry_once_over_capacity() {
    let mut engine = TextLayoutEngine::new_from_locked_catalog();
    for i in 0..(SHAPE_CACHE_CAPACITY + 1) {
        engine.shape(
            &format!("caption {i}"),
            TextFontFamily::Lato,
            TextFontStyle::Regular,
            None,
            32.0,
            None,
            (0.0, 0.0),
            TextDirection::Auto,
            TextAlign::Auto,
        );
    }
    assert_eq!(engine.shape_cache.len(), SHAPE_CACHE_CAPACITY);

    // "caption 0" was the least-recently-used entry and should have been evicted -- shaping it
    // again must be a fresh miss, not a hit.
    let (_, misses_before) = engine.shape_cache_stats();
    engine.shape(
        "caption 0",
        TextFontFamily::Lato,
        TextFontStyle::Regular,
        None,
        32.0,
        None,
        (0.0, 0.0),
        TextDirection::Auto,
        TextAlign::Auto,
    );
    let (_, misses_after) = engine.shape_cache_stats();
    assert_eq!(misses_after, misses_before + 1);
}
