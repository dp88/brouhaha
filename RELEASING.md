# Releasing

`brouhaha` lists its optional `spacewalk` dependency as a git dependency
until spacewalk is on crates.io. Cargo resolves optional dependencies even
when the feature is off, so the swap below must wait for that release.

1. Publish `spacewalk` to crates.io first.
2. Change the dependency to a registry dependency:
   `spacewalk = { version = "0.1", optional = true }`. Remove the git key,
   its comment, and the `allow-git` entry in `deny.toml`.
3. Move the `Unreleased` changelog entries under a new version heading with
   today's date, and confirm the version in `Cargo.toml`.
4. Run the full local validation suite with `--all-features`.
5. Check the archive and registry upload without publishing:
   `cargo package --list` and `cargo publish --dry-run`.
6. Publish with `cargo publish`.
7. Tag the release commit with an annotated tag and push it:
   `git tag -a vX.Y.Z -m "brouhaha X.Y.Z"`, then `git push origin vX.Y.Z`.
8. Create the GitHub release from the tag.
