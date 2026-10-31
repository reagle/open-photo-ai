//! What the library can be asked to run, published to the window.

// One command, and it does nothing on the way through: `opai::catalogue` is already the data a front end reads
// instead of restating the model vocabulary, and anything done to it here would be this crate having a second
// opinion about which models exist or what they take.
//
// Unfiltered, including the two families no settings row draws. Filtering it to "the families the settings dialog
// lists" would put the answer to *which enhancements are configurable* in this crate, which is exactly the
// duplication the catalogue exists to prevent — and it would be wrong twice over, because the add-enhancement menu
// and the options popovers read the same catalogue with a different subset in mind. The library's own doc settles
// the analogous case: detection is listed although the reference's frontend registry has no `dt` row, because which
// model detects faces is model vocabulary.
//
// Named for its subject rather than for the screen that asked first. A `settings.rs` holding this and the log path
// together would be a module named after one caller, which the tier rule `opai::models` states for its own files
// refuses.

use opai::{FamilyEntry, ModelTrust};

use crate::command::{Traceparent, command_span, traced_sync};

// The command's name is written once on the TypeScript side too, in `frontend/ipc/catalogue.ts`; nothing in either
// toolchain notices when one of the two is renamed alone.
/// Every enhancement family, the models each one offers, and what each model takes.
///
/// Returned as `opai` publishes it: every family in the library's own order, each variant with its codename, its
/// display label, the precisions it is published in, and its parameters with the bounds the same constructors
/// enforce. A control built from this cannot offer a model that does not exist or a value the library would refuse.
///
/// Nothing here can fail, and it needs no initialized application, so there is no error to report and no state it
/// could be asked for too early.
///
/// Fork-only, not for upstream: as `opai::installable_catalogue` serves it, so a model this fork exports carries the
/// precisions this install cannot run it at in `unavailable`. For every published model that is the library's
/// catalogue unchanged. The models directory does not change while the window is open — the fork's models are placed
/// by a script with the application closed — so the window keeps the first answer, as it always has.
#[tauri::command]
pub(crate) fn catalogue(traceparent: Traceparent) -> Vec<FamilyEntry> {
    traced_sync(command_span!("catalogue", traceparent), || {
        opai::installable_catalogue(opai::APP_NAME, ModelTrust::Published)
    })
}

#[cfg(test)]
mod tests {
    use opai::Family;

    use super::*;

    /// What the command serves, with which precisions this install can run left out: the shape on the wire is what
    /// these tests are about, and which precisions are unavailable is `opai`'s to test, against the models it knows
    /// are placed rather than whatever the machine running the suite has.
    fn served() -> Vec<FamilyEntry> {
        let mut served = catalogue(Traceparent::default());

        for variant in served.iter_mut().flat_map(|family| family.variants.iter_mut()) {
            variant.unavailable.clear();
        }

        served
    }

    #[test]
    fn every_family_the_library_names_is_published() {
        let published = served();

        // Driven from `Family::ALL` rather than from a list written here, so a family added to the library and left
        // out of the catalogue fails this rather than being noticed by whoever draws the chooser. Detection and
        // colorization are in it, which is the point: no settings row draws either, and this crate does not decide
        // that.
        for family in Family::ALL {
            assert!(
                published.iter().any(|entry| entry.family == family),
                "{family:?} is in the library's vocabulary and not on the wire"
            );
        }

        assert_eq!(published.len(), Family::ALL.len(), "the command published a family the library does not name");
    }

    #[test]
    fn a_family_with_nothing_to_adjust_arrives_with_an_empty_parameter_list() {
        // Colorization, which takes nothing. Present with an empty list rather than left out, so "this enhancement
        // has nothing to configure" is something the window is told rather than something it infers from an absence.
        let colorization = served()
            .into_iter()
            .find(|entry| entry.family == Family::Colorization)
            .expect("colorization is a family the library names");

        assert!(
            !colorization.variants.is_empty(),
            "a family with no variants is not a family a chooser can draw"
        );
        for variant in &colorization.variants {
            assert!(variant.parameters.is_empty(), "{} gained an adjustable parameter", variant.codename);
        }
    }

    #[test]
    fn it_is_the_librarys_catalogue_with_nothing_but_the_unavailable_precisions_added() {
        // Nothing is filtered, reordered or renamed on the way through, asserted against the library's own answer
        // rather than against a copy of it written here. What is unavailable is only ever a precision the library
        // publishes.
        for variant in catalogue(Traceparent::default()).iter().flat_map(|family| &family.variants) {
            assert!(
                variant.unavailable.iter().all(|precision| variant.precisions.contains(precision)),
                "{} was marked unavailable at a precision it is not published at",
                variant.codename
            );
        }

        assert_eq!(served(), opai::catalogue());
    }

    /// What actually crosses the boundary, which is the half of the contract `frontend/ipc/catalogue.ts` is written
    /// against: the family key, and a variant's four fields with the parameter bounds flattened onto the kind.
    #[test]
    fn the_wire_shape_carries_the_bounds_a_control_is_built_from() {
        let json = serde_json::to_value(served()).expect("the catalogue should serialize");
        let families = json.as_array().expect("the catalogue is a list of families");

        let sharpen = families
            .iter()
            .find(|entry| entry["family"] == "sharpen")
            .expect("sharpen is published under its snake_case family key");

        let variant = &sharpen["variants"][0];
        assert!(variant["codename"].is_string());
        assert!(variant["label"].is_string());
        assert!(variant["precisions"].is_array());

        let parameter = &variant["parameters"][0];
        assert_eq!(parameter["kind"], "range", "the parameter kind is not the tag a control switches on");
        assert!(parameter["name"].is_string());
        assert!(parameter["min"].is_number(), "a slider built from this would have no lower bound");
        assert!(parameter["max"].is_number(), "a slider built from this would have no upper bound");
    }

    /// The whole of two families' entries as they cross the boundary, pinned as literals: one whose variants take
    /// ranges and faces, and one that takes nothing. A field renamed, dropped or added shows up here as a deliberate
    /// edit rather than as a front end silently reading `undefined`.
    #[test]
    fn the_wire_shape_of_a_family_entry_is_pinned() {
        let json = serde_json::to_value(served()).expect("the catalogue should serialize");
        let entry = |family: &str| {
            json.as_array()
                .expect("the catalogue is a list of families")
                .iter()
                .find(|entry| entry["family"] == family)
                .cloned()
                .expect("the family is published")
        };

        assert_eq!(
            entry("face_recovery"),
            serde_json::json!({
                "family": "face_recovery",
                "order": 2,
                "variants": [
                    {
                        "codename": "athens",
                        "label": "Athens",
                        "precisions": ["fp32", "fp16"],
                        "parameters": [
                            { "name": "faces", "kind": "faces" },
                            { "name": "fidelity", "kind": "range", "min": 0.0, "max": 1.0, "default": 1.0 },
                        ],
                    },
                    {
                        "codename": "santorini",
                        "label": "Santorini",
                        "precisions": ["fp32", "fp16"],
                        "parameters": [{ "name": "faces", "kind": "faces" }],
                    },
                ],
            })
        );
        assert_eq!(entry("colorization")["order"], serde_json::json!(3));
        // Remove runs first, and its one parameter is a painted region rather than a control.
        assert_eq!(
            entry("remove"),
            serde_json::json!({
                "family": "remove",
                "order": 0,
                "variants": [{
                    "codename": "lima",
                    "label": "Lima",
                    "precisions": ["fp32"],
                    "parameters": [{ "name": "mask", "kind": "mask" }],
                }],
            })
        );
        // Detection is never in a chain, so it carries no place in one — absent rather than null.
        assert!(entry("detection").get("order").is_none());
        assert_eq!(
            entry("colorization")["variants"][0],
            serde_json::json!({ "codename": "delhi", "label": "Delhi", "precisions": ["fp32", "fp16"], "parameters": [] })
        );
    }
}
