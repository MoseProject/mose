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

use super::*;
use crate::keyframe::evaluate_keyframes;
use crate::timeline::{ShapeKind, TextFontFamily, TextFontStyle};

fn text_element(id: &str, text: TextBinding, color: ColorBinding) -> TemplateTextElement {
    TemplateTextElement {
        id: id.to_string(),
        text,
        color_rgba: color,
        font_family: TextFontFamily::Lato,
        font_style: TextFontStyle::Regular,
        font_size: 32.0,
        pos_x: 0.1,
        pos_y: 0.8,
        timing: TemplateTiming::default(),
    }
}

fn shape_element(id: &str, color: ColorBinding) -> TemplateShapeElement {
    TemplateShapeElement {
        id: id.to_string(),
        shape_kind: ShapeKind::rectangle(),
        color_rgba: color,
        center_x: 0.5,
        center_y: 0.5,
        width: 0.3,
        height: 0.1,
        rotation_deg: 0.0,
        stroke_thickness_px: 0.0,
    }
}

fn minimal_template() -> GraphicTemplate {
    GraphicTemplate {
        schema_version: TEMPLATE_SCHEMA_VERSION,
        name: "Scoreboard".to_string(),
        canvas_width: 1920,
        canvas_height: 1080,
        safe_area_margin: 0.0,
        parameters: vec![],
        elements: vec![],
    }
}

#[test]
fn a_minimal_template_with_no_elements_is_valid() {
    let template = minimal_template();
    assert!(template.validate().is_ok());
}

#[test]
fn rejects_an_unsupported_schema_version() {
    let mut template = minimal_template();
    template.schema_version = 99;
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::UnsupportedSchemaVersion { found: 99 })
    );
}

#[test]
fn rejects_a_zero_canvas_dimension() {
    let mut template = minimal_template();
    template.canvas_width = 0;
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::InvalidCanvasSize {
            width: 0,
            height: 1080
        })
    );
}

#[test]
fn rejects_duplicate_parameter_ids() {
    let mut template = minimal_template();
    template.parameters = vec![
        TemplateParameter {
            id: "name".to_string(),
            label: "Name".to_string(),
            kind: TemplateParameterKind::Text,
        },
        TemplateParameter {
            id: "name".to_string(),
            label: "Name again".to_string(),
            kind: TemplateParameterKind::Text,
        },
    ];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::DuplicateParameterId(
            "name".to_string()
        ))
    );
}

#[test]
fn rejects_duplicate_element_ids_across_text_and_shape() {
    let mut template = minimal_template();
    template.elements = vec![
        TemplateElement::Text(text_element(
            "e1",
            TextBinding::Fixed("Hi".to_string()),
            ColorBinding::Fixed([255, 255, 255, 255]),
        )),
        TemplateElement::Shape(shape_element("e1", ColorBinding::Fixed([0, 0, 0, 255]))),
    ];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::DuplicateElementId(
            "e1".to_string()
        ))
    );
}

#[test]
fn rejects_a_text_binding_referencing_an_undeclared_parameter() {
    let mut template = minimal_template();
    template.elements = vec![TemplateElement::Text(text_element(
        "e1",
        TextBinding::Parameter("missing".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    ))];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::UnknownParameterReference {
            element_id: "e1".to_string(),
            parameter_id: "missing".to_string(),
        })
    );
}

#[test]
fn rejects_a_text_binding_referencing_a_color_typed_parameter() {
    let mut template = minimal_template();
    template.parameters = vec![TemplateParameter {
        id: "team_color".to_string(),
        label: "Team color".to_string(),
        kind: TemplateParameterKind::Color,
    }];
    template.elements = vec![TemplateElement::Text(text_element(
        "e1",
        TextBinding::Parameter("team_color".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    ))];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::ParameterKindMismatch {
            element_id: "e1".to_string(),
            parameter_id: "team_color".to_string(),
            expected: TemplateParameterKind::Text,
        })
    );
}

#[test]
fn accepts_a_correctly_typed_parameter_reference() {
    let mut template = minimal_template();
    template.parameters = vec![
        TemplateParameter {
            id: "player_name".to_string(),
            label: "Player name".to_string(),
            kind: TemplateParameterKind::Text,
        },
        TemplateParameter {
            id: "team_color".to_string(),
            label: "Team color".to_string(),
            kind: TemplateParameterKind::Color,
        },
    ];
    template.elements = vec![TemplateElement::Text(text_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
        ColorBinding::Parameter("team_color".to_string()),
    ))];
    assert!(template.validate().is_ok());
}

#[test]
fn rejects_a_non_positive_font_size() {
    let mut template = minimal_template();
    let mut t = text_element(
        "e1",
        TextBinding::Fixed("Hi".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    );
    t.font_size = 0.0;
    template.elements = vec![TemplateElement::Text(t)];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::NonPositiveFontSize {
            element_id: "e1".to_string(),
            value: 0.0,
        })
    );
}

#[test]
fn rejects_a_non_positive_shape_extent() {
    let mut template = minimal_template();
    let mut s = shape_element("e1", ColorBinding::Fixed([0, 0, 0, 255]));
    s.width = 0.0;
    template.elements = vec![TemplateElement::Shape(s)];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::NonPositiveShapeExtent {
            element_id: "e1".to_string(),
            width: 0.0,
            height: 0.1,
        })
    );
}

#[test]
fn rejects_a_negative_fade_in() {
    let mut template = minimal_template();
    let mut t = text_element(
        "e1",
        TextBinding::Fixed("Hi".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    );
    t.timing.fade_in_secs = -1.0;
    template.elements = vec![TemplateElement::Text(t)];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::NegativeTiming {
            element_id: "e1".to_string(),
            fade_in_secs: -1.0,
            fade_out_secs: 0.0,
        })
    );
}

#[test]
fn rejects_a_negative_fade_out() {
    let mut template = minimal_template();
    let mut t = text_element(
        "e1",
        TextBinding::Fixed("Hi".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    );
    t.timing.fade_out_secs = -0.5;
    template.elements = vec![TemplateElement::Text(t)];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::NegativeTiming {
            element_id: "e1".to_string(),
            fade_in_secs: 0.0,
            fade_out_secs: -0.5,
        })
    );
}

#[test]
fn zero_timing_produces_no_opacity_keyframes() {
    let keyframes = timing_opacity_keyframes(TemplateTiming::default(), 3.0);
    assert!(keyframes.is_empty());
}

#[test]
fn zero_duration_produces_no_opacity_keyframes_even_with_timing_set() {
    let timing = TemplateTiming {
        fade_in_secs: 0.5,
        fade_out_secs: 0.5,
    };
    assert!(timing_opacity_keyframes(timing, 0.0).is_empty());
}

#[test]
fn fade_in_only_holds_full_opacity_after_the_fade() {
    let timing = TemplateTiming {
        fade_in_secs: 1.0,
        fade_out_secs: 0.0,
    };
    let keyframes = timing_opacity_keyframes(timing, 4.0);
    assert_eq!(keyframes.len(), 2);
    assert_eq!(
        keyframes[0],
        Keyframe {
            time_fraction: 0.0,
            value: 0.0
        }
    );
    assert_eq!(
        keyframes[1],
        Keyframe {
            time_fraction: 0.25,
            value: 1.0
        }
    );
    // Fully opaque well past the fade -- evaluate_keyframes holds the last keyframe's value.
    assert_eq!(evaluate_keyframes(&keyframes, 0.9, 0.0), 1.0);
}

#[test]
fn fade_out_only_holds_full_opacity_before_the_fade() {
    let timing = TemplateTiming {
        fade_in_secs: 0.0,
        fade_out_secs: 1.0,
    };
    let keyframes = timing_opacity_keyframes(timing, 4.0);
    assert_eq!(keyframes.len(), 2);
    assert_eq!(
        keyframes[0],
        Keyframe {
            time_fraction: 0.75,
            value: 1.0
        }
    );
    assert_eq!(
        keyframes[1],
        Keyframe {
            time_fraction: 1.0,
            value: 0.0
        }
    );
    assert_eq!(evaluate_keyframes(&keyframes, 0.1, 0.0), 1.0);
}

#[test]
fn fade_in_and_out_together_produce_four_ascending_keyframes() {
    let timing = TemplateTiming {
        fade_in_secs: 1.0,
        fade_out_secs: 1.0,
    };
    let keyframes = timing_opacity_keyframes(timing, 4.0);
    assert_eq!(keyframes.len(), 4);
    let fractions: Vec<f32> = keyframes.iter().map(|k| k.time_fraction).collect();
    let mut sorted = fractions.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(fractions, sorted, "keyframes must be ascending");
    assert_eq!(keyframes[0].value, 0.0);
    assert_eq!(keyframes[1].value, 1.0);
    assert_eq!(keyframes[2].value, 1.0);
    assert_eq!(keyframes[3].value, 0.0);
}

#[test]
fn an_overlapping_fade_in_and_out_is_scaled_down_to_fit_the_duration() {
    // fade_in + fade_out (6.0) exceeds duration_secs (4.0) -- both must shrink proportionally
    // rather than producing out-of-order time_fraction keyframes.
    let timing = TemplateTiming {
        fade_in_secs: 3.0,
        fade_out_secs: 3.0,
    };
    let keyframes = timing_opacity_keyframes(timing, 4.0);
    assert_eq!(keyframes.len(), 4);
    let fractions: Vec<f32> = keyframes.iter().map(|k| k.time_fraction).collect();
    let mut sorted = fractions.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(
        fractions, sorted,
        "keyframes must stay ascending after scaling"
    );
    // Equal fades scaled equally must meet exactly in the middle.
    assert!((keyframes[1].time_fraction - 0.5).abs() < 1e-6);
    assert!((keyframes[2].time_fraction - 0.5).abs() < 1e-6);
}

#[test]
fn negative_timing_inputs_are_clamped_rather_than_producing_backwards_keyframes() {
    // validate() rejects this at the GraphicTemplate level -- this only proves the pure
    // conversion function itself never panics or misbehaves if ever called with unvalidated
    // input directly.
    let timing = TemplateTiming {
        fade_in_secs: -1.0,
        fade_out_secs: 1.0,
    };
    let keyframes = timing_opacity_keyframes(timing, 4.0);
    assert_eq!(
        keyframes.len(),
        2,
        "a clamped-negative fade_in must not contribute keyframes"
    );
}

#[test]
fn parse_and_validate_rejects_an_unrecognized_primitive_kind() {
    let json = r#"{
        "schema_version": 1,
        "name": "Bad",
        "canvas_width": 100,
        "canvas_height": 100,
        "parameters": [],
        "elements": [
            { "Video": { "id": "e1" } }
        ]
    }"#;
    let result = GraphicTemplate::parse_and_validate(json);
    assert!(matches!(
        result,
        Err(TemplateParseOrValidationError::Parse(_))
    ));
}

#[test]
fn parse_and_validate_round_trips_a_real_template_through_json() {
    let mut template = minimal_template();
    template.parameters = vec![TemplateParameter {
        id: "name".to_string(),
        label: "Name".to_string(),
        kind: TemplateParameterKind::Text,
    }];
    template.elements = vec![TemplateElement::Text(text_element(
        "e1",
        TextBinding::Parameter("name".to_string()),
        ColorBinding::Fixed([255, 0, 0, 255]),
    ))];

    let json = serde_json::to_string(&template).unwrap();
    let parsed = GraphicTemplate::parse_and_validate(&json).unwrap();
    assert_eq!(parsed, template);
}

#[test]
fn instantiate_resolves_fixed_and_parameter_bound_values() {
    let mut template = minimal_template();
    template.parameters = vec![
        TemplateParameter {
            id: "player_name".to_string(),
            label: "Player name".to_string(),
            kind: TemplateParameterKind::Text,
        },
        TemplateParameter {
            id: "team_color".to_string(),
            label: "Team color".to_string(),
            kind: TemplateParameterKind::Color,
        },
    ];
    template.elements = vec![TemplateElement::Text(text_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
        ColorBinding::Parameter("team_color".to_string()),
    ))];

    let mut values = HashMap::new();
    values.insert(
        "player_name".to_string(),
        ParameterValue::Text("PacoPaçoca".to_string()),
    );
    values.insert(
        "team_color".to_string(),
        ParameterValue::Color([10, 20, 30, 255]),
    );

    let result = instantiate(&template, &values).unwrap();
    assert_eq!(result.len(), 1);
    match &result[0] {
        InstantiatedElement::Text {
            text, color_rgba, ..
        } => {
            assert_eq!(text, "PacoPaçoca");
            assert_eq!(*color_rgba, [10, 20, 30, 255]);
        }
        InstantiatedElement::Shape { .. } => panic!("expected a text element"),
    }
}

#[test]
fn instantiate_fails_with_a_missing_parameter_value() {
    let mut template = minimal_template();
    template.parameters = vec![TemplateParameter {
        id: "player_name".to_string(),
        label: "Player name".to_string(),
        kind: TemplateParameterKind::Text,
    }];
    template.elements = vec![TemplateElement::Text(text_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    ))];

    let values = HashMap::new();
    assert_eq!(
        instantiate(&template, &values),
        Err(TemplateInstantiateError::MissingParameterValue(
            "player_name".to_string()
        ))
    );
}

#[test]
fn instantiate_fails_when_a_supplied_value_has_the_wrong_kind() {
    let mut template = minimal_template();
    template.parameters = vec![TemplateParameter {
        id: "player_name".to_string(),
        label: "Player name".to_string(),
        kind: TemplateParameterKind::Text,
    }];
    template.elements = vec![TemplateElement::Text(text_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    ))];

    let mut values = HashMap::new();
    values.insert(
        "player_name".to_string(),
        ParameterValue::Color([1, 2, 3, 255]),
    );
    assert_eq!(
        instantiate(&template, &values),
        Err(TemplateInstantiateError::WrongParameterValueKind {
            parameter_id: "player_name".to_string(),
            expected: TemplateParameterKind::Text,
        })
    );
}

#[test]
fn instantiate_rejects_an_invalid_template_before_touching_values() {
    let mut template = minimal_template();
    template.schema_version = 99;
    let values = HashMap::new();
    assert_eq!(
        instantiate(&template, &values),
        Err(TemplateInstantiateError::Validation(
            TemplateValidationError::UnsupportedSchemaVersion { found: 99 }
        ))
    );
}

#[test]
fn instantiate_resolves_a_shape_element_with_a_fixed_color() {
    let mut template = minimal_template();
    template.elements = vec![TemplateElement::Shape(shape_element(
        "s1",
        ColorBinding::Fixed([9, 9, 9, 255]),
    ))];
    let values = HashMap::new();
    let result = instantiate(&template, &values).unwrap();
    match &result[0] {
        InstantiatedElement::Shape { color_rgba, .. } => {
            assert_eq!(*color_rgba, [9, 9, 9, 255]);
        }
        InstantiatedElement::Text { .. } => panic!("expected a shape element"),
    }
}

#[test]
fn safe_area_violations_is_a_no_op_when_margin_is_zero() {
    let mut template = minimal_template();
    let mut t = text_element(
        "e1",
        TextBinding::Fixed("Hi".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    );
    t.pos_x = 0.0;
    t.pos_y = 0.0;
    template.elements = vec![TemplateElement::Text(t)];
    assert!(safe_area_violations(&template).is_empty());
}

#[test]
fn safe_area_violations_flags_a_text_anchor_inside_the_margin() {
    let mut template = minimal_template();
    template.safe_area_margin = 0.1;
    let mut t = text_element(
        "e1",
        TextBinding::Fixed("Hi".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    );
    t.pos_x = 0.02;
    t.pos_y = 0.5;
    template.elements = vec![TemplateElement::Text(t)];
    let violations = safe_area_violations(&template);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].element_id, "e1");
}

#[test]
fn safe_area_violations_accepts_a_text_anchor_clear_of_the_margin() {
    let mut template = minimal_template();
    template.safe_area_margin = 0.1;
    let mut t = text_element(
        "e1",
        TextBinding::Fixed("Hi".to_string()),
        ColorBinding::Fixed([255, 255, 255, 255]),
    );
    t.pos_x = 0.5;
    t.pos_y = 0.5;
    template.elements = vec![TemplateElement::Text(t)];
    assert!(safe_area_violations(&template).is_empty());
}

#[test]
fn safe_area_violations_flags_a_shape_bounding_box_crossing_the_margin() {
    let mut template = minimal_template();
    template.safe_area_margin = 0.1;
    let mut s = shape_element("s1", ColorBinding::Fixed([0, 0, 0, 255]));
    s.center_x = 0.05;
    s.center_y = 0.5;
    s.width = 0.05;
    s.height = 0.05;
    template.elements = vec![TemplateElement::Shape(s)];
    let violations = safe_area_violations(&template);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].element_id, "s1");
}

#[test]
fn safe_area_violations_accepts_a_shape_fully_clear_of_the_margin() {
    let mut template = minimal_template();
    template.safe_area_margin = 0.1;
    let s = shape_element("s1", ColorBinding::Fixed([0, 0, 0, 255]));
    template.elements = vec![TemplateElement::Shape(s)];
    assert!(safe_area_violations(&template).is_empty());
}

fn family_with(variants: Vec<TemplateVariant>) -> TemplateFamily {
    TemplateFamily {
        name: "Scoreboard".to_string(),
        variants,
    }
}

#[test]
fn template_family_rejects_an_empty_variant_list() {
    let family = family_with(vec![]);
    assert_eq!(family.validate(), Err(TemplateFamilyValidationError::Empty));
}

#[test]
fn template_family_rejects_duplicate_aspect_ratios() {
    let family = family_with(vec![
        TemplateVariant {
            aspect_ratio: ExportAspectRatio::Landscape,
            template: minimal_template(),
        },
        TemplateVariant {
            aspect_ratio: ExportAspectRatio::Landscape,
            template: minimal_template(),
        },
    ]);
    assert_eq!(
        family.validate(),
        Err(TemplateFamilyValidationError::DuplicateAspectRatio(
            ExportAspectRatio::Landscape
        ))
    );
}

#[test]
fn template_family_propagates_a_variants_own_validation_error() {
    let mut invalid = minimal_template();
    invalid.schema_version = 99;
    let family = family_with(vec![TemplateVariant {
        aspect_ratio: ExportAspectRatio::Portrait,
        template: invalid,
    }]);
    assert_eq!(
        family.validate(),
        Err(TemplateFamilyValidationError::Variant {
            aspect_ratio: ExportAspectRatio::Portrait,
            error: TemplateValidationError::UnsupportedSchemaVersion { found: 99 },
        })
    );
}

#[test]
fn template_family_accepts_distinct_aspect_ratios_and_valid_variants() {
    let family = family_with(vec![
        TemplateVariant {
            aspect_ratio: ExportAspectRatio::Landscape,
            template: minimal_template(),
        },
        TemplateVariant {
            aspect_ratio: ExportAspectRatio::Portrait,
            template: minimal_template(),
        },
    ]);
    assert!(family.validate().is_ok());
}

#[test]
fn template_family_variant_for_looks_up_by_aspect_ratio() {
    let mut portrait = minimal_template();
    portrait.name = "Portrait variant".to_string();
    let family = family_with(vec![
        TemplateVariant {
            aspect_ratio: ExportAspectRatio::Landscape,
            template: minimal_template(),
        },
        TemplateVariant {
            aspect_ratio: ExportAspectRatio::Portrait,
            template: portrait,
        },
    ]);
    let found = family.variant_for(ExportAspectRatio::Portrait).unwrap();
    assert_eq!(found.name, "Portrait variant");
    assert!(family.variant_for(ExportAspectRatio::Square).is_none());
}

#[test]
fn every_validation_error_names_its_offender() {
    let errors = vec![
        TemplateValidationError::UnsupportedSchemaVersion { found: 9 },
        TemplateValidationError::InvalidCanvasSize {
            width: 0,
            height: 7,
        },
        TemplateValidationError::DuplicateParameterId("dup_param".to_string()),
        TemplateValidationError::DuplicateElementId("dup_element".to_string()),
        TemplateValidationError::UnknownParameterReference {
            element_id: "e_unknown".to_string(),
            parameter_id: "p_unknown".to_string(),
        },
        TemplateValidationError::ParameterKindMismatch {
            element_id: "e_kind".to_string(),
            parameter_id: "p_kind".to_string(),
            expected: TemplateParameterKind::Color,
        },
        TemplateValidationError::NonPositiveFontSize {
            element_id: "e_font".to_string(),
            value: -3.0,
        },
        TemplateValidationError::NonPositiveShapeExtent {
            element_id: "e_shape".to_string(),
            width: 0.0,
            height: 5.0,
        },
        TemplateValidationError::NegativeTiming {
            element_id: "e_timing".to_string(),
            fade_in_secs: -1.0,
            fade_out_secs: 0.5,
        },
    ];
    let messages: Vec<String> = errors.iter().map(|error| error.to_string()).collect();
    for (message, needle) in messages.iter().zip([
        "9",
        "0",
        "dup_param",
        "dup_element",
        "p_unknown",
        "p_kind",
        "e_font",
        "e_shape",
        "e_timing",
    ]) {
        assert!(
            message.contains(needle),
            "{message} should mention {needle}"
        );
    }
    let unique: std::collections::HashSet<_> = messages.iter().collect();
    assert_eq!(unique.len(), messages.len());
}

#[test]
fn parse_instantiate_and_family_errors_are_readable() {
    let parse = GraphicTemplate::parse_and_validate("{").unwrap_err();
    assert!(matches!(parse, TemplateParseOrValidationError::Parse(_)));
    assert!(parse.to_string().contains("could not parse"));
    let validation =
        TemplateParseOrValidationError::from(TemplateValidationError::UnsupportedSchemaVersion {
            found: 4,
        });
    assert!(validation.to_string().contains('4'));

    let missing = TemplateInstantiateError::MissingParameterValue("p1".to_string());
    assert!(missing.to_string().contains("p1"));
    let wrong = TemplateInstantiateError::WrongParameterValueKind {
        parameter_id: "p2".to_string(),
        expected: TemplateParameterKind::Color,
    };
    assert!(wrong.to_string().contains("p2"));
    let wrapped = TemplateInstantiateError::Validation(
        TemplateValidationError::DuplicateElementId("e9".to_string()),
    );
    assert!(wrapped.to_string().contains("e9"));

    assert!(TemplateFamilyValidationError::Empty
        .to_string()
        .contains("no variants"));
    let duplicate =
        TemplateFamilyValidationError::DuplicateAspectRatio(ExportAspectRatio::Portrait);
    assert!(duplicate.to_string().contains("Portrait"));
    let variant = TemplateFamilyValidationError::Variant {
        aspect_ratio: ExportAspectRatio::Landscape,
        error: TemplateValidationError::DuplicateParameterId("dp".to_string()),
    };
    let message = variant.to_string();
    assert!(message.contains("Landscape") && message.contains("dp"));
}

#[test]
fn a_shape_color_bound_to_a_text_parameter_is_rejected_and_wrong_values_fail_to_instantiate() {
    let mut template = minimal_template();
    template.parameters = vec![TemplateParameter {
        id: "label".to_string(),
        label: "Label".to_string(),
        kind: TemplateParameterKind::Text,
    }];
    template.elements = vec![TemplateElement::Shape(shape_element(
        "s1",
        ColorBinding::Parameter("label".to_string()),
    ))];
    assert_eq!(
        template.validate(),
        Err(TemplateValidationError::ParameterKindMismatch {
            element_id: "s1".to_string(),
            parameter_id: "label".to_string(),
            expected: TemplateParameterKind::Color,
        })
    );

    // At instantiate time a text slot given a colour, and a colour slot given text, both fail.
    let mut text_template = minimal_template();
    text_template.parameters = vec![TemplateParameter {
        id: "name".to_string(),
        label: "Name".to_string(),
        kind: TemplateParameterKind::Text,
    }];
    text_template.elements = vec![TemplateElement::Text(text_element(
        "t1",
        TextBinding::Parameter("name".to_string()),
        ColorBinding::Fixed([0, 0, 0, 255]),
    ))];
    let mut values = HashMap::new();
    values.insert("name".to_string(), ParameterValue::Color([1, 2, 3, 4]));
    assert!(matches!(
        instantiate(&text_template, &values),
        Err(TemplateInstantiateError::WrongParameterValueKind {
            expected: TemplateParameterKind::Text,
            ..
        })
    ));

    let mut color_template = minimal_template();
    color_template.parameters = vec![TemplateParameter {
        id: "tint".to_string(),
        label: "Tint".to_string(),
        kind: TemplateParameterKind::Color,
    }];
    color_template.elements = vec![TemplateElement::Shape(shape_element(
        "s2",
        ColorBinding::Parameter("tint".to_string()),
    ))];
    let mut values = HashMap::new();
    values.insert("tint".to_string(), ParameterValue::Text("red".to_string()));
    assert!(matches!(
        instantiate(&color_template, &values),
        Err(TemplateInstantiateError::WrongParameterValueKind {
            expected: TemplateParameterKind::Color,
            ..
        })
    ));
    assert!(matches!(
        instantiate(&color_template, &HashMap::new()),
        Err(TemplateInstantiateError::MissingParameterValue(_))
    ));
}
