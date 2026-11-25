# Changelog

All notable changes to OPPA and OpenPrinter are documented here.

## Unreleased

Prepared stable versions: OPPA **0.3.0**, `@openprinter/protocol`
**0.4.0**, `@openprinter/server` **0.4.0**, and `openprinter`
**0.1.1**. These versions have not been released yet.

### Features

- PDF document printing with signed retrieval, bounded downloads,
  printer document capabilities, and configurable rendering and
  submission settings. Submission means backend acceptance, not
  confirmed physical printing.
  ([4156bdf](https://github.com/neplextech/oppa/commit/4156bdf39689ab377b13f012fb449e451fff24d1))
- Protocol v2 negotiation for PDF jobs and document capabilities, with
  a base-protocol v1 fallback. Handshake envelopes remain v1; peers
  use the lowest envelope version required by the content and reject
  PDF delivery to v1 peers.
  ([3786589](https://github.com/neplextech/oppa/commit/378658928e1eacd7e4ec741c9da2ec0dd9708bed))
- Desktop UI/UX redesign with guided onboarding, consistent light and
  dark themes, a collapsible sidebar, operational overview, searchable
  printer and job lists, resizable inspectors, virtual document
  previews, categorized preferences, sanitized diagnostics, and native
  tray status.
  ([46b4bd2](https://github.com/neplextech/oppa/commit/46b4bd26eb3b9cd1c22f656572d85d18e79ef270))

### Packages

- Publish the PDF schemas and negotiation helpers in
  `@openprinter/protocol` 0.4.0 and the corresponding session support
  in `@openprinter/server` 0.4.0.
- Update `openprinter` to 0.1.1 so its published protocol dependency
  resolves to `@openprinter/protocol` 0.4.0.

### Bug Fixes

- Gate Windows printing dependencies by target and satisfy Rust 1.99
  Clippy.

## oppa-v0.2.8 — 2026-09-25

### Bug Fixes

- Silence needless_pass_by_value on quit_application command
  ([52f5389](https://github.com/neplextech/oppa/commit/52f5389a5fe9ed897598cc371d6d604e831bef9b))

- Hide Windows printer probe console
  ([613a9eb](https://github.com/neplextech/oppa/commit/613a9eb859352c903674fef4e6013f822bd06b04))

### Features

- Add OpenPrinter Cloud SDK and documentation
  ([27089d5](https://github.com/neplextech/oppa/commit/27089d549e8b21483e7a9bd05a5f3bb49119607c))

- Enhance documentation contents
  ([8bb7fd3](https://github.com/neplextech/oppa/commit/8bb7fd31f5ced2f7eaeee9c87da28ee0b09dff72))

- Document public OpenPrinter client skill
  ([166bf3d](https://github.com/neplextech/oppa/commit/166bf3d5173d2bcf542c13f64623faa6d608e0eb))

- Add driver and ESC/POS output pipelines
  ([28155c5](https://github.com/neplextech/oppa/commit/28155c5a17a8a4e8186b0d7e5e29216d6815cae9))

### Other

- Bump oppa app
  ([f27601a](https://github.com/neplextech/oppa/commit/f27601a3ad8d7c37dc263c21a13ed3515dcccaf6))

## oppa-v0.2.7 — 2026-08-28

### Bug Fixes

- Keep agent running when hiding app
  ([4652da9](https://github.com/neplextech/oppa/commit/4652da930bf5a482f8799d58d3a825be953b6dae))

- Box reporting error message
  ([18a3b36](https://github.com/neplextech/oppa/commit/18a3b3697e3f408cfe986ebcd16a7448e6255e4d))

- Dev tauri launch command on windows
  ([7ebdf93](https://github.com/neplextech/oppa/commit/7ebdf93ab9ef46899a441cdd406b2bc5f1c06308))

- Use native Windows frame controls
  ([89924a9](https://github.com/neplextech/oppa/commit/89924a92089af33cb6dc991b0501a71a4bdca09e))

- Implement Windows system-queue printing and printer reachability
  ([a0c4933](https://github.com/neplextech/oppa/commit/a0c49330c98816410275206a210329f658d0029a))

- Route dev test prints by printer id, not fingerprint
  ([2a42313](https://github.com/neplextech/oppa/commit/2a4231376d700e83a77e306f06f161212f637115))

### Other

- Prettier
  ([e1f9e10](https://github.com/neplextech/oppa/commit/e1f9e102c00c25adddfc2a2dfc0e27bbcb66b231))

- Release oppa 0.2.7
  ([db24bce](https://github.com/neplextech/oppa/commit/db24bce9158e8a0645c633a62f327343ac7bc578))

### Refactoring

- Extract virtual output mapping
  ([e64991a](https://github.com/neplextech/oppa/commit/e64991a4df2c622d05289ac96c5962debdb86983))

## oppa-v0.2.6 — 2026-08-20

### Bug Fixes

- Resolve pnpm lint:js and cargo clippy warnings
  ([5313fe8](https://github.com/neplextech/oppa/commit/5313fe8fbaab9cc47a001e0de92449afb8d2aac8))

### Features

- Render structured virtual outputs as receipts
  ([09724c5](https://github.com/neplextech/oppa/commit/09724c521cc05f3c0d10f5560fc899f715955bea))

### Other

- Release 0.2.6
  ([9fde389](https://github.com/neplextech/oppa/commit/9fde389714183961a7f2ec9b671a38f41acac647))

## oppa-v0.2.5 — 2026-08-04

### Bug Fixes

- Use oppa-dev:// scheme in node example dev UI
  ([c47ba33](https://github.com/neplextech/oppa/commit/c47ba338b6a330952af741dc4ec5a20b4ad99453))

- Fix deep link scheme matching and add dev testing hook
  ([ea01790](https://github.com/neplextech/oppa/commit/ea01790a33e7157c1b619cc74169fcaf1fe531ca))

### Features

- Add debug devtools support for main window
  ([4757209](https://github.com/neplextech/oppa/commit/475720965c361824c64a7ec65fdc812a57628288))

- Simplify deep link dialog and add node server dev UI
  ([8d563ad](https://github.com/neplextech/oppa/commit/8d563adc53d61fa2a59c84d36200d7174c32cbe5))

- Isolated dev build with oppa-dev product and scheme
  ([1886e31](https://github.com/neplextech/oppa/commit/1886e31b42d93fcfccbf8a6d47142a323feec47a))

### Other

- Release oppa 0.2.5
  ([f15c4b4](https://github.com/neplextech/oppa/commit/f15c4b4dfe1a0bbddac166af099efaa2c93273bf))

## oppa-v0.2.4 — 2026-08-03

### Bug Fixes

- Ui consistency and icons
  ([b20570d](https://github.com/neplextech/oppa/commit/b20570de2d3e4d4a8729a7c2d170a3d3dad0ce1a))

- Deep link pair miss issue
  ([f33dfb8](https://github.com/neplextech/oppa/commit/f33dfb842df62c95f434f8999346c4fe2265f073))

### Documentation

- Support mermaid diagram
  ([28f4d32](https://github.com/neplextech/oppa/commit/28f4d3254682e05662defdd25ec5dd2a60131888))

### Features

- Use consistent site brand
  ([64229d7](https://github.com/neplextech/oppa/commit/64229d7c5bb28338a43d821379f83bd1d56e8c43))

- Add ability to remove recent servers
  ([86f9a5d](https://github.com/neplextech/oppa/commit/86f9a5d4876510f4da63b5e9b22d73de2bcc1725))

### Other

- Oppa icons
  ([4d2bd51](https://github.com/neplextech/oppa/commit/4d2bd5127c8856433d10d4d9c31644f35d9c8756))

- Lint fix
  ([e7fa7c5](https://github.com/neplextech/oppa/commit/e7fa7c566db1cde2473b9638f562b29fa01adefb))

- Release oppa 0.2.4
  ([0dbbfb9](https://github.com/neplextech/oppa/commit/0dbbfb96fcd9e6f4534833a9784b834bdbf1ffdd))

- Release oppa 0.2.4
  ([440e3ca](https://github.com/neplextech/oppa/commit/440e3ca724c0fabd8d32b41e1d8d7a12b50d45d2))

## oppa-v0.2.3 — 2026-08-02

### Bug Fixes

- App version
  ([cce155d](https://github.com/neplextech/oppa/commit/cce155d63ee0e8ca89c3125d9decfa83c7a00db3))

- Place developer mode config above product info
  ([922f411](https://github.com/neplextech/oppa/commit/922f411deac30c3fdd83b130f0d5dc5b99594a2f))

- Use password field for server url
  ([96f05a4](https://github.com/neplextech/oppa/commit/96f05a4971b97c7f2456ddafd74819e4f323e8fa))

- Deep link pairing silently fails when already paired
  ([ce3e77d](https://github.com/neplextech/oppa/commit/ce3e77dc2df29daf355a12ffd610b4a3990eec6e))

### Other

- Lint fix
  ([919d8b8](https://github.com/neplextech/oppa/commit/919d8b8973213dae738e5752dd4ceaac275c105e))

- Release oppa 0.2.3
  ([06d474e](https://github.com/neplextech/oppa/commit/06d474ea87a4a1d6014856ec46daeaf752cb1f20))

## oppa-v0.2.2 — 2026-08-02

### Bug Fixes

- Use route handler for sitemap.xml to support static export
  ([c4d89ad](https://github.com/neplextech/oppa/commit/c4d89ad14a0643667ef96f983404496aa7485a43))

- Prevent long server URL from overflowing setup screen layout
  ([2a6a8dd](https://github.com/neplextech/oppa/commit/2a6a8dd90efdfb1c2aa003603e06a57875c15199))

- Add window drag and standard window control permissions
  ([03d5b92](https://github.com/neplextech/oppa/commit/03d5b92b42bf360364336fcb4c93ae36199effae))

### Documentation

- Document oppa:// deep link pairing
  ([613af4b](https://github.com/neplextech/oppa/commit/613af4be0926110b7f94eca5b076e84f81b7415d))

### Features

- UI revamp, settings from setup, clear records, dev isolation
  ([f8b321e](https://github.com/neplextech/oppa/commit/f8b321ec5e2959751a8a31e74fbfb10b4c3dfd19))

- Add llms.txt and llms-full.txt via Fumadocs LLM integration
  ([56083cd](https://github.com/neplextech/oppa/commit/56083cdc9f2a8db25fc19f39ce13063bd6b68bd9))

- Add sitemap.xml and robots.txt
  ([1796700](https://github.com/neplextech/oppa/commit/17967000e40db331123b7f81bc1b2e36dd4b1a61))

- Custom overlay titlebar, DEV badge, and settings improvements
  ([b885128](https://github.com/neplextech/oppa/commit/b8851283c658af1b0265bbe57ef44a1f0f910cb4))

- Deep link pairing and recent server persistence
  ([35610cf](https://github.com/neplextech/oppa/commit/35610cf232ff0fa62c76882f3506b6e3977ddbb2))

- Show recent servers list in setup screen
  ([7ed6437](https://github.com/neplextech/oppa/commit/7ed6437936bd122846fdc1338736f3cb51cb1f2f))

- Smart platform download button and hero CTA fix
  ([119d8d9](https://github.com/neplextech/oppa/commit/119d8d9f4e97aec2dd624ccbe650893f51f9e7f2))

### Other

- Release oppa 0.2.2
  ([023bcaf](https://github.com/neplextech/oppa/commit/023bcafc4a634e28a36c3e00dbf84f22c3a48056))

- Release oppa 0.2.2
  ([cee51e0](https://github.com/neplextech/oppa/commit/cee51e02ad1f49781b7d7473301d8f2c7af1b8fe))

## oppa-v0.2.1 — 2026-08-02

### Bug Fixes

- Preserve server URL path when building discovery URL
  ([5c33c0c](https://github.com/neplextech/oppa/commit/5c33c0cb1123be7a7a2b838665aa37d68a9bc0eb))

### Other

- Prettier
  ([db488da](https://github.com/neplextech/oppa/commit/db488daf9b2096ca3d70a0d222320c26d113ab27))

- Release oppa 0.2.1
  ([616e619](https://github.com/neplextech/oppa/commit/616e619f996b18b20cb8c406c1700fe42bdbe959))

## oppa-v0.2.0 — 2026-08-01

### Bug Fixes

- Build protocol package before running ESLint on packages
  ([a2a853b](https://github.com/neplextech/oppa/commit/a2a853b0704f6b8a59315d54fec4f8ff64c68497))

- Build server package before linting to resolve examples types
  ([bfbbbd8](https://github.com/neplextech/oppa/commit/bfbbbd89c6b9529354c7d534cbff97ae6c4f3401))

### Features

- Add skills.sh skill definitions for @openprinter/protocol and
  @openprinter/server
  ([d44102f](https://github.com/neplextech/oppa/commit/d44102f84783691aae33883ac10c99b41947a6cb))

- Add og images
  ([f2a4a6f](https://github.com/neplextech/oppa/commit/f2a4a6f8b467e94fca57dd68f88d01e1c5d834e5))

### Other

- Release
  ([fbbaeb3](https://github.com/neplextech/oppa/commit/fbbaeb3fda4e32b9098d970850d33a4da71693f4))

### Refactoring

- Oppa authentication system
  ([2a8d40c](https://github.com/neplextech/oppa/commit/2a8d40cb251578e21037105a14ae2c4de642fe70))

## oppa-v0.1.0 — 2026-07-30

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

- Release
  ([5916944](https://github.com/neplextech/oppa/commit/59169447a0d9dfc17195857654a252964eb1da3c))

### Refactoring

- Redesign landing page
  ([0676ec0](https://github.com/neplextech/oppa/commit/0676ec0e8f38bdf6b2599debd15645bab93a76fb))
