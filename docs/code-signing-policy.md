# Code signing policy

Free code signing provided by SignPath.io, certificate by SignPath Foundation.

EdgePad's releases are built from this repository by GitHub Actions (`.github/workflows/release.yml`) when a version tag is pushed. Only what that workflow builds from the tagged source is submitted for signing; nothing built elsewhere is signed.

## Roles

- Authors (committers): [Marby3](https://github.com/marby3)
- Reviewers: [Marby3](https://github.com/marby3)
- Approvers: [Marby3](https://github.com/marby3)

Every change reaches the main branch through a pull request. Each signing request of a release is approved by an approver.

## Privacy

EdgePad does not send any information over the network. It reads its settings file and writes it when you save from the settings window, and with `--debug` it writes a log beside the exe; nothing leaves your computer. It makes no network connection at all.
