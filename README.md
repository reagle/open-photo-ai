# Open Photo AI (OPAI)

<p align="center">
<img src="docs/assets/icon.avif" width="300" alt="Open Photo AI (OPAI)"/>
<br/>
<strong>Open Photo AI</strong> is an open source alternative to the popular photo AI editor.
<br/>
It currently supports the following enhancements:
<br/><br/>
<img src="https://img.shields.io/badge/Denoise-FDEBD3?style=for-the-badge&logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjRweCIgdmlld0JveD0iMCAtOTYwIDk2MCA5NjAiIHdpZHRoPSIyNHB4IiBmaWxsPSIjMDAwMDAwIj48cGF0aCBkPSJNMTA2LTM4NnEtNi02LTYtMTR0Ni0xNHE2LTYgMTQtNnQxNCA2cTYgNiA2IDE0dC02IDE0cS02IDYtMTQgNnQtMTQtNlptMC0xNjBxLTYtNi02LTE0dDYtMTRxNi02IDE0LTZ0MTQgNnE2IDYgNiAxNHQtNiAxNHEtNiA2LTE0IDZ0LTE0LTZabTEwNS41IDMzNC41UTIwMC0yMjMgMjAwLTI0MHQxMS41LTI4LjVRMjIzLTI4MCAyNDAtMjgwdDI4LjUgMTEuNVEyODAtMjU3IDI4MC0yNDB0LTExLjUgMjguNVEyNTctMjAwIDI0MC0yMDB0LTI4LjUtMTEuNVptMC0xNjBRMjAwLTM4MyAyMDAtNDAwdDExLjUtMjguNVEyMjMtNDQwIDI0MC00NDB0MjguNSAxMS41UTI4MC00MTcgMjgwLTQwMHQtMTEuNSAyOC41UTI1Ny0zNjAgMjQwLTM2MHQtMjguNS0xMS41Wm0wLTE2MFEyMDAtNTQzIDIwMC01NjB0MTEuNS0yOC41UTIyMy02MDAgMjQwLTYwMHQyOC41IDExLjVRMjgwLTU3NyAyODAtNTYwdC0xMS41IDI4LjVRMjU3LTUyMCAyNDAtNTIwdC0yOC41LTExLjVabTAtMTYwUTIwMC03MDMgMjAwLTcyMHQxMS41LTI4LjVRMjIzLTc2MCAyNDAtNzYwdDI4LjUgMTEuNVEyODAtNzM3IDI4MC03MjB0LTExLjUgMjguNVEyNTctNjgwIDI0MC02ODB0LTI4LjUtMTEuNVptMTQ2IDMzNFEzNDAtMzc1IDM0MC00MDB0MTcuNS00Mi41UTM3NS00NjAgNDAwLTQ2MHQ0Mi41IDE3LjVRNDYwLTQyNSA0NjAtNDAwdC0xNy41IDQyLjVRNDI1LTM0MCA0MDAtMzQwdC00Mi41LTE3LjVabTAtMTYwUTM0MC01MzUgMzQwLTU2MHQxNy41LTQyLjVRMzc1LTYyMCA0MDAtNjIwdDQyLjUgMTcuNVE0NjAtNTg1IDQ2MC01NjB0LTE3LjUgNDIuNVE0MjUtNTAwIDQwMC01MDB0LTQyLjUtMTcuNVptMTQgMzA2UTM2MC0yMjMgMzYwLTI0MHQxMS41LTI4LjVRMzgzLTI4MCA0MDAtMjgwdDI4LjUgMTEuNVE0NDAtMjU3IDQ0MC0yNDB0LTExLjUgMjguNVE0MTctMjAwIDQwMC0yMDB0LTI4LjUtMTEuNVptMC00ODBRMzYwLTcwMyAzNjAtNzIwdDExLjUtMjguNVEzODMtNzYwIDQwMC03NjB0MjguNSAxMS41UTQ0MC03MzcgNDQwLTcyMHQtMTEuNSAyOC41UTQxNy02ODAgNDAwLTY4MHQtMjguNS0xMS41Wk0zODYtMTA2cS02LTYtNi0xNHQ2LTE0cTYtNiAxNC02dDE0IDZxNiA2IDYgMTR0LTYgMTRxLTYgNi0xNCA2dC0xNC02Wm0wLTcyMHEtNi02LTYtMTR0Ni0xNHE2LTYgMTQtNnQxNCA2cTYgNiA2IDE0dC02IDE0cS02IDYtMTQgNnQtMTQtNlptMTMxLjUgNDY4LjVRNTAwLTM3NSA1MDAtNDAwdDE3LjUtNDIuNVE1MzUtNDYwIDU2MC00NjB0NDIuNSAxNy41UTYyMC00MjUgNjIwLTQwMHQtMTcuNSA0Mi41UTU4NS0zNDAgNTYwLTM0MHQtNDIuNS0xNy41Wm0wLTE2MFE1MDAtNTM1IDUwMC01NjB0MTcuNS00Mi41UTUzNS02MjAgNTYwLTYyMHQ0Mi41IDE3LjVRNjIwLTU4NSA2MjAtNTYwdC0xNy41IDQyLjVRNTg1LTUwMCA1NjAtNTAwdC00Mi41LTE3LjVabTE0IDMwNlE1MjAtMjIzIDUyMC0yNDB0MTEuNS0yOC41UTU0My0yODAgNTYwLTI4MHQyOC41IDExLjVRNjAwLTI1NyA2MDAtMjQwdC0xMS41IDI4LjVRNTc3LTIwMCA1NjAtMjAwdC0yOC41LTExLjVabTAtNDgwUTUyMC03MDMgNTIwLTcyMHQxMS41LTI4LjVRNTQzLTc2MCA1NjAtNzYwdDI4LjUgMTEuNVE2MDAtNzM3IDYwMC03MjB0LTExLjUgMjguNVE1NzctNjgwIDU2MC02ODB0LTI4LjUtMTEuNVpNNTQ2LTEwNnEtNi02LTYtMTR0Ni0xNHE2LTYgMTQtNnQxNCA2cTYgNiA2IDE0dC02IDE0cS02IDYtMTQgNnQtMTQtNlptMC03MjBxLTYtNi02LTE0dDYtMTRxNi02IDE0LTZ0MTQgNnE2IDYgNiAxNHQtNiAxNHEtNiA2LTE0IDZ0LTE0LTZabTE0NS41IDYxNC41UTY4MC0yMjMgNjgwLTI0MHQxMS41LTI4LjVRNzAzLTI4MCA3MjAtMjgwdDI4LjUgMTEuNVE3NjAtMjU3IDc2MC0yNDB0LTExLjUgMjguNVE3MzctMjAwIDcyMC0yMDB0LTI4LjUtMTEuNVptMC0xNjBRNjgwLTM4MyA2ODAtNDAwdDExLjUtMjguNVE3MDMtNDQwIDcyMC00NDB0MjguNSAxMS41UTc2MC00MTcgNzYwLTQwMHQtMTEuNSAyOC41UTczNy0zNjAgNzIwLTM2MHQtMjguNS0xMS41Wm0wLTE2MFE2ODAtNTQzIDY4MC01NjB0MTEuNS0yOC41UTcwMy02MDAgNzIwLTYwMHQyOC41IDExLjVRNzYwLTU3NyA3NjAtNTYwdC0xMS41IDI4LjVRNzM3LTUyMCA3MjAtNTIwdC0yOC41LTExLjVabTAtMTYwUTY4MC03MDMgNjgwLTcyMHQxMS41LTI4LjVRNzAzLTc2MCA3MjAtNzYwdDI4LjUgMTEuNVE3NjAtNzM3IDc2MC03MjB0LTExLjUgMjguNVE3MzctNjgwIDcyMC02ODB0LTI4LjUtMTEuNVpNODI2LTM4NnEtNi02LTYtMTR0Ni0xNHE2LTYgMTQtNnQxNCA2cTYgNiA2IDE0dC02IDE0cS02IDYtMTQgNnQtMTQtNlptMC0xNjBxLTYtNi02LTE0dDYtMTRxNi02IDE0LTZ0MTQgNnE2IDYgNiAxNHQtNiAxNHEtNiA2LTE0IDZ0LTE0LTZaIi8+PC9zdmc+"/>
<img src="https://img.shields.io/badge/Face Recovery-F9BE5A?style=for-the-badge&logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjRweCIgdmlld0JveD0iMCAtOTYwIDk2MCA5NjAiIHdpZHRoPSIyNHB4IiBmaWxsPSIjMDAwMDAwIj4KICAgIDxwYXRoIGQ9Ik0zNjAtMzkwcS0yMSAwLTM1LjUtMTQuNVQzMTAtNDQwcTAtMjEgMTQuNS0zNS41VDM2MC00OTBxMjEgMCAzNS41IDE0LjVUNDEwLTQ0MHEwIDIxLTE0LjUgMzUuNVQzNjAtMzkwWm0yNDAgMHEtMjEgMC0zNS41LTE0LjVUNTUwLTQ0MHEwLTIxIDE0LjUtMzUuNVQ2MDAtNDkwcTIxIDAgMzUuNSAxNC41VDY1MC00NDBxMCAyMS0xNC41IDM1LjVUNjAwLTM5MFpNNDgwLTE2MHExMzQgMCAyMjctOTN0OTMtMjI3cTAtMjQtMy00Ni41VDc4Ni01NzBxLTIxIDUtNDIgNy41dC00NCAyLjVxLTkxIDAtMTcyLTM5VDM5MC03MDhxLTMyIDc4LTkxLjUgMTM1LjVUMTYwLTQ4NnY2cTAgMTM0IDkzIDIyN3QyMjcgOTNabTAgODBxLTgzIDAtMTU2LTMxLjVUMTk3LTE5N3EtNTQtNTQtODUuNS0xMjdUODAtNDgwcTAtODMgMzEuNS0xNTZUMTk3LTc2M3E1NC01NCAxMjctODUuNVQ0ODAtODgwcTgzIDAgMTU2IDMxLjVUNzYzLTc2M3E1NCA1NCA4NS41IDEyN1Q4ODAtNDgwcTAgODMtMzEuNSAxNTZUNzYzLTE5N3EtNTQgNTQtMTI3IDg1LjVUNDgwLTgwWm0tNTQtNzE1cTQyIDcwIDExNCAxMTIuNVQ3MDAtNjQwcTE0IDAgMjctMS41dDI3LTMuNXEtNDItNzAtMTE0LTExMi41VDQ4MC04MDBxLTE0IDAtMjcgMS41dC0yNyAzLjVaTTE3Ny01ODFxNTEtMjkgODktNzV0NTctMTAzcS01MSAyOS04OSA3NXQtNTcgMTAzWm0yNDktMjE0Wm0tMTAzIDM2WiIvPgo8L3N2Zz4="/>
<img src="https://img.shields.io/badge/Colorization-D9698A?style=for-the-badge&logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjRweCIgdmlld0JveD0iMCAtOTYwIDk2MCA5NjAiIHdpZHRoPSIyNHB4IiBmaWxsPSIjRkZGRkZGIj48cGF0aCBkPSJNNDQwLTgwcS0zMyAwLTU2LjUtMjMuNVQzNjAtMTYwdi0xNjBIMjQwcS0zMyAwLTU2LjUtMjMuNVQxNjAtNDAwdi0yODBxMC02NiA0Ny0xMTN0MTEzLTQ3aDQ4MHY0NDBxMCAzMy0yMy41IDU2LjVUNzIwLTMyMEg2MDB2MTYwcTAgMzMtMjMuNSA1Ni41VDUyMC04MGgtODBaTTI0MC01NjBoNDgwdi0yMDBoLTQwdjE2MGgtODB2LTE2MGgtNDB2ODBoLTgwdi04MEgzMjBxLTMzIDAtNTYuNSAyMy41VDI0MC02ODB2MTIwWm0wIDE2MGg0ODB2LTgwSDI0MHY4MFptMCAwdi04MCA4MFoiLz48L3N2Zz4="/>
<br/>
<img src="https://img.shields.io/badge/Light Adjustment-53C0E0?style=for-the-badge&logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjRweCIgdmlld0JveD0iMCAtOTYwIDk2MCA5NjAiIHdpZHRoPSIyNHB4IiBmaWxsPSIjZTNlM2UzIj48cGF0aCBkPSJNNDgwLTM2MHE1MCAwIDg1LTM1dDM1LTg1cTAtNTAtMzUtODV0LTg1LTM1cS01MCAwLTg1IDM1dC0zNSA4NXEwIDUwIDM1IDg1dDg1IDM1Wm0wIDgwcS04MyAwLTE0MS41LTU4LjVUMjgwLTQ4MHEwLTgzIDU4LjUtMTQxLjVUNDgwLTY4MHE4MyAwIDE0MS41IDU4LjVUNjgwLTQ4MHEwIDgzLTU4LjUgMTQxLjVUNDgwLTI4MFpNMjAwLTQ0MEg0MHYtODBoMTYwdjgwWm03MjAgMEg3NjB2LTgwaDE2MHY4MFpNNDQwLTc2MHYtMTYwaDgwdjE2MGgtODBabTAgNzIwdi0xNjBoODB2MTYwaC04MFpNMjU2LTY1MGwtMTAxLTk3IDU3LTU5IDk2IDEwMC01MiA1NlptNDkyIDQ5Ni05Ny0xMDEgNTMtNTUgMTAxIDk3LTU3IDU5Wm0tOTgtNTUwIDk3LTEwMSA1OSA1Ny0xMDAgOTYtNTYtNTJaTTE1NC0yMTJsMTAxLTk3IDU1IDUzLTk3IDEwMS01OS01N1ptMzI2LTI2OFoiLz48L3N2Zz4="/>
<img src="https://img.shields.io/badge/Color Balance-C3E88D?style=for-the-badge&logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjRweCIgdmlld0JveD0iMCAtOTYwIDk2MCA5NjAiIHdpZHRoPSIyNHB4IiBmaWxsPSIjMDAwMDAwIj48cGF0aCBkPSJNNDgwLTgwcS04MiAwLTE1NS0zMS41dC0xMjcuNS04NlExNDMtMjUyIDExMS41LTMyNVQ4MC00ODBxMC04MyAzMi41LTE1NnQ4OC0xMjdRMjU2LTgxNyAzMzAtODQ4LjVUNDg4LTg4MHE4MCAwIDE1MSAyNy41dDEyNC41IDc2cTUzLjUgNDguNSA4NSAxMTVUODgwLTUxOHEwIDExNS03MCAxNzYuNVQ2NDAtMjgwaC03NHEtOSAwLTEyLjUgNXQtMy41IDExcTAgMTIgMTUgMzQuNXQxNSA1MS41cTAgNTAtMjcuNSA3NFQ0ODAtODBabTAtNDAwWm0tMTc3IDIzcTE3LTE3IDE3LTQzdC0xNy00M3EtMTctMTctNDMtMTd0LTQzIDE3cS0xNyAxNy0xNyA0M3QxNyA0M3ExNyAxNyA0MyAxN3Q0My0xN1ptMTIwLTE2MHExNy0xNyAxNy00M3QtMTctNDNxLTE3LTE3LTQzLTE3dC00MyAxN3EtMTcgMTctMTcgNDN0MTcgNDNxMTcgMTcgNDMgMTd0NDMtMTdabTIwMCAwcTE3LTE3IDE3LTQzdC0xNy00M3EtMTctMTctNDMtMTd0LTQzIDE3cS0xNyAxNy0xNyA0M3QxNyA0M3ExNyAxNyA0MyAxN3Q0My0xN1ptMTIwIDE2MHExNy0xNyAxNy00M3QtMTctNDNxLTE3LTE3LTQzLTE3dC00MyAxN3EtMTcgMTctMTcgNDN0MTcgNDNxMTcgMTcgNDMgMTd0NDMtMTdaTTQ4MC0xNjBxOSAwIDE0LjUtNXQ1LjUtMTNxMC0xNC0xNS0zM3QtMTUtNTdxMC00MiAyOS02N3Q3MS0yNWg3MHE2NiAwIDExMy0zOC41VDgwMC01MThxMC0xMjEtOTIuNS0yMDEuNVQ0ODgtODAwcS0xMzYgMC0yMzIgOTN0LTk2IDIyN3EwIDEzMyA5My41IDIyNi41VDQ4MC0xNjBaIi8+PC9zdmc+"/>
<img src="https://img.shields.io/badge/Sharpen-F55951?style=for-the-badge&logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjRweCIgdmlld0JveD0iMCAtOTYwIDk2MCA5NjAiIHdpZHRoPSIyNHB4IiBmaWxsPSIjRkZGRkZGIj48cGF0aCBkPSJtODAtMTYwIDQwMC02NDAgNDAwIDY0MEg4MFptMTQ0LTgwaDUxMkw0ODAtNjUwIDIyNC0yNDBabTI1Ni0yMDVaIi8+PC9zdmc+"/>
<br/>
<img src="https://img.shields.io/badge/Upscale-984E7D?style=for-the-badge&logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjRweCIgdmlld0JveD0iMCAtOTYwIDk2MCA5NjAiIHdpZHRoPSIyNHB4IiBmaWxsPSIjZTNlM2UzIj48cGF0aCBkPSJNMTIwLTEyMHYtMzIwaDgwdjE4NGw1MDQtNTA0SDUyMHYtODBoMzIwdjMyMGgtODB2LTE4NEwyNTYtMjAwaDE4NHY4MEgxMjBaIi8+PC9zdmc+"/>
</p>

## 🍴 About this fork

> This is [Joseph Reagle](https://reagle.org/joseph/)'s fork of [vegidio/open-photo-ai](https://github.com/vegidio/open-photo-ai). Everything below this section is the upstream README.

**Why it exists.** I use Open Photo AI in a Lightroom-centred photo workflow on macOS, and wanted a few things that workflow needs: colour profiles and metadata that survive an export, Ultra HDR (gain map) JPEGs that stay HDR, keyboard shortcuts, a CLI that is pleasant for batches, and a few new enhancements (Restore, Grain, masks, Remove, Lens Blur). The intent is to contribute this work back upstream, not to maintain a separate product; first submissions are open upstream (issues #63–#67, #71, #74, #75; PRs #68–#69).

**Why a fork.** Some of the work depends on changes to [vegidio/rust-sak](https://github.com/vegidio/rust-sak) (the project's shared library) that have to be released upstream first. Until then they live in my fork, [reagle/rust-sak](https://github.com/reagle/rust-sak), under tags such as `26.11.0-meta.7`, and this fork picks them up through a single `[patch]` entry in `Cargo.toml`. Only the colour stack (step 10 below) adds that entry; everything after it inherits it.

**How it was made.** The changes were planned and implemented with Claude (Anthropic's AI assistant, via Claude Code), using the project's own [OpenSpec](https://github.com/reagle/open-photo-ai/tree/roadmap/openspec/changes) process, with independent AI reviews for correctness and security. I reviewed and tested them and am responsible for them. Every commit carries a `Co-Authored-By: Claude` trailer.

**How the branches are arranged (2026-11-02).** Every series is based on upstream 26.11.0 (`main` at f8e58d8). Every branch is merge-free. Four small ones sit directly on upstream `main`; everything else is one stack, in the merge order below, each series on the one before it. Step 1 rides on copies of PR #68's two commits rebased onto current `main`: drop them once #68 is merged. A series lists only its own commits, so compare a branch with the one before it. "Needs" names the earlier steps a series uses; "order only" means it is stacked for the merge order and uses nothing before it. Sizes are commits and lines added/removed against the step before it.

### Review map, in merge order

On upstream `main`, independent of each other:

| Branch | What it does | Size | Upstream | Status |
|---|---|---|---|---|
| [`cli-output-default`](https://github.com/reagle/open-photo-ai/tree/cli-output-default) → [`cli-report-enhancements`](https://github.com/reagle/open-photo-ai/tree/cli-report-enhancements) | CLI: without `-o`, write `photo_opai.jpg` beside the source; `--verbose` names what each image received | 2, +859 −126 | PR [#68](https://github.com/vegidio/open-photo-ai/pull/68) | Open PR |
| [`gui-macos-window-state`](https://github.com/reagle/open-photo-ai/tree/gui-macos-window-state) | Remembers the window's size, position and maximised state | 1, +92 −11 | PR [#69](https://github.com/vegidio/open-photo-ai/pull/69) | Open PR |
| [`fix-coreml-duplicate-weights`](https://github.com/reagle/open-photo-ai/tree/fix-coreml-duplicate-weights) | Shares the two identical weight copies CoreML keeps per engine | 1, +255 −1 | [#61](https://github.com/vegidio/open-photo-ai/issues/61) | Tested on macOS |
| [`fix-gui-drawer-stays-open`](https://github.com/reagle/open-photo-ai/tree/fix-gui-drawer-stays-open) | The image drawer folds only when the preview is clicked (a behaviour change, to discuss first) | 1, +58 −12 | [#60](https://github.com/vegidio/open-photo-ai/issues/60) | Tested on macOS |

The stack, bottom to top:

| # | Branch (slices) | What it does | Needs | Size | OpenSpec · upstream | Status |
|---|---|---|---|---|---|---|
| 1 | [`cli-help-headings`](https://github.com/reagle/open-photo-ai/tree/cli-help-headings) | `--help` grouped under headings, wrapped to the terminal | #68 | 1, +88 −23 | — | Tested on macOS |
| 2 | [`cli-gui-settings`](https://github.com/reagle/open-photo-ai/tree/cli-gui-settings) | The CLI follows the GUI's saved settings below flags and `OPAI_AP_*` | #68 | 3, +1707 −114 | [change](https://github.com/reagle/open-photo-ai/tree/roadmap/openspec/changes/cli-gui-settings) · [#65](https://github.com/vegidio/open-photo-ai/issues/65) | Tested on macOS |
| 3 | [`gui-macos-open-with`](https://github.com/reagle/open-photo-ai/tree/gui-macos-open-with) → [`cli-gui-open`](https://github.com/reagle/open-photo-ai/tree/cli-gui-open) | Finder "Open With"; `opai --gui photo.jpg` opens photographs in the app | 1–2 (CLI options) | 2, +1139 −76 | [change](https://github.com/reagle/open-photo-ai/tree/roadmap/openspec/changes/gui-macos-open-with) · [#64](https://github.com/vegidio/open-photo-ai/issues/64) | Tested on macOS |
| 4 | [`add-gui-shortcuts-1-defaults`](https://github.com/reagle/open-photo-ai/tree/add-gui-shortcuts-1-defaults) → [`-2-settings`](https://github.com/reagle/open-photo-ai/tree/add-gui-shortcuts-2-settings) → [`-3-discoverable`](https://github.com/reagle/open-photo-ai/tree/add-gui-shortcuts-3-discoverable) → [`-4-app-actions`](https://github.com/reagle/open-photo-ai/tree/add-gui-shortcuts-4-app-actions) → [`-5-editor-actions`](https://github.com/reagle/open-photo-ai/tree/add-gui-shortcuts-5-editor-actions) | Keyboard shortcuts, a Keyboard settings page, a `?` list, the macOS app menu, editor keys | order only | 6, +8361 −251 | [change](https://github.com/reagle/open-photo-ai/tree/roadmap/openspec/changes/add-gui-shortcuts-1-defaults) · [#60](https://github.com/vegidio/open-photo-ai/issues/60) | Tested on macOS; hint tooltips hand check pending |
| 5 | [`add-sharpen-kazan`](https://github.com/reagle/open-photo-ai/tree/add-sharpen-kazan) → [`add-restore-family`](https://github.com/reagle/open-photo-ai/tree/add-restore-family) | Kazan (PiSA-SR), then a Restore family for generative restorers; Osaka moves there | 1–2 (CLI) | 6, +2957 −979 | — | Tested on macOS |
| 6 | [`add-restore-grain`](https://github.com/reagle/open-photo-ai/tree/add-restore-grain) → [`add-grain-family`](https://github.com/reagle/open-photo-ai/tree/add-grain-family) | Grain measured per luminance, then a Grain family last in every chain (`--grain`) | 5 | 8, +2761 −105 | [restore grain](https://github.com/reagle/open-photo-ai/tree/openspec-restore-grain/openspec/changes/add-restore-grain) · [grain family](https://github.com/reagle/open-photo-ai/tree/openspec-grain-family/openspec/changes/add-grain-family) | Tested on macOS |
| 7 | [`gui-descriptive-progress`](https://github.com/reagle/open-photo-ai/tree/gui-descriptive-progress) | Says what is running ("Denoising", "Compiling Kazan model", first-compile time) | order only | 3, +1478 −187 | — | Tested on macOS |
| 8 | [`add-masking-1-core`](https://github.com/reagle/open-photo-ai/tree/add-masking-1-core) → [`add-masking-2-brush`](https://github.com/reagle/open-photo-ai/tree/add-masking-2-brush) | Masks: `--mask`, and a brush in the GUI, so enhancements apply only where painted | order only | 5, +6680 −58 | [change](https://github.com/reagle/open-photo-ai/tree/openspec-masking/openspec/changes/add-masking-1-core) | Tested on macOS |
| 9 | [`add-remove-1-core`](https://github.com/reagle/open-photo-ai/tree/add-remove-1-core) → [`add-remove-2-gui`](https://github.com/reagle/open-photo-ai/tree/add-remove-2-gui) | Remove (LaMa "Lima"): `--remove mask.png`, painted in the GUI | 8 | 3, +4141 −369 | [change](https://github.com/reagle/open-photo-ai/tree/openspec-remove/openspec/changes/add-remove-1-core) | Tested on macOS |
| 9a | [`fix-gui-original-after-remove`](https://github.com/reagle/open-photo-ai/tree/fix-gui-original-after-remove) (on 9) | Serves the opened photograph after a run that handed it back unchanged; to fold into 9 | 9 | 1, +33 −11 | — | Tested on macOS |
| 10 | [`add-color-1-icc-profile`](https://github.com/reagle/open-photo-ai/tree/add-color-1-icc-profile) → [`preserve-metadata-1-capture`](https://github.com/reagle/open-photo-ai/tree/preserve-metadata-1-capture) → [`-2-exif`](https://github.com/reagle/open-photo-ai/tree/preserve-metadata-2-exif) → [`-3-xmp`](https://github.com/reagle/open-photo-ai/tree/preserve-metadata-3-xmp) → [`add-color-2-loss-warnings`](https://github.com/reagle/open-photo-ai/tree/add-color-2-loss-warnings) → [`-3-loss-warnings-gui`](https://github.com/reagle/open-photo-ai/tree/add-color-3-loss-warnings-gui) → [`-4-ultra-hdr-gainmap`](https://github.com/reagle/open-photo-ai/tree/add-color-4-ultra-hdr-gainmap) | ICC profiles, EXIF/IPTC/XMP kept and made true, loss warnings, Ultra HDR gain maps kept | rust-sak `26.11.0-color.1`, `-meta.1` … `-meta.7` | 11, +8166 −224 | [change](https://github.com/reagle/open-photo-ai/tree/roadmap/openspec/changes/add-color-1-icc-profile) · [#63](https://github.com/vegidio/open-photo-ai/issues/63) | Tested on macOS |
| 11 | [`add-remove-3-hdr-gainmap`](https://github.com/reagle/open-photo-ai/tree/add-remove-3-hdr-gainmap) | Remove fills the gain map too | 9, 10 | 1, +1213 −55 | [change](https://github.com/reagle/open-photo-ai/tree/openspec-remove/openspec/changes/add-remove-3-hdr-gainmap) | Tested on macOS |
| 12 | [`add-lens-blur-5-hdr-gainmap`](https://github.com/reagle/open-photo-ai/tree/add-lens-blur-5-hdr-gainmap) → [`add-lens-blur-8-matting-hdr`](https://github.com/reagle/open-photo-ai/tree/add-lens-blur-8-matting-hdr) | Lens Blur: depth (MoGe-2 "Lisbon"), layered render, `--lb`, GUI panel, gain map (slice 5), then a matted subject (ViTMatte "Porto", BiRefNet "Bilbao") and edge fixes | 10 (gain map); 9, 11 order only | 22, +18824 −353 | [changes](https://github.com/reagle/open-photo-ai/tree/openspec-lens-blur-matte/openspec/changes) · [#74](https://github.com/vegidio/open-photo-ai/issues/74) | Tested on macOS |
| 13 | [`add-remove-4-lens-blur`](https://github.com/reagle/open-photo-ai/tree/add-remove-4-lens-blur) → [`add-masking-3-lens-blur`](https://github.com/reagle/open-photo-ai/tree/add-masking-3-lens-blur) | Depth and matte from the removed photograph; a mask kept in focus (`--mask-use`) | 8, 11, 12 | 2, +1474 −170 | [change](https://github.com/reagle/open-photo-ai/tree/openspec-masking/openspec/changes/add-masking-3-lens-blur) | Tested on macOS |
| 13a | [`add-remove-5-grain`](https://github.com/reagle/open-photo-ai/tree/add-remove-5-grain) (on `add-remove-4-lens-blur`) | A fill made from a reduced crop gets the photograph's grain | 11 | 1, +560 −16 | — | Tested on macOS |
| 14 | [`add-gui-open-recent`](https://github.com/reagle/open-photo-ai/tree/add-gui-open-recent) | File > Open Recent (and a proposal for session restore) | 2–4 | 2, +1559 −35 | [change](https://github.com/reagle/open-photo-ai/tree/add-gui-open-recent/openspec/changes/add-gui-session-restore) | Tested on macOS |
| 15 | [`gui-enhancement-groups`](https://github.com/reagle/open-photo-ai/tree/gui-enhancement-groups) | Headings in the Add enhancement menu | 5, 6, 9, 12 | 1, +200 −21 | — | Tested on macOS |
| 16 | [`cli-gui-lens-blur`](https://github.com/reagle/open-photo-ai/tree/cli-gui-lens-blur) | `opai --gui --lens-blur` opens with a Lens Blur set | 3, 12 | 1, +600 −16 | — | Tested on macOS |
| 17 | [`fix-lens-blur-trimap-1-depth-step`](https://github.com/reagle/open-photo-ai/tree/fix-lens-blur-trimap-1-depth-step) → [`-3-strands`](https://github.com/reagle/open-photo-ai/tree/fix-lens-blur-trimap-3-strands) → [`-4-false-holes`](https://github.com/reagle/open-photo-ai/tree/fix-lens-blur-trimap-4-false-holes) → [`-5-firm-matte`](https://github.com/reagle/open-photo-ai/tree/fix-lens-blur-trimap-5-firm-matte) → [`-6-porto-slim`](https://github.com/reagle/open-photo-ai/tree/fix-lens-blur-trimap-6-porto-slim) → [`-7-refine`](https://github.com/reagle/open-photo-ai/tree/fix-lens-blur-trimap-7-refine) | Better subject edges (hair, holes), and a Refine brush (`--lb-refine`) | 12; 8 (brush) | 8, +1804 −176 | [#74](https://github.com/vegidio/open-photo-ai/issues/74) | Tested on macOS |
| 17a | [`add-lens-blur-small-notice`](https://github.com/reagle/open-photo-ai/tree/add-lens-blur-small-notice) (on 17) | Says when a lens blur adds too little to see | 17 | 1, +366 −40 | — | Hand check pending |
| 17b | [`gui-open-panel-on-add`](https://github.com/reagle/open-photo-ai/tree/gui-open-panel-on-add) → [`gui-consistent-input`](https://github.com/reagle/open-photo-ai/tree/gui-consistent-input) (on 17) | Opens a new enhancement's panel; one set of input rules (Return, scroll pans, pinch zooms, Space pans) | 4, 8 | 5, +1824 −214 | [change](https://github.com/reagle/open-photo-ai/tree/gui-consistent-input/openspec/changes/add-gui-shortcuts-6-consistent-input) | Hand check pending |
| 17c | [`gui-mask-clarity`](https://github.com/reagle/open-photo-ai/tree/gui-mask-clarity) (on 17) | Says which way round the mask is | 6, 13, 17 | 2, +611 −109 | — | Hand check pending |
| 18 | [`fix-sharpen-tile-anchor`](https://github.com/reagle/open-photo-ai/tree/fix-sharpen-tile-anchor) (on 16) | Sharpen tiles anchored to their input: no grid from Moscow, no mottle from Petersburg | 5 | 1, +670 −61 | — | Tested on macOS |
| 19 | [`cli-gui-sharpen`](https://github.com/reagle/open-photo-ai/tree/cli-gui-sharpen) → [`add-gui-batch-1-cancel`](https://github.com/reagle/open-photo-ai/tree/add-gui-batch-1-cancel) … [`-7-release-bilbao`](https://github.com/reagle/open-photo-ai/tree/add-gui-batch-7-release-bilbao) (on 16) | `--gui --sharpen`; process every opened photograph by paging, with Pause/Resume, and memory limits | 16 | 8, +4334 −468 | — | Tested on macOS |
| 20 | [`add-metadata-edits-1-pixel-fields`](https://github.com/reagle/open-photo-ai/tree/add-metadata-edits-1-pixel-fields) → [`-2-edit-record`](https://github.com/reagle/open-photo-ai/tree/add-metadata-edits-2-edit-record) → [`-3-info-card`](https://github.com/reagle/open-photo-ai/tree/add-metadata-edits-3-info-card) (on 16) | Metadata follows the pixels; an edit record in XMP; an info card (`I`) | 10, 12 | 3, +2690 −22 | — | Tested on macOS |
| 20a | [`gui-info-card-larger`](https://github.com/reagle/open-photo-ai/tree/gui-info-card-larger) (on 20) | The info card two points larger | 20 | 1, +1 −1 | — | Tested on macOS |
| 20b | [`add-gui-hdr-badge`](https://github.com/reagle/open-photo-ai/tree/add-gui-hdr-badge) → [`add-export-ultra-hdr-format`](https://github.com/reagle/open-photo-ai/tree/add-export-ultra-hdr-format) (on 20) | An HDR badge; JPG (Ultra HDR) offered beside plain JPG (`--format uhdr`) | 10, 20 | 2, +1171 −154 | [#63](https://github.com/vegidio/open-photo-ai/issues/63) | Hand check pending |

**Fork-only — skip.** [`fork-only`](https://github.com/reagle/open-photo-ai/tree/fork-only) (on 17c) holds what upstream should not take: a local listing for the models this fork exports itself (a copy of `fork-model-listing`), [`fix-model-availability`](https://github.com/reagle/open-photo-ai/tree/fix-model-availability) (which depends on it), the benchmark dumps (including [`fix-lens-blur-trimap-2-bench-dumps`](https://github.com/reagle/open-photo-ai/tree/fix-lens-blur-trimap-2-bench-dumps)), and the version stamp for local builds. [`integration`](https://github.com/reagle/open-photo-ai/tree/integration) merges every branch above plus `fork-only`, for testing only; the only resolutions it carries are where side branches meet each other (e.g. the batch series and consistent input both adding a setting); everything where a later series meets an earlier one is in the later series itself. `main` carries only this section.

**Superseded copies and parked work — skip.** `add-lens-blur-1-depth` … `-4-gui`, `-7-matte-render`, `-8-matting`, `fork-model-listing`, `fix-lens-blur-seed-colour` and `fix-lens-blur-trimap-on-matting(-hdr)` are an earlier copy of the lens-blur line on `main` without the colour stack (step 12 supersedes them). `wip-*` branches are experiments.

**Trying it.** Build the `integration` branch with `just build-fork gui` (or `cli`); the result is in `build/`. It reports its version as the upstream release it is based on plus fork build metadata (e.g. `26.11.0+reagle.20261102.7f61a41a`), so it doesn't claim an update is available, and that string is what exported images record as their Software. It shares models and settings with an installed Open Photo AI, so quit one before opening the other.

**Status.** Developed and tested on macOS (Apple silicon) only; Windows and Linux are untested. Proposed and waiting for the maintainer's view before any work: linear DNG output for camera RAW files (until then, `--format preserve` writes camera RAW sources as 16-bit TIFF), and an automatic export quality (match the source, live size estimate). The fork's own models (Kazan, Lisbon, Porto, Bilbao, Lima) are exported locally and published nowhere yet; see [#72](https://github.com/vegidio/open-photo-ai/issues/72) on licences.

## 💡 Motivation

There are many excellent AI-based photo editing tools available today, ranging from open-source solutions – often powerful but complex to set up and use, such as ComfyUI – to commercial products that favor ease of use over deep customization, like those from Topaz Labs.

I have long used both ComfyUI and Topaz Labs solutions, choosing between them depending on the task. Recently, however, Topaz Labs moved from a perpetual license to a subscription-based pricing model, a change I strongly dislike. As a developer, I am happy to pay for software that is useful for me, whether open source or proprietary, but I believe subscription models are rarely designed to benefit users and instead primarily serve company interests.

That is why I created this project: an open-source alternative to Topaz Photo AI. It may never match the same level of polish or performance – Topaz has teams of full-time engineers, while this is a solo project built in my spare time – but I have ambitious goals and aim to reach feature parity with their product over time.

## ⬇️ Installation

This app has versions for Windows, macOS, and Linux. Download the [latest release](https://github.com/vegidio/open-photo-ai/releases) that matches your computer architecture and operating system.

However, the recommended (and easiest) way to install **Open Photo AI** is using one of the following scripts; copy and paste the command below in the terminal, and the script will automatically detect and install the correct version of the app:

### macOS & Linux

```bash
curl -fsSL https://vegidio.github.io/open-photo-ai/install.sh | sh
```

### Windows (PowerShell)

```bash
irm https://vegidio.github.io/open-photo-ai/install.ps1 | iex
```

### Choosing what to install

By default the scripts install both the GUI and the CLI. Set `OPAI_INSTALL` to `gui` or `cli` to install only one of them:

```bash
# macOS & Linux
curl -fsSL https://vegidio.github.io/open-photo-ai/install.sh | OPAI_INSTALL=cli sh

# Windows (PowerShell)
$env:OPAI_INSTALL='cli'; irm https://vegidio.github.io/open-photo-ai/install.ps1 | iex
```

## 🖼️ Usage

There are two ways to use **Open Photo AI**: using the GUI or the CLI.

The GUI is the easiest way to use the app, with an intuitive interface that allows you to enhance images with just a few clicks. The CLI is more advanced and allows you to enhance images in a more automated way.

### GUI ([video](https://www.youtube.com/watch?v=NdSfeyiXPf8) 🎥)

<p align="center">
<img src="docs/assets/gui-screenshot.avif" width="80%" alt="Open Photo AI - GUI"/>
</p>

1. Click on the button `Browse images` to select one or more images that you would like to enhance.
2. The images are enhanced automatically or manually depending on the toggle `Autopilot` in the top right side of the screen:
   - If enabled, the app will automatically analyse the images and suggest enhancements for them.
   - If disabled, you will need to select the enhancements yourself, using the button `Add enhancement`.
3. Select one or more images that you would like to export on the image drawer at the bottom of the screen.
4. Click on the button `Export image`, select the location and image format, then click on `Export`.

### CLI

<p align="center">
<img src="docs/assets/cli-screenshot.avif" width="80%" alt="Open Photo AI - CLI"/>
</p>

The CLI enhances one image per command:

```bash
opai <input> <enhancement flags...> [--quality hd|sd] --output <file>
```

For the complete reference of how to use the CLI, see the [CLI Reference](docs/CLI_REFERENCE.md).

#### Examples:

```bash
# Upscale using the default model, at the scale the photo needs
opai photo.jpg --up -o photo-large.png

# Restore faces using the default model, then upscale 2x with Kyoto, and save as WebP
opai portrait.jpg --fr --up kyoto --scale 2 -o portrait.webp

# Upscale with Osaka using its SD quality
opai scan.png --up osaka -q sd -o scan.avif

# Denoise twice as strongly with Gothenburg, then adjust light with Paris
opai night.jpg --dn gothenburg --dn-str 200 --la --la-bias -20 -o night-clean.jpg
```

## 💻 System Requirements

|          | Windows                             | Linux                                  | macOS                               |
|----------|-------------------------------------|----------------------------------------|-------------------------------------|
| **OS**   | Windows 10+                         | Ubuntu 24.04, Fedora 40, Debian 13     | macOS 12+, Apple Silicon only       |
| **RAM**  | 8 GB (16 GB recommended)            | 8 GB (16 GB recommended)               | 8 GB (16 GB recommended)            |
| **GPU**  | Optional, any DirectX 12 GPU        | Optional, any GPU with a Vulkan driver | Built-in (CoreML/Metal)             |
| **Disk** | 5 GB (25 GB to install every model) | 5 GB (25 GB to install every model)    | 3 GB (20 GB to install every model) |

The app runs on any of these systems even without a GPU, falling back to the CPU when no supported accelerator is found; it will just be much slower.

### NVIDIA acceleration (CUDA & TensorRT)

On Windows x64 and Linux, the app can use CUDA and TensorRT for the best performance. For that you need:

- A GTX 16xx, RTX 20xx or newer GPU (compute capability 7.5+). Older cards, like the GTX 10xx series, still work, but through the slower WebGPU acceleration.
- A recent NVIDIA driver with support to CUDA 13 (version 580 or newer).
- At least 6 GB of VRAM; 8 GB or more is recommended.

### Osaka upscaler

**Osaka** is a much heavier model than the others and has its own requirements: at least 32 GB of RAM and a GPU with 16 GB of VRAM on Windows and Linux, or 16 GB of unified memory on macOS. It also takes about 7 GB of extra disk space.

## ✨ Enhancements

All enhancements available here come from open-source AI models that were adapted and converted to work on this project. The models and the credits to the original works can be found in the Hugging Face repository [vegidio/open-photo-ai](https://huggingface.co/vegidio/open-photo-ai):

### Denoise

- **Stockholm**: use when you need fast, high-quality denoising of real sensor noise and computational efficiency matters. It's a good choice when throughput and resource constraints are real concerns, keeping inference times low without sacrificing quality.
- **Gothenburg**: use when your photos contain real-world sensor noise, the kind produced by shooting in low light or at high ISO with a smartphone or DSLR. It handles complex noise patterns that cameras produce, making it the right choice for photography.
- **Malmö**: use to remove rain streaks from outdoor images, whether captured in light drizzle or heavy downpour. It handles rain of varying scale, density, and direction, restoring fine details behind streaks. A good choice when weather artifacts obscure the scene.

*Verdict*: for regular sensor noise, start with **Gothenburg** for the best quality, then switch to **Stockholm** if speed matters more; reach for **Malmö** only when you need to remove rain streaks.

### Face Recovery

- **Athens**: use when identity fidelity matters most. This model lets you preserve facial structure while restoring details, even on heavily degraded faces. Best when you want restoration without changing the person.
- **Santorini**: use when you want aggressive, fast enhancement and can tolerate identity drift. It produces sharp, visually pleasing faces on moderate degradation, but may hallucinate features and alter identity on very low-quality inputs.

*Verdict*: if identity matters, start with **Athens**; if aesthetics matter more, use **Santorini**.

### Colorization

- **Delhi**: use as your first choice for most black-and-white photos. It looks at the whole scene to decide what things should look like, producing rich, confident colors with clean edges — skin, skies, and clothing rarely bleed into each other. Occasionally it commits boldly to a color that wasn't there.
- **Mumbai**: use when Delhi's colors feel too intense or modern for the photo. Built on the same engine but trained for a more creative touch, it leans toward softer, gently stylized tones that can suit vintage portraits and film scans — though its choices vary more from one photo to the next.
- **Jaipur**: use when consistency matters most. Its colors are more muted and restrained than the others, but it almost never produces the odd color patches or strange guesses colorization is prone to, making it a steady choice for old family portraits, landscapes, and badly degraded scans.

*Verdict*: start with **Delhi** for the most lifelike result; try **Mumbai** if you'd like a softer, more nostalgic feel; switch to **Jaipur** when the other two hallucinate colors or leave strange patches on a difficult photo.

### Light Adjustment

- **Paris**: use when the light in the photo is uneven — night scenes, backlit subjects, or a face lost in shadow while the rest looks fine. It brings back what the darkness hides, keeps colors natural, and is very fast.
- **Lyon**: use when the whole photo came out too bright or too dark — washed-out skies, or a subject that disappears into shadow. It rebuilds the brightness of the entire image, rescuing detail at both ends at once.

*Verdict*: reach for **Paris** when only parts of the photo are too dark and you want them opened up; switch to **Lyon** when the whole shot is over- or under-exposed and the brightness needs fixing from end to end.

### Color Balance

- **Rio**: use when the whole photo leans one way — orange under indoor lamps, blue in open shade or on a cloudy day. It re-renders the shot as if the camera had picked the right white balance, correcting the cast globally.
- **São Paulo**: use when two light sources fight inside the same frame, like a warm lamp indoors against daylight from the window. It renders the scene under several white balances and blends them pixel by pixel, area by area.

*Verdict*: start with **Rio**: most photos lean the same wrong way and it's the faster of the two; switch to **São Paulo** when fixing one part of the frame throws another off, which no single global correction can solve.

### Sharpen

- **Moscow**: use when blur comes from the camera being out of focus rather than from movement — e.g. portraits with a blurry background or foreground, macro photography gone soft, or any scene where a lens failed to focus on the right plane.
- **St. Petersburg**: use when you need fast, lightweight motion deblurring and efficiency matters more than squeezing out every last bit of quality. It's well-suited for action footage and handheld camera shake, and it's a solid choice when running on limited hardware.
- **Novgorod**: use when blur is caused by camera shake or fast-moving subjects — e.g. sports, handheld shots in low light, or any photo where something moved during exposure. It prioritizes maximum restoration quality over speed; good when results matter most.

*Verdict*: use **Moscow** for out-of-focus blur; for motion blur, start with **Novgorod** for the best quality, or fall back to **St. Petersburg** if speed matters more.

### Upscale

- **Tokyo**: use when you want a natural upscale without exaggeration. It focuses on preserving the original look and fine structures instead of "inventing" new details, making it ideal when realism and faithfulness matter more than sharpness.
- **Kyoto**: use for real-world photos (people, landscapes, products). It excels at restoring details while handling noise, blur, and compression artifacts. Ideal for practical applications where images are imperfect, and you want visually pleasing, robust results fast.
- **Saitama**: use for cartoon, drawings, line art, and digital illustrations. It preserves clean lines, flat colors, and stylized shading without introducing photo-like textures. Best when sharp edges and stylistic consistency matter more than realism.
- **Osaka**: use when you want the best possible quality upscale available. It rebuilds detail rather than just sharpening it, going furthest on photos that are soft or heavily compressed. It's slow, though, and needs a powerful GPU to run well.

*Verdict*: start with **Tokyo** if you have a powerful GPU, then try **Kyoto** if it's taking too long. Reach for **Osaka** only when the others leave the image looking soft, and you can afford the wait.

## 💣 Troubleshooting

### "App Is Damaged/Blocked..." (Windows & macOS only)

For a couple of years now, Microsoft and Apple have required developers to join their "Developer Program" to gain the pretentious status of an _identified developer_ 😛.

Translating to non-BS language, this means that if you’re not registered with them (i.e., paying the fee), you can’t freely distribute Windows or macOS software. Apps from unidentified developers will display a message saying the app is damaged or blocked and can’t be opened.

To bypass this, open the Terminal and run one of the commands below (depending on your operating system), replacing `<path-to-app>` with the correct path to where you’ve installed the app:

- Windows: `Unblock-File -Path <path-to-app>`
- macOS: `xattr -d com.apple.quarantine <path-to-app>`

### "Trojan:XXXXXX" detected (Windows only)

This is a false positive. It isn't a specific virus, but a generic label Windows Defender uses when something _behaves_ suspiciously. The installer downloads the app, unpacks it, creates a Start Menu shortcut and adds it to your PATH; normal installer steps that, combined in a small unsigned script, match a pattern Defender is cautious about.

I suggest to upload the app binary to [VirusTotal](https://www.virustotal.com) and you'll get a clean report. You can also read the source or build the binary yourself, but if you're still uneasy, don't disable your antivirus; just wait until the detection clears.

### The app is taking too long to download dependencies

This app has some important dependencies that can't be bundled with the app itself because they are rather big, like ONNX Runtime, CUDA and TensorRT (if supported by your system). They are hosted on Github and the app will download them the first time it opens.

Unfortunately, GitHub has a rate limit that will throttle the download speed if these files are downloaded too frequently. Since this is an open-source and free project I can't afford to pay for a hosted solution where we wouldn't have this problem.

If someday this project receives enough funds/donations, then I will pay for a better hosting solution. Meanwhile, bare with me on this one.

### The model is taking too long to load (TensorRT only)

When you open the app for the first time, if it detects that TensorRT is available on your system, it will prompt you to enable it or not. If you choose to enable it, all models will run with TensorRT acceleration.

This is one of the fastest ways to run the models; however, TensorRT needs to optimize the model graphs the first time it's used, which can take a few minutes. This is why it seems to be taking too long or even stuck when you run the models for the first time. But on subsequent runs, when the TensorRT optimization is already done, all enhancements will run much faster.

If you don't want to use TensorRT acceleration, you can disable it in the app Settings.

## 🐛 Error Reporting

If you encounter any issues while using the app, please report them by [creating a new issue](https://github.com/vegidio/open-photo-ai/issues) on our repository and give as much detail as possible, including steps to reproduce the issue, screenshots, and any error messages you receive.

Errors reported by e-mail or other channels will not be tracked, so please make sure to report them on Github.

## 🛠️ Build

### Dependencies

To build this project, you will need the following dependencies installed in your computer:

- [Rust](https://rust-lang.org/tools/install)
- [Just](https://just.systems/man/en/installation.html)

If you want to build the GUI you will also need:

- [Node.js](https://nodejs.org/en/download/)
- [PNPM](https://pnpm.io/installation)

### Compiling

With all the dependencies installed, in the project's root folder run the command:

```bash
just build <interface> <architecture>
```

Where:

- `<interface>`: can be `cli` or `gui`.
- `<architecture>`: can be `x64` or `arm64` (optional).

For example, if I wanted to build a GUI version of the app, on architecture x64, I would run the command:

```bash
just build gui x64
```

## ❤️ Sponsors

This project is possible due to the generous contributions from my sponsors. You can see the full list of sponsors [on my GitHub profile](https://github.com/vegidio/vegidio/blob/main/README.md#sponsors).

If you find **Open Photo AI** useful, please consider [becoming a sponsor](https://github.com/sponsors/vegidio) too.

## 📝 License

**Open Photo AI** is released under the AGPL-3.0 License. See [LICENSE](LICENSE) for details.

## 👨🏾‍💻 Author

Vinicius Egidio ([vinicius.io](http://vinicius.io))
