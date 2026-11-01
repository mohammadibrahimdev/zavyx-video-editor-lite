//! 20 original title templates built on top of `TitleEffect`.
//!
//! Each template is a preset configuration of `EffectInstance` parameters
//! (with the `zavyx.title` effect_id) plus a font, size, position, and color.
//! The user can apply a template to a clip and then customize.
//!
//! All templates are designed in the Zavyx violet palette (`#8B6CFF → #5B34E6`)
//! or complementary colors. No third-party assets are used.

use std::collections::BTreeMap;
use zavyx_ve_core::effect::{EffectInstance, ParamValue};

/// A title template — display name + the params to set on a `TitleEffect`.
pub struct TitleTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub params: BTreeMap<String, ParamValue>,
}

/// Returns all 20 built-in title templates.
pub fn all_templates() -> Vec<TitleTemplate> {
    use zavyx_ve_core::Rational;
    let _ = Rational::ZERO;
    vec![
        TitleTemplate {
            id: "title.center_white",
            name: "Center White Title",
            params: params(&[
                ("size", 102.0), ("x", 0.1), ("y", 0.4),
                ("color_r", 1.0), ("color_g", 1.0), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.lower_third_white",
            name: "Lower Third White",
            params: params(&[
                ("size", 58.0), ("x", 0.05), ("y", 0.75),
                ("color_r", 1.0), ("color_g", 1.0), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.violet_gradient",
            name: "Violet Gradient",
            params: params(&[
                ("size", 115.0), ("x", 0.15), ("y", 0.4),
                ("color_r", 0.55), ("color_g", 0.42), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.bold_intro",
            name: "Bold Intro",
            params: params(&[
                ("size", 154.0), ("x", 0.1), ("y", 0.35),
                ("color_r", 1.0), ("color_g", 1.0), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.subtitle_small",
            name: "Subtitle Small",
            params: params(&[
                ("size", 38.0), ("x", 0.1), ("y", 0.85),
                ("color_r", 0.85), ("color_g", 0.85), ("color_b", 0.9),
            ]),
        },
        TitleTemplate {
            id: "title.credits_roll",
            name: "Credits Roll",
            params: params(&[
                ("size", 51.0), ("x", 0.3), ("y", 0.5),
                ("color_r", 1.0), ("color_g", 1.0), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.zavyx_brand",
            name: "Zavyx Brand",
            params: params(&[
                ("size", 128.0), ("x", 0.05), ("y", 0.05),
                ("color_r", 0.55), ("color_g", 0.29), ("color_b", 0.9),
            ]),
        },
        TitleTemplate {
            id: "title.cinematic_wide",
            name: "Cinematic Wide",
            params: params(&[
                ("size", 77.0), ("x", 0.05), ("y", 0.45),
                ("color_r", 0.95), ("color_g", 0.92), ("color_b", 0.98),
            ]),
        },
        TitleTemplate {
            id: "title.neon_glow",
            name: "Neon Glow",
            params: params(&[
                ("size", 102.0), ("x", 0.2), ("y", 0.4),
                ("color_r", 0.55), ("color_g", 0.42), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.tiktok_caption",
            name: "TikTok Caption",
            params: params(&[
                ("size", 64.0), ("x", 0.1), ("y", 0.7),
                ("color_r", 1.0), ("color_g", 1.0), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.instagram_story",
            name: "Instagram Story",
            params: params(&[
                ("size", 90.0), ("x", 0.15), ("y", 0.45),
                ("color_r", 1.0), ("color_g", 1.0), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.youtube_intro",
            name: "YouTube Intro",
            params: params(&[
                ("size", 115.0), ("x", 0.1), ("y", 0.1),
                ("color_r", 1.0), ("color_g", 1.0), ("color_b", 1.0),
            ]),
        },
        TitleTemplate {
            id: "title.end_screen",
            name: "End Screen",
            params: params(&[
                ("size", 77.0), ("x", 0.2), ("y", 0.45),
                ("color_r", 0.85), ("color_g", 0.85), ("color_b", 0.9),
            ]),
        },
        TitleTemplate {
            id: "title.callout_arrow",
            name: "Callout (with arrow)",
            params: params(&[
                ("size", 45.0), ("x", 0.6), ("y", 0.3),
                ("color_r", 1.0), ("color_g", 0.85), ("color_b", 0.0),
            ]),
        },
        TitleTemplate {
            id: "title.warning_red",
            name: "Warning Red",
            params: params(&[
                ("size", 90.0), ("x", 0.1), ("y", 0.4),
                ("color_r", 0.82), ("color_g", 0.24), ("color_b", 0.24),
            ]),
        },
        TitleTemplate {
            id: "title.success_green",
            name: "Success Green",
            params: params(&[
                ("size", 90.0), ("x", 0.1), ("y", 0.4),
                ("color_r", 0.13), ("color_g", 0.63), ("color_b", 0.35),
            ]),
        },
        TitleTemplate {
            id: "title.info_blue",
            name: "Info Blue",
            params: params(&[
                ("size", 90.0), ("x", 0.1), ("y", 0.4,
                ),  // Note: closing paren misplaced intentionally? No — fix below.
                ("color_r", 0.25), ("color_g", 0.55), ("color_b", 0.95),
            ]),
        },
        TitleTemplate {
            id: "title.tagline_italic",
            name: "Tagline Italic",
            params: params(&[
                ("size", 58.0), ("x", 0.2), ("y", 0.55,
                ),
                ("color_r", 0.9), ("color_g", 0.88), ("color_b", 0.95,
                ),
            ]),
        },
        TitleTemplate {
            id: "title.quote_block",
            name: "Quote Block",
            params: params(&[
                ("size", 51.0), ("x", 0.15), ("y", 0.4,
                ),
                ("color_r", 0.95), ("color_g", 0.92), ("color_b", 0.85,
                ),
            ]),
        },
        TitleTemplate {
            id: "title.vintage_film",
            name: "Vintage Film",
            params: params(&[
                ("size", 70.0), ("x", 0.1), ("y", 0.1,
                ),
                ("color_r", 0.92), ("color_g", 0.78), ("color_b", 0.55,
                ),
            ]),
        },
    ]
}

fn params(values: &[(&str, f64)]) -> BTreeMap<String, ParamValue> {
    values.iter().map(|(k, v)| (k.to_string(), ParamValue::Const(*v))).collect()
}

/// Apply a template's parameters to an `EffectInstance` (modifies in-place).
pub fn apply_template(instance: &mut EffectInstance, template_id: &str) -> bool {
    let templates = all_templates();
    let template = templates.iter().find(|t| t.id == template_id);
    if let Some(t) = template {
        instance.params = t.params.clone();
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_20_templates_present() {
        let t = all_templates();
        assert_eq!(t.len(), 20, "expected 20 templates, got {}", t.len());
    }

    #[test]
    fn template_ids_are_unique() {
        let t = all_templates();
        let ids: Vec<&str> = t.iter().map(|x| x.id).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "template ids should be unique");
    }

    #[test]
    fn apply_template_overrides_params() {
        let mut instance = EffectInstance::new("zavyx.title");
        instance.set_const("size", 19.0);  // initial small
        let applied = apply_template(&mut instance, "title.center_white");
        assert!(applied);
        let size = instance.params.get("size").and_then(|v| match v {
            ParamValue::Const(x) => Some(*x),
            _ => None,
        }).unwrap_or(0.0);
        assert_eq!(size, 102.0);
    }

    #[test]
    fn apply_template_unknown_returns_false() {
        let mut instance = EffectInstance::new("zavyx.title");
        let applied = apply_template(&mut instance, "title.does_not_exist");
        assert!(!applied);
    }

    #[test]
    fn templates_use_zavyx_palette_for_some() {
        // At least one template should use the Zavyx violet color.
        let t = all_templates();
        let any_violet = t.iter().any(|tmpl| {
            tmpl.params.get("color_r").and_then(|v| match v {
                ParamValue::Const(x) => Some(*x),
                _ => None,
            }).unwrap_or(0.0) > 0.5
            && tmpl.params.get("color_b").and_then(|v| match v {
                ParamValue::Const(x) => Some(*x),
                _ => None,
            }).unwrap_or(0.0) > 0.8
        });
        assert!(any_violet, "at least one template should use the Zavyx violet palette");
    }
}
