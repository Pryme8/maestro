# Releasing

For maintainers. A release is a tag, and the downloads are built by GitHub from that tag, so nobody has to build them by hand. No release has been made yet, and this process has not been run end to end.

## What gets built

The workflow in [.github/workflows/release.yml](../.github/workflows/release.yml) runs the tests and builds three downloads:

| File | Built on | Holds |
|---|---|---|
| `maestro-<version>-windows-x64.zip` | Windows | `maestro.exe`, the README and the licence |
| `maestro-<version>-macos-universal.zip` | macOS | `Maestro.app`, one program for Apple Silicon and Intel |
| `maestro-<version>-linux-x64.tar.gz` | Ubuntu 22.04 | `maestro`, the README and the licence |

It also writes `SHA256SUMS.txt`.

The Linux download is built on Ubuntu 22.04 so that it runs on that version and newer. It will not run on systems with an older C library.

Every action the workflows use is pinned to a full commit, with the version in a comment beside it. A test in `src/guard.rs` fails if one is not, so when updating an action, change the commit and the comment together.

## Try the build first

On the repository's Actions page, choose "Release" and "Run workflow". This builds and tests all three and keeps the files as artifacts on the run, without making a release. Do this before the first tag, because it is the first time the workflow, and the macOS code, will have run at all.

## Make a release

1. Set `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows, and move the "Unreleased" entries in [CHANGELOG.md](../CHANGELOG.md) under the new version.
2. Commit, then tag and push:
   ```
   git tag v0.1.0
   git push origin main v0.1.0
   ```
   The tag must be `v` followed by the version in `Cargo.toml`, or the build stops.
3. The workflow opens a **draft** release with the files attached. Nothing is public yet.
4. Download each file and run it on its system. Check the notes, and say plainly in them what is untested on each system: see [Status](status.md).
5. Publish the draft.

To take a release back, delete it and its tag on GitHub. People who already downloaded it keep their copy.

## macOS

The macOS code has never been compiled, so the macOS build is marked `experimental` in the workflow: if it fails, the Windows and Linux files are still built and the draft release simply has no macOS file. Once it builds, set `experimental: false` so a broken Mac build is noticed.

`scripts/bundle-macos.sh` makes the app. It can be run on a Mac by hand:

```
sh scripts/bundle-macos.sh
```

It builds for both kinds of Mac, joins them into one program, makes the icon from `assets/icon.png`, writes `Info.plist`, signs the app ad hoc and zips it into `dist/`. It has not been run yet.

## Signing

None of the downloads is signed, so each system warns before running them. What signing takes:

- **Windows:** a code-signing certificate. Sign `maestro.exe` with `signtool` before it is zipped. A new certificate still shows the SmartScreen warning until enough people have run the program, unless it is an extended-validation certificate.
- **macOS:** membership of the Apple Developer Program, which is paid yearly. Sign `Maestro.app` with a "Developer ID Application" identity and the hardened runtime, send it to Apple with `notarytool`, and staple the result to the app. The same membership is what blocking one program on the network needs.
- **Linux:** nothing is expected. The checksums are enough.

The certificate and the Apple credentials would be stored as repository secrets and used by the workflow. Until then, [Install](install.md#the-downloads-are-not-signed-yet) tells people how to get past each warning.

## Not set up

- An installer or a package for winget, Homebrew or apt.
- An icon inside `maestro.exe`. The window has the icon; the file in Explorer does not.
- A `--version` flag, so a person can tell which release they are running.
- Builds for ARM Windows or ARM Linux.
