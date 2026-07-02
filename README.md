# Simple uBlock Installer — by [@FPSHEAVEN](https://x.com/FPSHEAVEN)

A tiny Windows app that installs **uBlock** into **Chrome** and **Firefox** with (almost) one click — no admin rights required.

<p align="center">
  <img src="assets/icon.png" alt="FPSHEAVEN" width="96">
</p>

## Download

Grab the latest `SimpleUblockInstaller.exe` from the [**Releases**](https://github.com/fpsheaven/SimpleUblockInstaller/releases) page, run it, and click **Install**.

## What it does

It detects which browsers you have and only touches those:

- **Google Chrome → zero clicks.** Silently force‑installs **uBlock Origin Lite** (Manifest V3) via a Chrome enterprise policy. It installs in every Chrome profile, stays enabled, and can't be turned off by accident. No administrator rights needed.
- **Mozilla Firefox → one confirmation.** Opens the official **uBlock Origin** add‑on page; you click **Add to Firefox → Add** for the **full uBlock Origin** (Firefox still supports Manifest V2).

There's also an **Uninstall** button that reverses everything the installer did.

> **Why "Lite" on Chrome?** Current Chrome (150+) refuses to install any Manifest V2 extension — full uBlock Origin included — by *any* method. uBlock Origin **Lite** (MV3, by the same author) is the only version Chrome will install, so that's what's used there. Firefox has no such restriction, so it gets the full uBlock Origin.

## How it works (technical)

- **Chrome:** writes a single value under `HKCU\Software\Policies\Google\Chrome\ExtensionInstallForcelist` pointing at uBlock Origin Lite's Web Store ID (`ddkjiahejlhfcafbddmgiahcphecmpfh`). Chrome downloads and installs it from the Web Store on its next launch. Written to **HKCU**, so no admin prompt.
- **Firefox:** extensions can't be installed silently without admin, so the app just opens the AMO install page for you.
- **Uninstall:** removes the Chrome policy entry (and the empty policy key if it was the only one — other policy‑installed extensions are left untouched); Chrome then removes the extension on restart. For Firefox it opens `about:addons` so you can click **Remove**.

The installer never bundles the extensions — Chrome fetches from the Web Store and Firefox from AMO, so you always get the genuine, signed, up‑to‑date uBlock.

## Build from source

Requires [Rust](https://rustup.rs/) (edition 2024) on Windows.

```sh
cargo build --release
```

The binary lands at `target/release/fpsheaven_ublock_installer.exe`.

## Credits

- **uBlock Origin** and **uBlock Origin Lite** are created by Raymond Hill ([gorhill](https://github.com/gorhill/uBlock)) — this installer just automates installing them. It is **not affiliated with or endorsed by** the uBlock Origin project.
- Built by [FPSHEAVEN](https://x.com/FPSHEAVEN).

## License

[MIT](LICENSE)
