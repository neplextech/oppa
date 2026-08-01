# Changelog

All notable changes to OPPA and OpenPrinter are documented here.

## Unreleased

### Breaking changes

- Replace the three-endpoint browser/token flow with one server base
  URL, `/.well-known/openprinter` discovery, one-time pairing codes,
  locally generated Ed25519 credentials, and gateway challenge
  authentication.
- Change protocol version values from numeric `1` to string `"1"` and
  remove `agent.authentication_metadata`.
- Change `@openprinter/server` to authenticate accepted transports
  against pluggable public credential stores before `agent.hello`.

## 0.2.0 — 2026-07-30

### Bug Fixes

- Search route
  ([937f71a](https://github.com/neplextech/oppa/commit/937f71aedd4ab883e762a62bd1d1ede329b3cc9d))

- Adjust landing page components
  ([4d47de9](https://github.com/neplextech/oppa/commit/4d47de9a127f4daaf948e3d08061b019e0e01900))

### Features

- Initial mvp
  ([53e426f](https://github.com/neplextech/oppa/commit/53e426fccbc70eeaee6a61a76fb7a1424b98f86e))

- MacOS-style UI redesign with light/dark mode, context menus, and
  command palette
  ([28179df](https://github.com/neplextech/oppa/commit/28179dfe545947240a9b118004b75b7487bd5747))

- Add runtime configuration and initial wrangler.toml setup
  ([0e39e20](https://github.com/neplextech/oppa/commit/0e39e20ed3be9498c8024fdacd22dd12ce5a9e75))

- Static www export
  ([30b317b](https://github.com/neplextech/oppa/commit/30b317b26c952cbb1009eb9fcf2babaf2b9718bb))

- Use simplified protocol and enhance desktop app experience
  ([a50f206](https://github.com/neplextech/oppa/commit/a50f2069a08536ae94d8bde238e1bcfb28241c0e))

- Developer mode, keybindings, routing, and virtual printer sound
  ([63a7fef](https://github.com/neplextech/oppa/commit/63a7fef6fee2f55b722b76aaa3fe66630a34260e))

- Enhance printer sound functionality and clean up code formatting
  ([e7a6041](https://github.com/neplextech/oppa/commit/e7a6041545065614c5a672f8834246f8ac3676b8))

### Other

- Create LICENSE
  ([9840215](https://github.com/neplextech/oppa/commit/9840215ffc82e8916e87181ea9db2da29e1a33b6))

- Create CODE_OF_CONDUCT.md
  ([50d3f58](https://github.com/neplextech/oppa/commit/50d3f580c1daf1d7cece10501233df29897c3f7d))

- Create CONTRIBUTING.md
  ([ff22759](https://github.com/neplextech/oppa/commit/ff22759bc40ae536323d406d8c877c32b83bad00))

- Initial commit
  ([1733bb2](https://github.com/neplextech/oppa/commit/1733bb27293618f014d7403728dd6cd68d4e8ffb))

- Deps
  ([a367060](https://github.com/neplextech/oppa/commit/a367060a50f4e26c9dd201205b219d9f486c0ff6))

- Deps
  ([e9414fa](https://github.com/neplextech/oppa/commit/e9414faa779da89fa936d0f5d2dc80eaf93f7540))

- Add skills
  ([74138cf](https://github.com/neplextech/oppa/commit/74138cfbce94fcaca02ed650483b5f15efecda72))

- Prettier
  ([73ee8a7](https://github.com/neplextech/oppa/commit/73ee8a71591242d0700e188cbecea43d8fd42e3a))

- Remove wrangler.toml configuration file
  ([feea222](https://github.com/neplextech/oppa/commit/feea222168dab8789aa5849cbb43c3c618eebc18))

- Deps
  ([e7b173f](https://github.com/neplextech/oppa/commit/e7b173fb8cdd2be712561ce14824e71fc2568233))

- Deps
  ([5e5b864](https://github.com/neplextech/oppa/commit/5e5b864557db478b983a44e8e73ab07532588de8))

- Deps
  ([96c4db9](https://github.com/neplextech/oppa/commit/96c4db9a37f24c04bdd8447ee84cafbc4967dbb0))

- Lint fixes
  ([88f5138](https://github.com/neplextech/oppa/commit/88f51389db48821d14f1846115802e5d09304732))

### Refactoring

- Redesign landing page
  ([0676ec0](https://github.com/neplextech/oppa/commit/0676ec0e8f38bdf6b2599debd15645bab93a76fb))
