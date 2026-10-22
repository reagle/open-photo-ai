## ADDED Requirements

### Requirement: A loaded JPEG's quality is estimated

Loading a JPEG SHALL record an estimate of the quality it was saved at, on the 1 to 100 scale the application writes
JPEGs with, together with whether the file's tables matched that scale exactly. A file whose tables cannot be placed,
and any image that is not a JPEG, SHALL carry no estimate. Pictures derived from a loaded image SHALL carry its
estimate.

#### Scenario: A JPEG written at quality 75

- **WHEN** a JPEG this application wrote at quality 75 is loaded
- **THEN** its estimated quality is 75 and the estimate is exact

#### Scenario: A PNG

- **WHEN** a PNG is loaded
- **THEN** it carries no quality estimate
