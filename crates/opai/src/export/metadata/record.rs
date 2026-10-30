//! What an export records of what was rendered: the enhancements and their settings, a simulated aperture and focus,
//! a crop's effect on the field of view, and what kind of image the result is.
//!
//! **Capture fields describe the capture.** A lens blur leaves FNumber as the lens was set, a crop leaves
//! FocalLengthIn35mmFilm as the lens is; what was rendered goes beside them, where no reader takes it for what the
//! camera did — the practice of every application that renders a synthetic aperture (Lightroom's `crs:LensBlur`,
//! Google's `GFocus:`, Apple's adjustment data).
//!
//! | Where | What |
//! |---|---|
//! | `opai:Enhancements` | an ordered list, one entry per enhancement: its name and settings, `Upscale Kyoto 4x (FP16): scale=4.0` |
//! | `opai:Masked` | `True` where a mask limited the enhancements to part of the photograph |
//! | `opai:LensBlurFNumber` | the 35 mm-equivalent f-number a lens blur simulated |
//! | `opai:FocusPointX`, `opai:FocusPointY` | where it focused, from `0` to `1` across and down the exported frame |
//! | `opai:CropFactor` | how many times the source's diagonal the crop's is shorter: what a crop multiplies the 35 mm-equivalent focal length by |
//! | `opai:EffectiveFocalLengthIn35mmFormat` | the source's FocalLengthIn35mmFilm times the crop factor: the field of view the export shows |
//! | `stEvt:parameters` of the export's `xmpMM:History` event | the enhancements, by name, after the conversion |
//! | `Iptc4xmpExt:DigitalSourceType` | the IPTC term for the strongest enhancement of the chain ([`digital_source_type`]) |
//! | EXIF ImageEditingSoftware `0xA43B` | Open Photo AI and its version: set where the EXIF is 3.0 or later, replaced where present ([`super::exif`]) |
//!
//! The `opai:` namespace is [`NAMESPACE`], under the project's own published site so that it cannot collide and
//! stays the project's. Nothing is recorded for an export of the source as it was.

use super::xmp::edit::{Array, Xmp};
use crate::image::Edits;
use crate::models::Operation;

/// The namespace URI of Open Photo AI's own XMP properties, under the prefix `opai`.
pub(crate) const NAMESPACE: &str = "https://vegidio.github.io/open-photo-ai/ns/1.0/";

/// The IPTC Extension namespace, which `DigitalSourceType` belongs to.
pub(crate) const IPTC_EXT: &str = "http://iptc.org/std/Iptc4xmpExt/2008-02-29/";

/// Where the IPTC digital source type vocabulary's terms live; a value is this followed by the term.
pub(crate) const DIGITAL_SOURCE_TYPE: &str = "http://cv.iptc.org/newscodes/digitalsourcetype/";

/// The IPTC digital source types an edit can make a photograph, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SourceType {
    /// `humanEdits`: framed with non-generative tools — a crop, a turn, a flip.
    HumanEdits,
    /// `algorithmicallyEnhanced`: "modification or correction by algorithm without changing the main content of the
    /// media", the vocabulary's examples being sharpening and noise reduction.
    AlgorithmicallyEnhanced,
    /// `compositeWithTrainedAlgorithmicMedia`: "augmentation using a generative AI model", inpainting named.
    CompositeWithTrainedAlgorithmicMedia,
}

impl SourceType {
    /// The term's URI, as the property's value.
    pub(crate) fn uri(self) -> String {
        let term = match self {
            Self::HumanEdits => "humanEdits",
            Self::AlgorithmicallyEnhanced => "algorithmicallyEnhanced",
            Self::CompositeWithTrainedAlgorithmicMedia => "compositeWithTrainedAlgorithmicMedia",
        };
        format!("{DIGITAL_SOURCE_TYPE}{term}")
    }
}

/// What one enhancement makes of a photograph, in the IPTC vocabulary.
///
/// What draws content no camera recorded is generative: a removal fills its region by inpainting (LaMa), a restoration
/// redraws lost detail with a generative model, a face recovery redraws faces from a learned prior, and a colouriser
/// invents the colours (the vocabulary names neither of the last two; judged so here, as the closest term). The rest
/// estimate rather than draw: denoise, sharpen, light, colour balance, a convolutional super-resolution upscale and a
/// lens blur computed from depth.
pub(crate) fn source_type_of(operation: &Operation) -> SourceType {
    match operation {
        Operation::Remove(_) | Operation::Restore(_) | Operation::FaceRecovery(_) | Operation::Colorization(_) => {
            SourceType::CompositeWithTrainedAlgorithmicMedia
        }
        Operation::Denoise(_)
        | Operation::Sharpen(_)
        | Operation::LightAdjustment(_)
        | Operation::ColorBalance(_)
        | Operation::Upscale(_)
        | Operation::LensBlur(_) => SourceType::AlgorithmicallyEnhanced,
    }
}

/// The strongest source type of `edits`: of its operations, or `humanEdits` for a framing alone; `None` where nothing
/// was done, and the source's own value stays true.
pub(crate) fn digital_source_type(edits: &Edits) -> Option<SourceType> {
    let framed = edits.frame().is_some().then_some(SourceType::HumanEdits);
    edits.operations().iter().map(source_type_of).chain(framed).max()
}

/// One enhancement as `opai:Enhancements` names it: its family and model, then each setting its serialized form carries
/// as a plain value, `key=value`. What only locates content — a removal's strokes, a face recovery's faces — is left
/// out: the list says what was done, not where in the photograph something was taken out.
pub(crate) fn describe(operation: &Operation) -> String {
    let mut settings = Vec::new();
    if let Ok(serde_json::Value::Object(fields)) = serde_json::to_value(operation) {
        for (key, value) in fields {
            if matches!(key.as_str(), "family" | "variant" | "mask" | "faces") {
                continue;
            }
            match value {
                serde_json::Value::Number(number) => settings.push(format!("{key}={number}")),
                serde_json::Value::String(text) => settings.push(format!("{key}={text}")),
                serde_json::Value::Bool(flag) => settings.push(format!("{key}={flag}")),
                _ => {}
            }
        }
    }
    let name = name(operation);
    if settings.is_empty() { name } else { format!("{name}: {}", settings.join(", ")) }
}

/// An enhancement's family and model: `Upscale Kyoto 4x (FP16)`, `Lens Blur Lisbon (FP16)`.
fn name(operation: &Operation) -> String {
    format!("{} {}", operation.family().label(), operation.display_name())
}

/// The history event's `stEvt:parameters` addition: the enhancements by name, in order, and the framing.
pub(crate) fn parameters(edits: &Edits) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if edits.frame().is_some_and(|frame| !frame.to_pixels.is_scale()) {
        parts.push("framed".to_owned());
    }
    let names: Vec<String> = edits.operations().iter().map(name).collect();
    if !names.is_empty() {
        let masked = if edits.masked() { " (masked)" } else { "" };
        parts.push(format!("enhanced with {}{masked}", names.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// How many times shorter the exported frame's diagonal is than the source's, measured in the source's pixels: `1`
/// for the whole photograph at any size. `None` where nothing moved the frame.
pub(crate) fn crop_factor(edits: &Edits, size: (u32, u32)) -> Option<f64> {
    let frame = edits.frame()?;
    let [[a, b], [d, e]] = frame.to_pixels.linear();
    let growth = (a * e - b * d).abs().sqrt();
    let diagonal = |(width, height): (u32, u32)| f64::from(width).hypot(f64::from(height));
    let factor = diagonal(frame.source) * growth / diagonal(size);
    factor.is_finite().then_some(factor)
}

/// A number as XMP writes it here: at most three decimals, no trailing zeros.
fn decimal(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Writes the module's XMP properties for `edits`, exported at `size`, from a source whose EXIF records
/// `focal_35mm` as its 35 mm-equivalent focal length. Every `opai:` property the source carried is replaced: it
/// described an earlier rendering.
pub(crate) fn record(xmp: &mut Xmp, edits: &Edits, size: (u32, u32), focal_35mm: Option<u32>) {
    let Some(source_type) = digital_source_type(edits) else {
        return;
    };
    xmp.remove_namespace(NAMESPACE);
    xmp.set(IPTC_EXT, "DigitalSourceType", &source_type.uri());

    let enhancements: Vec<String> = edits.operations().iter().map(describe).collect();
    if !enhancements.is_empty() {
        xmp.set_array(NAMESPACE, "Enhancements", Array::Seq, &enhancements);
    }
    if edits.masked() {
        xmp.set(NAMESPACE, "Masked", "True");
    }
    // The last lens blur is the one the pixels show; a chain runs at most one, last.
    if let Some(blur) = edits.operations().iter().rev().find_map(|operation| match operation {
        Operation::LensBlur(blur) => Some(blur),
        _ => None,
    }) {
        let (x, y) = blur.focus();
        xmp.set(NAMESPACE, "LensBlurFNumber", &blur.f_number().to_string());
        xmp.set(NAMESPACE, "FocusPointX", &x.to_string());
        xmp.set(NAMESPACE, "FocusPointY", &y.to_string());
    }
    if let Some(factor) = crop_factor(edits, size).filter(|factor| (factor - 1.0).abs() > 1e-3) {
        xmp.set(NAMESPACE, "CropFactor", &decimal(factor));
        if let Some(focal) = focal_35mm.filter(|&focal| focal > 0) {
            let effective = (f64::from(focal) * factor).round();
            xmp.set(NAMESPACE, "EffectiveFocalLengthIn35mmFormat", &decimal(effective));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::metadata::xmp::edit::ns;
    use crate::image::Affine;
    use crate::models::{Denoise, FNumber, FloatPrecision, Fraction, LensBlur, Remove, Scale, Strength, Upscale};

    fn kyoto() -> Operation {
        Upscale::kyoto(FloatPrecision::Fp16, Scale::new(2.0).expect("a scale"))
    }

    fn blur() -> Operation {
        LensBlur::lisbon(
            FNumber::new(1.4).expect("an f-number"),
            Fraction::new(0.25).expect("a fraction"),
            Fraction::new(0.75).expect("a fraction"),
        )
    }

    fn removal() -> Operation {
        Remove::lima(std::sync::Arc::default())
    }

    fn ran(operations: &[Operation]) -> Edits {
        Edits::NONE.after(operations, false, (100, 100), (100, 100))
    }

    #[test]
    fn the_strongest_term_of_the_chain_is_the_images() {
        assert_eq!(digital_source_type(&Edits::NONE), None);
        let cut = Edits::NONE.framed((100, 100), Affine::translation(-10.0, 0.0));
        assert_eq!(digital_source_type(&cut), Some(SourceType::HumanEdits));
        let denoised = Denoise::stockholm(FloatPrecision::Fp32, Strength::new(0.5).expect("a strength"));
        assert_eq!(
            digital_source_type(&ran(&[denoised.clone(), kyoto()])),
            Some(SourceType::AlgorithmicallyEnhanced)
        );
        assert_eq!(
            digital_source_type(&ran(&[removal(), denoised, blur()])),
            Some(SourceType::CompositeWithTrainedAlgorithmicMedia)
        );
        assert_eq!(
            SourceType::AlgorithmicallyEnhanced.uri(),
            "http://cv.iptc.org/newscodes/digitalsourcetype/algorithmicallyEnhanced"
        );
    }

    #[test]
    fn an_enhancement_is_named_with_its_settings_and_never_with_where_it_removed_something() {
        assert_eq!(describe(&kyoto()), format!("{}: scale=2.0", name(&kyoto())));
        let blurred = describe(&blur());
        assert!(blurred.contains("f_number=1.4") && blurred.contains("focus_x=0.25"), "{blurred}");
        assert!(!describe(&removal()).contains("mask"), "{}", describe(&removal()));
    }

    #[test]
    fn the_history_names_the_framing_and_the_enhancements() {
        let edits = Edits::NONE.framed((100, 100), Affine::translation(-10.0, 0.0)).after(
            &[kyoto()],
            true,
            (90, 100),
            (180, 200),
        );
        assert_eq!(parameters(&edits), Some(format!("framed; enhanced with {} (masked)", name(&kyoto()))));
        // An upscale grows the frame without framing anything.
        assert_eq!(parameters(&ran(&[])), None);
        assert_eq!(
            parameters(&Edits::NONE.after(&[kyoto()], false, (10, 10), (20, 20))),
            Some(format!("enhanced with {}", name(&kyoto())))
        );
    }

    #[test]
    fn a_crop_of_half_the_width_and_height_doubles_the_equivalent_focal_length_and_an_upscale_does_not() {
        let half = Edits::NONE.framed((4000, 3000), Affine::translation(-1000.0, -750.0));
        assert!((crop_factor(&half, (2000, 1500)).expect("a factor") - 2.0).abs() < 1e-9);
        let grown = half.after(&[kyoto()], false, (2000, 1500), (4000, 3000));
        assert!((crop_factor(&grown, (4000, 3000)).expect("a factor") - 2.0).abs() < 1e-9);
        assert_eq!(crop_factor(&Edits::NONE, (10, 10)), None);
    }

    #[test]
    fn the_record_is_written_beside_the_capture_fields() {
        let edits = Edits::NONE.framed((4000, 3000), Affine::translation(-1000.0, -750.0)).after(
            &[blur()],
            false,
            (2000, 1500),
            (2000, 1500),
        );
        let mut xmp = Xmp::empty();
        xmp.set(NAMESPACE, "LensBlurFNumber", "8");
        xmp.set(ns::EXIF, "FNumber", "8/1");
        record(&mut xmp, &edits, (2000, 1500), Some(26));

        assert_eq!(xmp.get(NAMESPACE, "LensBlurFNumber").as_deref(), Some("1.4"));
        assert_eq!(xmp.get(NAMESPACE, "FocusPointX").as_deref(), Some("0.25"));
        assert_eq!(xmp.get(NAMESPACE, "FocusPointY").as_deref(), Some("0.75"));
        assert_eq!(xmp.get(NAMESPACE, "CropFactor").as_deref(), Some("2"));
        assert_eq!(xmp.get(NAMESPACE, "EffectiveFocalLengthIn35mmFormat").as_deref(), Some("52"));
        assert_eq!(xmp.seq_values(NAMESPACE, "Enhancements").map(|items| items.len()), Some(1));
        assert_eq!(
            xmp.get(IPTC_EXT, "DigitalSourceType").as_deref(),
            Some("http://cv.iptc.org/newscodes/digitalsourcetype/algorithmicallyEnhanced")
        );
        // What the camera did is left as it was.
        assert_eq!(xmp.get(ns::EXIF, "FNumber").as_deref(), Some("8/1"));
        let text = String::from_utf8(xmp.to_bytes()).expect("UTF-8");
        assert!(text.contains(r#"xmlns:opai="https://vegidio.github.io/open-photo-ai/ns/1.0/""#), "{text}");
        assert!(text.contains("xmlns:Iptc4xmpExt="), "{text}");
    }

    #[test]
    fn nothing_is_recorded_for_the_source_as_it_was() {
        let mut xmp = Xmp::empty();
        xmp.set(NAMESPACE, "LensBlurFNumber", "2");
        record(&mut xmp, &Edits::NONE, (10, 10), None);
        assert_eq!(xmp.get(NAMESPACE, "LensBlurFNumber").as_deref(), Some("2"));
        assert!(!xmp.has(IPTC_EXT, "DigitalSourceType"));
    }
}
