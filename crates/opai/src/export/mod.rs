//! What an exported photograph is called, where it goes, which format it is written in, and how its name is claimed.
//!
//! Three parts, each a question every front end asks of the same rule:
//!
//! - [`mod@format`]: the eight formats an export can be written in, spelled as the front ends spell them
//!   ([`ExportFormat`]), a [`FormatChoice`] that is one of them or Preserve, and what each can do
//!   ([`capabilities`]).
//! - [`mod@name`]: the name and the destination a source is exported under ([`name_for`], [`destination_for`]): the
//!   prefix, the source's name, the suffix and the extension the format choice decides, inside the chosen folder or
//!   beside the source. Pure: it touches no file and cannot fail, so it is cheap enough to ask on every keystroke.
//! - [`mod@claim`]: holding that destination while the file is written ([`claim()`]) — the first free numbered name
//!   without overwriting, or a hidden name renamed over the destination on [`Claim::commit`] with it — and giving it
//!   back when the write does not finish.
//! - [`mod@loss`]: what writing a picture as a format loses against its source ([`losses`], [`Loss`]): HDR (except a
//!   gain-map JPEG written as JPEG with its gain map carried), bits per channel or float samples, and the colour profile
//!   (a format that cannot hold one, a profile for another colour model, one the load discarded, a coded description
//!   nothing writes yet). Asked before
//!   the write; worded in English by `Display` and left to a front end by `Serialize`. An HDR source whose PQ or HLG
//!   is declared by an embedded ICC `cicp` tag is reported as losing HDR even where the profile is embedded: a known
//!   over-warning.
//! - [`mod@write`]: the claim, the save and the commit together ([`write()`]), which is how a front end writes one.
//!   It assesses the losses, reconciles them with what the encoder embedded, logs each as a warning and answers them
//!   on [`Written::losses`].
//!   It embeds the source's metadata as the export policy rewrites it: the EXIF with every field true of the file and
//!   its MakerNote kept where safe, the XMP with a new identity and history, the IPTC-IIM, and the ICC profile.
//!   Written as JPEG, a picture with a gain map is an Ultra HDR JPEG again (`gain_map` prepares the gain map: the
//!   source's own bytes where nothing reframed or shrank it, otherwise encoded at its own resolution, never enlarged,
//!   at the export's quality or 90); every other format leaves the gain map out.
//!   **Known caveat:** light adjustment, colour balance and colourisation change the base image's brightness or
//!   colour and leave the gain map's ratios computed against the old base, so the HDR rendition is slightly off;
//!   recomputing the gain map is a later slice.
//!
//! # Why this sits beside `image` and not inside it
//!
//! [`image`](crate::image) reads and writes one file at a path its caller names. Choosing that path — the naming
//! convention, numbering, overwriting through a hidden file — is an export policy built on top of it, and the claim is
//! not image I/O at all.

pub mod claim;
pub mod format;
pub(crate) mod gain_map;
pub mod loss;
pub(crate) mod metadata;
pub mod name;
pub mod write;

pub use claim::{Claim, claim};
pub use format::{
    ExportFormat, ExportFormats, FormatCapability, FormatChoice, QualityRange, UnknownFormat, capabilities,
};
pub use loss::{HdrCause, Loss, ProfileCause, losses, predict_blocking};
pub use name::{Named, Naming, destination_for, name_for};
pub use write::{WriteError, WriteOptions, Written, write};
