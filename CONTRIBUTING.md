# Contributing

Thanks for helping. How to build and test: the [develop page](https://v1nc3t.github.io/meowNY/develop.html).

1. Open an issue before a large change.
2. Run the checks before you push; CI runs the same ones. A bug fix comes with a test that fails without it.
3. Keep dependencies few, and say why when you add one.
4. Keep it local-first: no new network call unless it is off until the user turns it on, and listed in
   `scripts/outbound-allowlist.txt`.
5. A new desktop feature ships its command line commands in the same change.
6. Update the [guide](docs/site/guide.mjs) when what users see, or what the app stores or sends, changes.

Security problems go privately, not in an issue: see [SECURITY.md](SECURITY.md).

By contributing you agree that your work is published under the [AGPL-3.0](LICENSE).
