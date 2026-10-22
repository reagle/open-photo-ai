## ADDED Requirements

### Requirement: A gain-map JPEG keeps its gain map through enhancement

Loading a JPEG that carries a gain map in a Multi-Picture Format secondary image (an Ultra HDR, Adobe `hdrgm` or ISO
21496-1 gain-map JPEG) SHALL keep that gain map, and the gain-map image's own gain-map metadata, with the loaded
image. Every image derived from it - an enhancement result, whether computed or served from the run cache, and a chain
of several - SHALL keep the same gain map, covering the same part of the scene as its pixels. A gain map that cannot
be read or decoded, whose size is implausible for the base image, or whose image carries no gain-map metadata of its
own (neither `hdrgm` XMP nor an ISO 21496-1 payload), SHALL be dropped with a logged warning, and the image SHALL
still load.

#### Scenario: A Lightroom Ultra HDR JPEG is loaded
- **WHEN** a JPEG with an ISO 21496-1 marker, `hdrgm` XMP and an MPF gain-map image is loaded
- **THEN** the loaded image carries the gain map and its metadata

#### Scenario: A gain map whose offset points past the end of the file
- **WHEN** a JPEG whose MPF index places the gain map beyond the end of the file is loaded
- **THEN** the image loads without a gain map, and a warning is logged

### Requirement: A JPEG export of a gain-map image is an Ultra HDR JPEG

Writing an image that carries a gain map as JPEG SHALL write an Ultra HDR JPEG: the base image as today, followed by
the gain map at its own resolution - never enlarged, and shrunk with a filter no worse than bilinear only where the
written image is smaller than it. A gain map neither cropped nor shrunk SHALL be written as the source's own gain-map
JPEG, byte for byte; one that was SHALL be re-encoded at the export's JPEG quality but no lower than 90, with its own
gain-map metadata as the source held it. The file SHALL have an MPF index whose sizes and offsets locate both images;
the ISO 21496-1 marker where the source had one; and XMP that keeps the gain-map version and, where the source used
one, a container directory whose gain-map length is the written one, all in the standard XMP segment. Writing it as
any other format SHALL leave the gain map out. A gain map that cannot be prepared or written, or whose primary XMP
would not fit a JPEG's standard XMP segment, SHALL be left out and the export SHALL still succeed with an SDR JPEG.

The loss assessment SHALL agree: an image that carries a gain map, written as JPEG, SHALL report no HDR loss; an HDR
gain-map source whose gain map could not be carried SHALL report `HDR gain map dropped: it could not be carried`; any
other format SHALL report the HDR loss as before.

#### Scenario: The hawk denoised and written as JPEG
- **WHEN** the Lightroom Ultra HDR JPEG (1197×1496, full-resolution gain map) is denoised and exported as JPEG
- **THEN** the file has two MPF images, the second a 1197×1496 gain map, readers that support ISO 21496-1 or Ultra HDR
recognise it as HDR, and no loss is reported

#### Scenario: Upscaled 2×
- **WHEN** the same image is upscaled 2× and exported as JPEG
- **THEN** the base is 2394×2992, the gain map stays 1197×1496, and the MPF offset of the second image is where its
  bytes start

#### Scenario: A source with a quarter-resolution gain map
- **WHEN** a JPEG of 1000×750 with a 250×188 gain map is upscaled 4× and exported as JPEG
- **THEN** the base is 4000×3000 and the gain map stays 250×188

#### Scenario: Written as PNG
- **WHEN** the hawk is exported as PNG
- **THEN** no gain map is written, and the HDR loss `HDR gain map dropped: PNG output is SDR only for now` is reported

#### Scenario: A gain map that could not be read
- **WHEN** a JPEG whose gain map is truncated is loaded and exported as JPEG
- **THEN** an SDR JPEG without gain-map XMP is written and `HDR gain map dropped: it could not be carried` is reported

#### Scenario: An export of an export
- **WHEN** the hawk is exported as JPEG and that result is exported as JPEG again
- **THEN** both files' gain maps are byte for byte the source's, and no loss is reported

#### Scenario: A secondary image with no gain-map metadata
- **WHEN** a JPEG whose MPF entry of type Gain Map points at an image with neither `hdrgm` XMP nor an ISO 21496-1
  payload is loaded and exported as JPEG
- **THEN** the image loads without a gain map, an SDR JPEG is written, and
  `HDR gain map dropped: it could not be carried` is reported, as the prediction said before the export

#### Scenario: A primary XMP too large for the standard segment
- **WHEN** an Ultra HDR JPEG whose primary XMP, once rewritten, is over 65,502 bytes is exported as JPEG
- **THEN** an SDR JPEG whose XMP, extended XMP included, declares no `hdrgm` namespace is written, the rest of the XMP
  is kept, and `HDR gain map dropped: it could not be carried` is reported
