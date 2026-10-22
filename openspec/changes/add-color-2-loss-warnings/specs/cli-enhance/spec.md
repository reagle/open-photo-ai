## ADDED Requirements

### Requirement: A result that lost HDR, bit depth or its colour profile is named in the terminal

When a written result lost anything against its input (HDR, bit depth or its colour profile, as the library reports
it), the command SHALL print `⚠️ <input path>: <loss>; <loss>` on stderr, whether or not `--verbose` is given. Off a
terminal the line SHALL follow that image's own line; on a terminal it SHALL be printed after the batch's live view
finishes, before the summary. It SHALL be yellow when colour is used and plain text otherwise, and SHALL NOT change
any image's row.

When more than one image was run and any lost something, a count line SHALL follow, naming how many images lost
something and how many lost each kind. Each loss SHALL also be logged. Losses SHALL NOT change the exit status.

#### Scenario: An Ultra HDR JPEG enhanced to JPEG, off a terminal
- **WHEN** `opai hawk.jpg -q sd --dn -o out.jpg 2>log.txt` is run on an Ultra HDR JPEG
- **THEN** stderr holds the image's line followed by
  `⚠️ hawk.jpg: HDR gain map dropped: JPEG output is SDR only for now`
- **AND** the exit status is 0

#### Scenario: Nothing lost
- **WHEN** an 8-bit sRGB JPEG is run with `-o out.jpg`
- **THEN** no `⚠️` line is printed

#### Scenario: A batch
- **WHEN** three images are run and two lose HDR
- **THEN** after their lines, `⚠️ 2 of 3 images lost something: 2 HDR` is printed
