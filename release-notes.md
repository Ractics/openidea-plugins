:robot: I have created a release *beep* *boop*
---


<details><summary>blade: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/blade-v0.3.0...blade-v0.4.0) (2026-11-18)


### Features

* **blade:** quick fix to insert the missing closing directive ([35091f5](https://github.com/arnaud-ritti/openidea-plugins/commit/35091f513c02e9ab0034c200d7cf59acfdfe7828))


### Bug Fixes

* **blade:** cache injection declarations; keep injecting after a stray &lt; ([ac962a1](https://github.com/arnaud-ritti/openidea-plugins/commit/ac962a1daf5d3c25f45622e8033b1de7152a20c0))
* **blade:** compile the bodies test helper only for tests; pin the unquoted &lt; injection case ([6caca4f](https://github.com/arnaud-ritti/openidea-plugins/commit/6caca4f0667159a8503aa8c6038ed61dac3e0fc2))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
</details>

<details><summary>ddev: 0.3.0</summary>

## [0.3.0](https://github.com/arnaud-ritti/openidea-plugins/compare/ddev-v0.2.0...ddev-v0.3.0) (2026-11-18)


### Features

* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **ddev:** hide the status item outside DDEV projects, confirm sharing ([5722881](https://github.com/arnaud-ritti/openidea-plugins/commit/57228819fa33ed8152e02d41c7b10d569a82ee9a))
* **ddev:** name the project before delete, explain why the runtime is unavailable ([ae8045c](https://github.com/arnaud-ritti/openidea-plugins/commit/ae8045cbed170733af3a9ced09df7dc5cfba454b))
* **ddev:** render services from cache; per-project snapshots ([c317dc4](https://github.com/arnaud-ritti/openidea-plugins/commit/c317dc4f2f4140ef741d82700aa7c5be530f708a))
* **ddev:** surface background refresh failures; per-root failures; one Prefs per render ([c2f3905](https://github.com/arnaud-ritti/openidea-plugins/commit/c2f390551ff718e59e8636b853ba2d48d877d2df))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>dependency-audit: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/dependency-audit-v0.3.0...dependency-audit-v0.4.0) (2026-11-18)


### Features

* **dependency-audit:** make the npm lockfile graph size limit a setting ([544f155](https://github.com/arnaud-ritti/openidea-plugins/commit/544f155d026399776b6d9be39f187562bbec2ae7))
* **dependency-audit:** read Maven/Gradle versions from the IDE's resolved dependency graph ([8cfe897](https://github.com/arnaud-ritti/openidea-plugins/commit/8cfe8977112c8f45f32bcd2e4ae490eaa9594c38))
* **dependency-audit:** vulnerability notes and inline warnings, diff before upgrade, no automatic build runs by default ([cc1f4c1](https://github.com/arnaud-ritti/openidea-plugins/commit/cc1f4c1a13e40d330ba2c5c264dd1b21694eb587))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **dependency-audit:** filter private npm v1 lock entries and list poetry source packages ([a17e917](https://github.com/arnaud-ritti/openidea-plugins/commit/a17e917c89d068a249c6a3b4da76c508ed209ff2))
* **dependency-audit:** keep pnpm private-tarball packages local; narrow privacy note to filtered lockfiles ([e37c8b2](https://github.com/arnaud-ritti/openidea-plugins/commit/e37c8b276b06e91df0e3e2a48818540be8aafb44))
* **dependency-audit:** map tool denials, report auto-resolve failures, plain names ([b5fa17b](https://github.com/arnaud-ritti/openidea-plugins/commit/b5fa17ba2d832f3c0c4b9eabc0264a23e904ea32))
* **dependency-audit:** narrow pnpm private-package change-note ([c6e7701](https://github.com/arnaud-ritti/openidea-plugins/commit/c6e7701799992d5e1dcb9b983f33281144b634d1))
* **dependency-audit:** no build-resolution warning in projects without Maven or Gradle ([64b73b7](https://github.com/arnaud-ritti/openidea-plugins/commit/64b73b756632e151af208d660cd148efb36eb8f7))
* **dependency-audit:** parse uv/poetry lockfiles correctly and skip private-registry packages ([4311da1](https://github.com/arnaud-ritti/openidea-plugins/commit/4311da136c367b30287540f7de82e9f008f50d92))
* **dependency-audit:** safer defaults, shared-property fixes, Gradle and PEP 440 parsing ([60220a4](https://github.com/arnaud-ritti/openidea-plugins/commit/60220a44dc7d87996ef4fc649574da2dd6f4638b))
* **dependency-audit:** say when no dependency files exist, readable scan times, palette fallback for row commands ([8ef15ea](https://github.com/arnaud-ritti/openidea-plugins/commit/8ef15ea3f71322eaf4c4049031afe12bd9227ace))
* **dependency-audit:** shorten change-notes, let explicit resolve undo a stored no ([24bb87d](https://github.com/arnaud-ritti/openidea-plugins/commit/24bb87d636800e6f44b553643fffad75a5b37a6f))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* **spring-text:** share one XML walker, match drive letters case-insensitively ([61d354d](https://github.com/arnaud-ritti/openidea-plugins/commit/61d354d7122df0117b830a25290e76ceaa59d04f))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>devcontainer: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/devcontainer-v0.3.0...devcontainer-v0.4.0) (2026-11-18)


### Features

* **devcontainer:** gutter links to Compose files and the Dockerfile ([aae0083](https://github.com/arnaud-ritti/openidea-plugins/commit/aae008371374a847b4c93f72a7dc03680dc9347a))
* **devcontainer:** reusable container shells, Open Shell from the start notification ([862b0bb](https://github.com/arnaud-ritti/openidea-plugins/commit/862b0bb279dc4a2a7fb1f352675c7ba6539acac1))


### Bug Fixes

* brand logos for the related-files gutter icons ([10c28d6](https://github.com/arnaud-ritti/openidea-plugins/commit/10c28d68d95fbff873034c530bbc0a29401d9590))
* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **devcontainer,kube-explorer:** real command names in messages, one category ([64e5776](https://github.com/arnaud-ritti/openidea-plugins/commit/64e577666b4cc78bba4b30048d4cad98d9501e68))
* **devcontainer:** ask for a folder when Start Cloned has none; say the review expires, not registrations ([c6b6542](https://github.com/arnaud-ritti/openidea-plugins/commit/c6b6542eabc0acd8810ce4dd6b7646176bb94fcc))
* **devcontainer:** confirm Stop, explain denials and plain copy ([bb2559c](https://github.com/arnaud-ritti/openidea-plugins/commit/bb2559c2785c805bda3151cb683694121d226033))
* **devcontainer:** restore post-clone start and handle denied storage ([47dd55e](https://github.com/arnaud-ritti/openidea-plugins/commit/47dd55e8fca8750c3996050704ced19c2c0a905b))
* **devcontainer:** start and rebuild buttons in the view, clearer stop message ([b3e2fae](https://github.com/arnaud-ritti/openidea-plugins/commit/b3e2fae3c54ddd9bee17a8663599ad5578a1477a))
* **devcontainer:** validate the registered container and simplify copy ([8559dfc](https://github.com/arnaud-ritti/openidea-plugins/commit/8559dfc146c30829b4029830285b71e8ca342d4e))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
</details>

<details><summary>dotenv: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/dotenv-v0.3.0...dotenv-v0.4.0) (2026-11-18)


### Features

* **dotenv,spring-osgi:** settings to turn checks off ([63a8525](https://github.com/arnaud-ritti/openidea-plugins/commit/63a852568124c8f5e29de7edd0f5cdae4e788a73))
* **dotenv:** gutter links from env keys to their usages and other env files ([c750d47](https://github.com/arnaud-ritti/openidea-plugins/commit/c750d477b409f940728993d5647ec0c46abca430))


### Bug Fixes

* brand logos for the related-files gutter icons ([10c28d6](https://github.com/arnaud-ritti/openidea-plugins/commit/10c28d68d95fbff873034c530bbc0a29401d9590))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **dotenv:** a truncated inventory stays incomplete after updates ([cf9adf1](https://github.com/arnaud-ritti/openidea-plugins/commit/cf9adf14e09ebb576ba3ab5f6333fb2453d7f928))
* **dotenv:** keep walk bookkeeping consistent with events; queue over-budget event paths ([b1c4c7a](https://github.com/arnaud-ritti/openidea-plugins/commit/b1c4c7a96bf9369f5cd22a7c819648ab78bab2e3))
* **dotenv:** restore completeness when blocking files are fixed, drop facts of a removed folder ([dc0c7d2](https://github.com/arnaud-ritti/openidea-plugins/commit/dc0c7d2e2e9b298940f22ad8a3c65404ab313aa4))
* **dotenv:** resume a deadline-cut walk from its cursor instead of re-walking ([5b2004a](https://github.com/arnaud-ritti/openidea-plugins/commit/5b2004a522ad25b927e14ce3bc5a9a4e6f44bfb2))
* **dotenv:** resume a deadline-cut walk, warn when file watching is denied, never mask empty values ([323c7c9](https://github.com/arnaud-ritti/openidea-plugins/commit/323c7c91285b5eca6d7d65356b57c027f5dc31b8))
* **dotenv:** set hard_incomplete when discovery is truncated ([f948e0b](https://github.com/arnaud-ritti/openidea-plugins/commit/f948e0bd9bc32ab76266bf98ebf3bb0ca13f47e9))
* **dotenv:** show unmasked values in a picker, not a notification ([87bb0c2](https://github.com/arnaud-ritti/openidea-plugins/commit/87bb0c241f9e14f9f533fb43c5e7016fbde7d423))
* **dotenv:** stop re-walking on every save; track external file changes ([c596131](https://github.com/arnaud-ritti/openidea-plugins/commit/c596131a08cabbb18b9e1cc96f106d2721ce6a38))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* prefix the .env and Spring WS command titles ([8b1aeb2](https://github.com/arnaud-ritti/openidea-plugins/commit/8b1aeb2a9dfa66d1352655909397e5b87c2d5c68))
</details>

<details><summary>flyway: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/flyway-v0.3.0...flyway-v0.4.0) (2026-11-18)


### Features

* **flyway:** gutter links from table references to migrations and entities ([7e69a8c](https://github.com/arnaud-ritti/openidea-plugins/commit/7e69a8c336e126b74611fb4fb1917d195c322b53))
* **flyway:** migration state notes in the project tree, redacted target in the status bar ([bebe677](https://github.com/arnaud-ritti/openidea-plugins/commit/bebe67793ed8f5e457f0a8fa9b1e1ad276b97f37))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* brand logos for the related-files gutter icons ([10c28d6](https://github.com/arnaud-ritti/openidea-plugins/commit/10c28d68d95fbff873034c530bbc0a29401d9590))
* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* **flyway:** block comparison start on Community edition, hide licensekey, check class-root fingerprint before java ([da8efff](https://github.com/arnaud-ritti/openidea-plugins/commit/da8efff474dbe6698badd242c46008b3a5159537))
* **flyway:** lead confirmations with the impact and a redacted target, plain command wording ([b55b703](https://github.com/arnaud-ritti/openidea-plugins/commit/b55b7032cb240041da178f1068850db365036606))
* **flyway:** long Hibernate runs in a task, local migrations view, precise redaction ([db5a2ba](https://github.com/arnaud-ritti/openidea-plugins/commit/db5a2ba886edc724c075803e521840b345b0b670))
* **flyway:** name the clean target, table columns and plain toolbar copy ([0bbb746](https://github.com/arnaud-ritti/openidea-plugins/commit/0bbb74656f485ed8a9da932bc8a2ba799c1d3be3))
* **flyway:** open the newest migration after Prepare ([3839567](https://github.com/arnaud-ritti/openidea-plugins/commit/383956757c8295f61b3e4a39df8bda052f93b012))
* **flyway:** redact glued secrets, check the CLI after confirming, cache empty projects ([5b09511](https://github.com/arnaud-ritti/openidea-plugins/commit/5b0951168b506a0ca0e742d005e01afe9f94f5bf))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* remaining jargon in titles ([4edf7e8](https://github.com/arnaud-ritti/openidea-plugins/commit/4edf7e8ebe03a85b56445d5aebfcc60818d8bfa2))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-text:** share one XML walker, match drive letters case-insensitively ([61d354d](https://github.com/arnaud-ritti/openidea-plugins/commit/61d354d7122df0117b830a25290e76ceaa59d04f))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>gitlab: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/gitlab-v0.3.0...gitlab-v0.4.0) (2026-11-18)


### Features

* commands and menu entries for toolbar-only actions ([940dcf2](https://github.com/arnaud-ritti/openidea-plugins/commit/940dcf2ae8ebff44ec7b104af38b7559eb86bff9))
* **gitlab:** repository selector for multi-repository workspaces ([#17](https://github.com/arnaud-ritti/openidea-plugins/issues/17)) ([297a3c3](https://github.com/arnaud-ritti/openidea-plugins/commit/297a3c38b214283a99726a7370e997759c21034d))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* **gitlab:** clearer confirmations, real added/deleted diffs, selection-aware commands ([4a7d10e](https://github.com/arnaud-ritti/openidea-plugins/commit/4a7d10e14ad1fc5f4e540e19a4650c61feafff21))
* **gitlab:** confirm before sending the token to a new server; fix non-ASCII completion crash ([6322e7a](https://github.com/arnaud-ritti/openidea-plugins/commit/6322e7abb54ba7b3f97e7a24c284f915350cdbeb))
* **gitlab:** move secondary merge request actions into a More menu ([514c085](https://github.com/arnaud-ritti/openidea-plugins/commit/514c0859cee472a3141d5ecd045d35e0c2eedbf5))
* **gitlab:** protect local branches on fork checkout; encode link revisions ([00cbc6f](https://github.com/arnaud-ritti/openidea-plugins/commit/00cbc6f931c64fa1e8a533d58f52ca8d4cf737fd))
* **gitlab:** remove duplicate attribute buttons, offer sharing when there is no remote ([e374ed5](https://github.com/arnaud-ritti/openidea-plugins/commit/e374ed57bf545c13fd06b0d7b0d76eb7105e48da))
* **gitlab:** treat an unexpected rev-parse status as an error before the overwrite confirm ([45afb20](https://github.com/arnaud-ritti/openidea-plugins/commit/45afb20e257f0997f6a69b9992a1789162d2475e))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>ideolog: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/ideolog-v0.3.0...ideolog-v0.4.0) (2026-11-18)


### Features

* **ideolog:** info, debug and trace counts in the Log Levels panel, level filter in the snapshot ([dab4613](https://github.com/arnaud-ritti/openidea-plugins/commit/dab46135410cfc6954e005ec3cb0e7ef3ebc3cb2))
* **ideolog:** level panel beside log editors, previous error/warning/exception navigation ([8c5d0cc](https://github.com/arnaud-ritti/openidea-plugins/commit/8c5d0cca74749cb9d127e8a5090f8d8d5e3cd1c5))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **ideolog:** bound single huge events, keep partial folds, narrow event invalidation ([884503f](https://github.com/arnaud-ritti/openidea-plugins/commit/884503f290fd4ff6d31eca37e700a1cc07cc34e7))
* **ideolog:** colour whole logs and apply pattern changes immediately ([ed2ce2d](https://github.com/arnaud-ritti/openidea-plugins/commit/ed2ce2d00ddf608ead59cb6d8e25ab03865911dd))
* **ideolog:** plain live tail and JVM copy, language-scoped commands, one tab name ([be005c5](https://github.com/arnaud-ritti/openidea-plugins/commit/be005c5ceda933c1694e440488335f49d56db939))
* **ideolog:** scope log commands to log files, explain highlight limits, confirm bulk removal ([07699af](https://github.com/arnaud-ritti/openidea-plugins/commit/07699af23712bcfadc7fb09d4e2a2ce20944f8eb))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* precise permission mapping and command fallbacks ([a46fee8](https://github.com/arnaud-ritti/openidea-plugins/commit/a46fee83335d5cbd91a15ee1739d0f1517daef63))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>ignore: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/ignore-v0.3.0...ignore-v0.4.0) (2026-11-18)


### Features

* **symfony,ignore,liquibase:** make New & commands create the file ([797a5b2](https://github.com/arnaud-ritti/openidea-plugins/commit/797a5b2208f68b17560909f10cfd76dd268dda5f))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **ignore:** add selected paths in one step and plainer copy ([5ad3872](https://github.com/arnaud-ritti/openidea-plugins/commit/5ad3872f0f41d17b04a96807c7f79c43104c14d0))
* **ignore:** bound check computation at 128 KiB in the shared cache ([0ae60a7](https://github.com/arnaud-ritti/openidea-plugins/commit/0ae60a76e43365584d8c2509e9d0add378fb2471))
* **ignore:** bound redundancy checks again and share checked problems with quick fixes ([62f2f65](https://github.com/arnaud-ritti/openidea-plugins/commit/62f2f65f84e79a28a451471574cdb6de114a9bb8))
* **ignore:** compute checks on provider cache miss and evict one entry ([2ce97fa](https://github.com/arnaud-ritti/openidea-plugins/commit/2ce97fa884647005101bd7236b0186b5f242ac93))
* **ignore:** git-correct whitelist matching and linear-time wildcards ([65f3565](https://github.com/arnaud-ritti/openidea-plugins/commit/65f35658fe00ff5968c80c6978439413e8c5515a))
* **ignore:** plain Git scan command titles ([44823fc](https://github.com/arnaud-ritti/openidea-plugins/commit/44823fc976b374e54dd1207efbbf38fed353d717))
* **ignore:** single diagnostics producer and targeted path completion ([ca5ae3f](https://github.com/arnaud-ritti/openidea-plugins/commit/ca5ae3f56be379a0e85ff05b5b2663ace1cdcdd1))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>kube-explorer: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/kube-explorer-v0.3.0...kube-explorer-v0.4.0) (2026-11-18)


### Features

* **kube-explorer:** context in the status bar, live diff before apply, reusable shells ([b7bd51f](https://github.com/arnaud-ritti/openidea-plugins/commit/b7bd51f504d4e19ae6ed381f545603e6f44ce214))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* **devcontainer,kube-explorer:** real command names in messages, one category ([64e5776](https://github.com/arnaud-ritti/openidea-plugins/commit/64e577666b4cc78bba4b30048d4cad98d9501e68))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* **kube-explorer:** confirm live replace and manifest delete, explain kubectl failures ([24edfea](https://github.com/arnaud-ritti/openidea-plugins/commit/24edfeae1f70bd71d42b2b9d8304b1f7bf6e47b9))
* **kube-explorer:** diff the live objects against a server dry run with noise removed ([f084c62](https://github.com/arnaud-ritti/openidea-plugins/commit/f084c62b9e7a8e670b1d8023a2f6c351d7f589fa))
* **kube-explorer:** keep the indexer as cache warmer, bound import placeholders, one eviction cache ([0659274](https://github.com/arnaud-ritti/openidea-plugins/commit/0659274bc1a97613a690fcf90744d2cfc26e587a))
* **kube-explorer:** label and selector gutter icons list every matching resource ([fe62182](https://github.com/arnaud-ritti/openidea-plugins/commit/fe62182ee84c1bb0e2d823f0df8222663609bf7b))
* **kube-explorer:** name context and namespace before delete and apply, separate no-context state ([00d1fc8](https://github.com/arnaud-ritti/openidea-plugins/commit/00d1fc8b05d61ab2c206b5bf37d94e2fae2a81e9))
* **kube-explorer:** never write live Secret data into manifests on reload ([cb84afe](https://github.com/arnaud-ritti/openidea-plugins/commit/cb84afe5962d7d9b87cbddfe541e64a5d3d5a09c))
* **kube-explorer:** reload past forbidden kinds, refuse fetched Secrets, reject duplicate data keys ([ad2968e](https://github.com/arnaud-ritti/openidea-plugins/commit/ad2968e6beac8f427bbdcab9a180f04516596022))
* **kube-explorer:** resilient watch, use and delete imported kubeconfigs, readme and change notes ([ceb731a](https://github.com/arnaud-ritti/openidea-plugins/commit/ceb731aff94f6be2163c12ca14382b44904ed8ae))
* **kube-explorer:** single diagnostics producer, scoped cache invalidation, cached kustomize facts ([3f267f7](https://github.com/arnaud-ritti/openidea-plugins/commit/3f267f7649bfe5e70d12d0fb546c3a1151b571d2))
* **kube-explorer:** stat-checked label cache, line table, helm ranges, one-step port forward ([3cf00d0](https://github.com/arnaud-ritti/openidea-plugins/commit/3cf00d08dadd043b138ae7ce1c4c88c667c8fa38))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* remaining jargon in titles ([4edf7e8](https://github.com/arnaud-ritti/openidea-plugins/commit/4edf7e8ebe03a85b56445d5aebfcc60818d8bfa2))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>laravel: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/laravel-v0.3.0...laravel-v0.4.0) (2026-11-18)


### Features

* **laravel:** controller methods open the native route chooser instead of a Route actions dialog ([80d9bbc](https://github.com/arnaud-ritti/openidea-plugins/commit/80d9bbcab64348023e129e2917642c5ccb92d625))
* **laravel:** fold Schema into the Database tab and show tables as a tree ([8aeb68f](https://github.com/arnaud-ritti/openidea-plugins/commit/8aeb68f66b90436fc44643d2fd58ee83ee874243))
* **symfony:** Database tab with Doctrine schema, compare, rows and migrations ([ceda891](https://github.com/arnaud-ritti/openidea-plugins/commit/ceda89170aea90a2742ac7f9caf0b71010b9a82d))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* **laravel,symfony:** show each tool window only in its own kind of project ([42a2797](https://github.com/arnaud-ritti/openidea-plugins/commit/42a2797267dcf49b73155cef3c4b853fca0d95f0))
* **laravel:** cap each indexing step at 150 ms ([1b86467](https://github.com/arnaud-ritti/openidea-plugins/commit/1b86467f1ca5c3523731f7f11c63f98ddf9395da))
* **laravel:** commands finish their own edits, plain names and messages ([9c60bb6](https://github.com/arnaud-ritti/openidea-plugins/commit/9c60bb65db2742da7fbc779017f149c5d12a40dd))
* **laravel:** confirm destructive database commands, route picker from the palette, hide commands outside Laravel projects ([cafe3f2](https://github.com/arnaud-ritti/openidea-plugins/commit/cafe3f23c5f41f0c19b2ff61663463849789333e))
* **laravel:** controller route icons stay within the editor's time budget ([4bda806](https://github.com/arnaud-ritti/openidea-plugins/commit/4bda806f942bcf8f0abeca740ba88b56673d5a31))
* **laravel:** keep editor providers out of index setup and mask probe errors ([5934298](https://github.com/arnaud-ritti/openidea-plugins/commit/5934298ec10ae71c5f402094d4650d0b138fcf89))
* **laravel:** make Docker targets and long PHP probes work on a real daemon ([6924168](https://github.com/arnaud-ritti/openidea-plugins/commit/69241688bfbd5865c23d26576582fe8370e27e33))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* **laravel:** send request input over stdin and keep provider indexing in budget ([54b096a](https://github.com/arnaud-ritti/openidea-plugins/commit/54b096af26199e68c1e10d33d451b5cd168785c9))
* **laravel:** translate the renamed Docker, editor hint and cURL titles ([cf22b24](https://github.com/arnaud-ritti/openidea-plugins/commit/cf22b240b8f7e7d282c4345da49c3a97c90bc70d))
* **laravel:** translate the short setting titles ([a469b52](https://github.com/arnaud-ritti/openidea-plugins/commit/a469b523e85097f5cdd9f98ecaa47bd7008d69a2))
* **laravel:** use shortcuts no other plugin takes ([87a9510](https://github.com/arnaud-ritti/openidea-plugins/commit/87a9510b59c988c60f67407da0356e3ec025339e))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* remaining jargon in titles ([4edf7e8](https://github.com/arnaud-ritti/openidea-plugins/commit/4edf7e8ebe03a85b56445d5aebfcc60818d8bfa2))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* standard permission copy for the HTTP request file fallback ([1c7c3bf](https://github.com/arnaud-ritti/openidea-plugins/commit/1c7c3bf8a2cf93ba179a01feaff81972d79bb137))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* **symfony:** stop overdue Send Request and database probes, close the rename racy-mtime gap ([43898a2](https://github.com/arnaud-ritti/openidea-plugins/commit/43898a225be5e7af0f58fbb3eadc4cfedf674e12))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>liquibase: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/liquibase-v0.3.0...liquibase-v0.4.0) (2026-11-18)


### Features

* **liquibase:** show pending changesets and mark changelog files ([b565c30](https://github.com/arnaud-ritti/openidea-plugins/commit/b565c30fb92bd32f37022c4d2b5c765b546a0479))
* **symfony,ignore,liquibase:** make New & commands create the file ([797a5b2](https://github.com/arnaud-ritti/openidea-plugins/commit/797a5b2208f68b17560909f10cfd76dd268dda5f))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* **liquibase:** bound the events-denied fallback warm to the open file ([e1f7e7f](https://github.com/arnaud-ritti/openidea-plugins/commit/e1f7e7f9846540c8320944e262b11b73281bd397))
* **liquibase:** includeAll lines list their changelogs, Go to Definition shows every candidate ([8dc59da](https://github.com/arnaud-ritti/openidea-plugins/commit/8dc59da6e99981ebab81a1678424fb412ad323cb))
* **liquibase:** lead run confirmations with the effect, target and command ([f4e23be](https://github.com/arnaud-ritti/openidea-plugins/commit/f4e23be37ba369ce0125590e88fd2ced89d09117))
* **liquibase:** one deadline per event batch, forget unreached changelogs, warm files opened before activation ([73d188d](https://github.com/arnaud-ritti/openidea-plugins/commit/73d188d0f32ec557b58563701789aa198590060d))
* **liquibase:** plain confirmations, first changelog, shorter toolbars ([624609b](https://github.com/arnaud-ritti/openidea-plugins/commit/624609b9349c2589bc29fb1aa62f4cd0ef84fa8a))
* **liquibase:** ship include and property checks; faster positions ([befee62](https://github.com/arnaud-ritti/openidea-plugins/commit/befee6269c55079e93509d8296be4d7187da1d7d))
* **liquibase:** warm the open file when events are denied; bound warm budget ([4f8f2e1](https://github.com/arnaud-ritti/openidea-plugins/commit/4f8f2e131aca09092376896795dbcbb1866df598))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* precise permission mapping and command fallbacks ([a46fee8](https://github.com/arnaud-ritti/openidea-plugins/commit/a46fee83335d5cbd91a15ee1739d0f1517daef63))
* **properties,liquibase:** drop dead settings path and test JSON completion ([3a0f8c5](https://github.com/arnaud-ritti/openidea-plugins/commit/3a0f8c5f6a865954e349e7d39774da1a3f29bdbb))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* **spring-text:** share one XML walker, match drive letters case-insensitively ([61d354d](https://github.com/arnaud-ritti/openidea-plugins/commit/61d354d7122df0117b830a25290e76ceaa59d04f))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>openapi: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/openapi-v0.3.0...openapi-v0.4.0) (2026-11-18)


### Features

* **openapi:** gutter links to every $ref of an operation and to a component's usages ([f3572b5](https://github.com/arnaud-ritti/openidea-plugins/commit/f3572b541dbd8b6a0de21aa89b6491727c3c5a67))
* **openapi:** operations panel beside spec editors ([e58517d](https://github.com/arnaud-ritti/openidea-plugins/commit/e58517d29b8d58db57c336864a46661357292a74))
* **openapi:** Try it out for YAML specs and a Try button in the preview ([8391499](https://github.com/arnaud-ritti/openidea-plugins/commit/8391499ccab9ebb88df62eb1002f7887d2c476e2))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* brand logos for the related-files gutter icons ([10c28d6](https://github.com/arnaud-ritti/openidea-plugins/commit/10c28d68d95fbff873034c530bbc0a29401d9590))
* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* **openapi:** close private-host bypasses and hidden-dir fallback ([42cc5fd](https://github.com/arnaud-ritti/openidea-plugins/commit/42cc5fd722f4662693fd5c38f7e49668b6d3ddc2))
* **openapi:** default request base URL to the spec server, remember the preview renderer ([fb30260](https://github.com/arnaud-ritti/openidea-plugins/commit/fb302601ef7848d8729d423cb4944db7f6c0e0e4))
* **openapi:** keep the rename confirm under the host's size limit ([0a86b79](https://github.com/arnaud-ritti/openidea-plugins/commit/0a86b799c740fb2e617563c921c62bb426df525a))
* **openapi:** name the DNS-rebinding ceiling of the host guard ([ff8df2a](https://github.com/arnaud-ritti/openidea-plugins/commit/ff8df2ad7d6942ef0dc825cbbb50eea63271ced1))
* **openapi:** palette endpoint picker, spec-only commands, codegen guidance ([ddab4d8](https://github.com/arnaud-ritti/openidea-plugins/commit/ddab4d85ca2a010304818cc2ce7d18ad22fba537))
* **openapi:** plain wording in Try it out, rename and codegen messages ([ad8d00e](https://github.com/arnaud-ritti/openidea-plugins/commit/ad8d00e754c6c8686916e462276369ca24c4e8cc))
* **openapi:** skip non-spec files, ignore staging dirs, refuse private hosts ([b97cf90](https://github.com/arnaud-ritti/openidea-plugins/commit/b97cf90592e5b385cd027ede8d4d1ee4d2fceb67))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-text:** share one XML walker, match drive letters case-insensitively ([61d354d](https://github.com/arnaud-ritti/openidea-plugins/commit/61d354d7122df0117b830a25290e76ceaa59d04f))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* standard permission copy for the HTTP request file fallback ([1c7c3bf](https://github.com/arnaud-ritti/openidea-plugins/commit/1c7c3bf8a2cf93ba179a01feaff81972d79bb137))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>properties: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/properties-v0.3.0...properties-v0.4.0) (2026-11-18)


### Features

* commands and menu entries for toolbar-only actions ([940dcf2](https://github.com/arnaud-ritti/openidea-plugins/commit/940dcf2ae8ebff44ec7b104af38b7559eb86bff9))
* **properties:** gutter links to the same key in other locale files ([c4d1c28](https://github.com/arnaud-ritti/openidea-plugins/commit/c4d1c28a6ada45cf06f8e595734c8c3c69e2a9ca))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* **properties,liquibase:** drop dead settings path and test JSON completion ([3a0f8c5](https://github.com/arnaud-ritti/openidea-plugins/commit/3a0f8c5f6a865954e349e7d39774da1a3f29bdbb))
* **properties:** activate for html and jsp; pin Properties parser edge cases ([ff19835](https://github.com/arnaud-ritti/openidea-plugins/commit/ff198359e6956e5526d68a781f5c066f0028a136))
* **properties:** add Reformat File to the editor context menu ([5cc0330](https://github.com/arnaud-ritti/openidea-plugins/commit/5cc033019c73f8a71a7f6e067ecb861f193c0221))
* **properties:** drop unused test import ([1a3cf7e](https://github.com/arnaud-ritti/openidea-plugins/commit/1a3cf7ea46e7443f85f940119abd45b24b67d050))
* **properties:** Java-correct continuation keys; narrower activation ([ab402ef](https://github.com/arnaud-ritti/openidea-plugins/commit/ab402efca7678191edaa74c6764fb28686de7174))
* **properties:** one name for locale edits, shorter bundle toolbar, plain copy ([55e20c7](https://github.com/arnaud-ritti/openidea-plugins/commit/55e20c72655ccbbef7aa915119469de44c8d8ff1))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* **spring-text:** share one XML walker, match drive letters case-insensitively ([61d354d](https://github.com/arnaud-ritti/openidea-plugins/commit/61d354d7122df0117b830a25290e76ceaa59d04f))
</details>

<details><summary>quarkus: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/quarkus-v0.3.0...quarkus-v0.4.0) (2026-11-18)


### Features

* **quarkus:** create the starter directly when project.write is allowed ([5df90f3](https://github.com/arnaud-ritti/openidea-plugins/commit/5df90f3c1761a0e4503f9d658c42135c7e16b82e))
* **quarkus:** gutter links between config keys and @ConfigProperty injections ([53bd171](https://github.com/arnaud-ritti/openidea-plugins/commit/53bd17181005751ae49d288938ed7210f7390388))


### Bug Fixes

* brand logos for the related-files gutter icons ([10c28d6](https://github.com/arnaud-ritti/openidea-plugins/commit/10c28d68d95fbff873034c530bbc0a29401d9590))
* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* **quarkus:** activate only for Quarkus projects ([fd3a4fb](https://github.com/arnaud-ritti/openidea-plugins/commit/fd3a4fbe44cf50cb876d9a84a540563d0daabba2))
* **quarkus:** one-confirm starter, palette path for lens details, plain copy ([e37ebdf](https://github.com/arnaud-ritti/openidea-plugins/commit/e37ebdf5ed8697d678c4b1984dd78bda3275e56d))
* **quarkus:** plain wording in JAR metadata messages ([12a2ca4](https://github.com/arnaud-ritti/openidea-plugins/commit/12a2ca4c47f17eaa6593cf05c35b1a610b31ab09))
* **quarkus:** state the gitignored .env limit precisely, test late Quarkus dependency ([eb891e1](https://github.com/arnaud-ritti/openidea-plugins/commit/eb891e18be4a1f873de5f029b04381c62d8edb7b))
* remaining jargon in titles ([4edf7e8](https://github.com/arnaud-ritti/openidea-plugins/commit/4edf7e8ebe03a85b56445d5aebfcc60818d8bfa2))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-text:** share one XML walker, match drive letters case-insensitively ([61d354d](https://github.com/arnaud-ritti/openidea-plugins/commit/61d354d7122df0117b830a25290e76ceaa59d04f))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
</details>

<details><summary>spring-beans: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-beans-v0.3.0...spring-beans-v0.4.0) (2026-11-18)


### Features

* commands and menu entries for toolbar-only actions ([940dcf2](https://github.com/arnaud-ritti/openidea-plugins/commit/940dcf2ae8ebff44ec7b104af38b7559eb86bff9))
* **spring-beans:** ambiguous injections open the native target chooser ([e8aaf69](https://github.com/arnaud-ritti/openidea-plugins/commit/e8aaf699b21e092a39788590fd62ff5f3882479a))
* **spring-beans:** find and focus beans in the graph ([cec1af0](https://github.com/arnaud-ritti/openidea-plugins/commit/cec1af0bb930cdc03d237c8f21c57dd3fc1e19f5))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-beans:** budget-safe providers, no 512-model cliff, hunk-only confirms ([3c29727](https://github.com/arnaud-ritti/openidea-plugins/commit/3c2972779c64850969177c6dd585d9ee21ce77f3))
* **spring-beans:** dock the Spring Beans window on the right with the other Spring views ([b38040a](https://github.com/arnaud-ritti/openidea-plugins/commit/b38040ac072a38730ad52ba29c44376c45841abe))
* **spring-beans:** exact-name index anchors, cached lookups, per-root inventory cap ([fc02492](https://github.com/arnaud-ritti/openidea-plugins/commit/fc02492bb65eb0ceff5ebe43611458074eabe411))
* **spring-beans:** finish a scan in one Refresh, re-run reports ([a0fba85](https://github.com/arnaud-ritti/openidea-plugins/commit/a0fba8545cffd154d0d221eedbaf5fbd1668b069))
* **spring-beans:** hide views outside JVM build projects, consistent titles, actionable empty states ([6ddfef3](https://github.com/arnaud-ritti/openidea-plugins/commit/6ddfef3d3167277de5c6e00d6554903591d26e64))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>spring-boot: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-boot-v0.3.0...spring-boot-v0.4.0) (2026-11-18)


### Features

* commands and menu entries for toolbar-only actions ([940dcf2](https://github.com/arnaud-ritti/openidea-plugins/commit/940dcf2ae8ebff44ec7b104af38b7559eb86bff9))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* precise permission mapping and command fallbacks ([a46fee8](https://github.com/arnaud-ritti/openidea-plugins/commit/a46fee83335d5cbd91a15ee1739d0f1517daef63))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-boot:** Find Property only in .properties editors, map denials ([0fb54a0](https://github.com/arnaud-ritti/openidea-plugins/commit/0fb54a00ce2e7de82f006859dbecdc88b4b0b0bd))
* **spring-boot:** flat YAML logging levels, nested import profiles, FQN enum match ([d56c86d](https://github.com/arnaud-ritti/openidea-plugins/commit/d56c86d91909aec1b31f84b20a2540508d8f21e9))
* **spring-boot:** one live index for providers, correct enum checks, JAR import fixes ([15e004c](https://github.com/arnaud-ritti/openidea-plugins/commit/15e004cbfaf2f967c64088fec7be14fad11a09b1))
* **spring-boot:** use spring-text balanced in rename; no simple-name enum fallback for package-qualified types ([47d9a3f](https://github.com/arnaud-ritti/openidea-plugins/commit/47d9a3f8acb54bc7deeddeaa10cfcad23a5ebd24))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
</details>

<details><summary>spring-data: 0.3.1</summary>

## [0.3.1](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-data-v0.3.0...spring-data-v0.3.1) (2026-11-18)


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-data:** keep derived-query checks on after idle and per-type completeness ([e586e82](https://github.com/arnaud-ritti/openidea-plugins/commit/e586e8219d18086caa51637d79e9b6b1632d5f98))
* **spring-data:** list inherited repository members in a picker ([ff37a54](https://github.com/arnaud-ritti/openidea-plugins/commit/ff37a545e4e4899f0c749030535280fdb37420ac))
* **spring-data:** prefix palette title with its category ([ce4d613](https://github.com/arnaud-ritti/openidea-plugins/commit/ce4d6131b78343f22ad96df096dd72b90f1b7ef7))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
</details>

<details><summary>spring-initializr: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-initializr-v0.3.0...spring-initializr-v0.4.0) (2026-11-18)


### Features

* **spring-initializr:** make Generate and Open the primary action with one confirmation ([f16b153](https://github.com/arnaud-ritti/openidea-plugins/commit/f16b153fc8439489ee078798d8561f6a2e32f539))
* **spring-initializr:** write starters and imports directly when project.write is allowed ([c192f25](https://github.com/arnaud-ritti/openidea-plugins/commit/c192f258392a8ea158338a198eda9d56e2052c92))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* remaining jargon in titles ([4edf7e8](https://github.com/arnaud-ritti/openidea-plugins/commit/4edf7e8ebe03a85b56445d5aebfcc60818d8bfa2))
* **spring-initializr:** add starters from the command, gate import menu, plain copy ([912635e](https://github.com/arnaud-ritti/openidea-plugins/commit/912635efe48d7439dbfcfe6c85bfdfa8603e582e))
* **spring-initializr:** larger build files, local-only generation, cleanup ([1d2447b](https://github.com/arnaud-ritti/openidea-plugins/commit/1d2447b4362d510da8945c3dbeacf762f8556dc8))
* **spring-initializr:** name the import command after what it does, plain starter confirm ([07a0d81](https://github.com/arnaud-ritti/openidea-plugins/commit/07a0d811b68af6414da4b6f06c5f8a9c97219fc0))
* **spring-initializr:** name the missing python3/net grant; clearer not-local wording ([52e40be](https://github.com/arnaud-ritti/openidea-plugins/commit/52e40be452bec30d2c8c90c1d4ed6379669c7169))
* **spring-initializr:** single-sentence status line, plain import title ([4ef73fc](https://github.com/arnaud-ritti/openidea-plugins/commit/4ef73fcfd7df226e00e35ae1083f62dc82a71366))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>spring-integration: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-integration-v0.3.0...spring-integration-v0.4.0) (2026-11-18)


### Features

* commands and menu entries for toolbar-only actions ([940dcf2](https://github.com/arnaud-ritti/openidea-plugins/commit/940dcf2ae8ebff44ec7b104af38b7559eb86bff9))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* precise permission mapping and command fallbacks ([a46fee8](https://github.com/arnaud-ritti/openidea-plugins/commit/a46fee83335d5cbd91a15ee1739d0f1517daef63))
* remaining jargon in titles ([4edf7e8](https://github.com/arnaud-ritti/openidea-plugins/commit/4edf7e8ebe03a85b56445d5aebfcc60818d8bfa2))
* **spring-integration:** Batch CLI commands only while running, plain copy ([1091de5](https://github.com/arnaud-ritti/openidea-plugins/commit/1091de5ca7ff1c1ad5cd5f48b8f62a14f2452cfc))
* **spring-integration:** channel gutter icons list every producer or consumer ([760fd4e](https://github.com/arnaud-ritti/openidea-plugins/commit/760fd4e98d804d4c2a097de0637cf6f2cf285844))
* **spring-integration:** cross-file XML navigation, scoped suppression, tolerant XML ([bc810c2](https://github.com/arnaud-ritti/openidea-plugins/commit/bc810c2671682d559cdcfb9fccd6c51b4383e1f6))
* **spring-integration:** gate XML walks on spring_text analysis_valid/generation_valid instead of a 10 ms timer ([3e97648](https://github.com/arnaud-ritti/openidea-plugins/commit/3e9764868266fa294aabe5990d84fd2fcff1bb25))
* **spring-integration:** hide view outside JVM build projects, plain titles and messages ([7348765](https://github.com/arnaud-ritti/openidea-plugins/commit/73487653e206932fcb970df0c7260a711231ff1e))
* **spring-integration:** one shared index, reset-proof contexts cache, shared helpers ([e804d76](https://github.com/arnaud-ritti/openidea-plugins/commit/e804d7671b976062e5cdb3622fabb0c4596217bb))
* **spring-integration:** reject lowercase doctype in batch navigation ([22a20fe](https://github.com/arnaud-ritti/openidea-plugins/commit/22a20fe880dfb9331220894b0e289fcbdf0e6e8e))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* **spring-text:** spring-web diagnostics never discover; applies reads each build file once ([fc2e40d](https://github.com/arnaud-ritti/openidea-plugins/commit/fc2e40d8d645965294a29b9a23c1cdbd59399799))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>spring-osgi: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-osgi-v0.3.0...spring-osgi-v0.4.0) (2026-11-18)


### Features

* **dotenv,spring-osgi:** settings to turn checks off ([63a8525](https://github.com/arnaud-ritti/openidea-plugins/commit/63a852568124c8f5e29de7edd0f5cdae4e788a73))


### Bug Fixes

* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* **spring-osgi:** keep checks after open/save, tolerant XML analysis ([070d6c0](https://github.com/arnaud-ritti/openidea-plugins/commit/070d6c039cb7476ec77b4abd657157e6c31b68e0))
* **spring-osgi:** raise the project.read notice only for OSGi files ([51a2c75](https://github.com/arnaud-ritti/openidea-plugins/commit/51a2c75428d1087937f875714cd480b401938c1a))
* **spring-osgi:** use spring-text scope for ownership and generation, accept drive-letter roots ([1dfd559](https://github.com/arnaud-ritti/openidea-plugins/commit/1dfd559dd08f0c12ccf8c1ce7e98bedc82c8bf6d))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
</details>

<details><summary>spring-runtime: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-runtime-v0.3.0...spring-runtime-v0.4.0) (2026-11-18)


### Features

* **spring-runtime:** Open Settings button in the empty view ([f793064](https://github.com/arnaud-ritti/openidea-plugins/commit/f793064bece508243275c387298b77a80df519d7))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-runtime:** hide views outside JVM build projects, plain auto-refresh wording, no Clear on empty view ([afc78cb](https://github.com/arnaud-ritti/openidea-plugins/commit/afc78cba3839544ca3227c29f471045e1fc1a105))
* **spring-runtime:** mask actuator env secrets in storage, tree and tooltips ([e21569d](https://github.com/arnaud-ritti/openidea-plugins/commit/e21569de1f47ba0b17851117718b4300cdc72286))
* **spring-runtime:** mask compound secret names, rewrite older caches masked, tighten JMX and URL masking ([de76285](https://github.com/arnaud-ritti/openidea-plugins/commit/de76285bc8e8e398369db34439153b77d443f71a))
* **spring-runtime:** mask JMX operation leaves, rank graph from cached index only ([78dd9cc](https://github.com/arnaud-ritti/openidea-plugins/commit/78dd9ccf9982b6e12ba823d5ddb166344def9b64))
* **spring-runtime:** re-mask pre-schema-5 caches with the full parse-time masking ([b9e0960](https://github.com/arnaud-ritti/openidea-plugins/commit/b9e0960cfab1b49d358743762864e3db9219f885))
* **spring-runtime:** stable source navigation and per-project snapshots ([9b13b90](https://github.com/arnaud-ritti/openidea-plugins/commit/9b13b9070aa57f83b5d52bc1794ca9afa3703e61))
* **spring-runtime:** start auto-refresh from the command, confirm Clear ([0dbba6f](https://github.com/arnaud-ritti/openidea-plugins/commit/0dbba6f580d217a90bff5782d9b3c11aea08edc3))
* **spring-runtime:** withhold deep JMX dumps, split attribute at first hash, strict userinfo masking ([3abe91f](https://github.com/arnaud-ritti/openidea-plugins/commit/3abe91f7803bc88013b09b8425c13b398ddc3934))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>spring-security: 0.3.1</summary>

## [0.3.1](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-security-v0.3.0...spring-security-v0.3.1) (2026-11-18)


### Bug Fixes

* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-security:** cache bean incompleteness per generation, accept DOCTYPE in ownership ([4899fd8](https://github.com/arnaud-ritti/openidea-plugins/commit/4899fd813cb8fc71831ae1afef3a8974b36f57be))
* **spring-security:** correct Java comment handling, fresh bean checks ([188619c](https://github.com/arnaud-ritti/openidea-plugins/commit/188619c490458dc42a00ac8e94600eb1342ca1c8))
* **spring-security:** short setting titles without internal caveats ([eeb6d8c](https://github.com/arnaud-ritti/openidea-plugins/commit/eeb6d8c627baa54ce847b0106b667e4407eb66c0))
* **spring-security:** use spring-text scope and XML gates instead of local copies and a timer-bound walker ([ede6330](https://github.com/arnaud-ritti/openidea-plugins/commit/ede63304da24ad4a03d13353bf9dc002e19c6aec))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
</details>

<details><summary>spring-shell: 0.3.1</summary>

## [0.3.1](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-shell-v0.3.0...spring-shell-v0.3.1) (2026-11-18)


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* precise permission mapping and command fallbacks ([a46fee8](https://github.com/arnaud-ritti/openidea-plugins/commit/a46fee83335d5cbd91a15ee1739d0f1517daef63))
* **spring-shell:** gate component fix on trusted index, cache extended set ([b3f2f88](https://github.com/arnaud-ritti/openidea-plugins/commit/b3f2f88dd2c2d56c0e57714a37ea1628f8012685))
* **spring-shell:** plain diagnostic wording ([a0433a1](https://github.com/arnaud-ritti/openidea-plugins/commit/a0433a1ee160d58f53d3e178ee3bb38c3c80e0bc))
* **spring-shell:** plain diagnostics, @ShellComponent fix and rename preview ([095d1db](https://github.com/arnaud-ritti/openidea-plugins/commit/095d1dba0514f7e256e24761217262d3e2f62eda))
* **spring-shell:** safe rename on partial index, file-change tracking ([8c64db7](https://github.com/arnaud-ritti/openidea-plugins/commit/8c64db736d09f8739e4ddc33c72a1f7f688a5f44))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
</details>

<details><summary>spring-web: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-web-v0.3.0...spring-web-v0.4.0) (2026-11-18)


### Features

* **spring-web:** resolve Feign navigation by service identity ([#16](https://github.com/arnaud-ritti/openidea-plugins/issues/16)) ([3d4ae08](https://github.com/arnaud-ritti/openidea-plugins/commit/3d4ae08511e1a179e2be6f06a03d02d39ff9b121))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* remaining jargon in titles ([4edf7e8](https://github.com/arnaud-ritti/openidea-plugins/commit/4edf7e8ebe03a85b56445d5aebfcc60818d8bfa2))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* **spring-text:** spring-web diagnostics never discover; applies reads each build file once ([fc2e40d](https://github.com/arnaud-ritti/openidea-plugins/commit/fc2e40d8d645965294a29b9a23c1cdbd59399799))
* **spring-web:** gutter icons list every route, template and module target ([d787eff](https://github.com/arnaud-ritti/openidea-plugins/commit/d787eff531fa45290cbc6befa054c6d0607fceb9))
* **spring-web:** hide views outside JVM build projects, endpoint picker for palette commands, consistent titles ([1d6f436](https://github.com/arnaud-ritti/openidea-plugins/commit/1d6f4367200c93e34a666024c1cfe29fdbe94a81))
* **spring-web:** one HTTP icon per request mapping ([abedc56](https://github.com/arnaud-ritti/openidea-plugins/commit/abedc56fe4c9c9754c9dadf5f0a7239022207f44))
* **spring-web:** partial endpoint indexes, shared provider budget, per-path event updates ([231f5e2](https://github.com/arnaud-ritti/openidea-plugins/commit/231f5e2909af39b1f66e9f830307251910b4580c))
* **spring-web:** plainer GraphQL subscription flow and empty states ([c0ab281](https://github.com/arnaud-ritti/openidea-plugins/commit/c0ab28197fb83374f8d6fb79a2bdf4d36ffa76b5))
* **spring-web:** share coverage notes, build detection and bracket check from spring-text ([ed34a86](https://github.com/arnaud-ritti/openidea-plugins/commit/ed34a8635d0e25037415efbe1ba267b106b4d69e))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>spring-ws: 0.3.1</summary>

## [0.3.1](https://github.com/arnaud-ritti/openidea-plugins/compare/spring-ws-v0.3.0...spring-ws-v0.3.1) (2026-11-18)


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* prefix the .env and Spring WS command titles ([8b1aeb2](https://github.com/arnaud-ritti/openidea-plugins/commit/8b1aeb2a9dfa66d1352655909397e5b87c2d5c68))
* **spring-text:** shared tolerant XML validator, Windows-aware scope and build detection ([c8b343c](https://github.com/arnaud-ritti/openidea-plugins/commit/c8b343ccf5939596fd27c54b7ad6660105d89db0))
* **spring-text:** spring-web diagnostics never discover; applies reads each build file once ([fc2e40d](https://github.com/arnaud-ritti/openidea-plugins/commit/fc2e40d8d645965294a29b9a23c1cdbd59399799))
* **spring-ws:** endpoint and schema gutter icons list every match ([351f498](https://github.com/arnaud-ritti/openidea-plugins/commit/351f4987ebbbb5cc0aef0ea22dd14e15af29483a))
* **spring-ws:** gate warms on Spring-WS, restore fixture brace, exact cap report ([e703e8c](https://github.com/arnaud-ritti/openidea-plugins/commit/e703e8c88cd952c9b1e117e0cff41fabb3177d4d))
* **spring-ws:** large schemas, fresh indexes after idle, warm only with Spring-WS ([1289442](https://github.com/arnaud-ritti/openidea-plugins/commit/12894424b756cd177a4b24648a786425307d02b5))
* **spring-ws:** plain endpoint scaffold flow, clearer titles, check settings ([481e6f6](https://github.com/arnaud-ritti/openidea-plugins/commit/481e6f6d960e64d1e9a0c98b8e39c36827b236c5))
* **spring-ws:** reject lowercase doctype in generated schema check ([2eafd1b](https://github.com/arnaud-ritti/openidea-plugins/commit/2eafd1bc7fd46d3325e77f373cfbb98a22716cbe))
* **spring-ws:** reuse spring-text XML gate, cache the scaffold cap per root, fix byte wording ([adfca14](https://github.com/arnaud-ritti/openidea-plugins/commit/adfca140afc2adc6e33b3da3f9d435f83d67eddf))
</details>

<details><summary>symfony: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/symfony-v0.3.0...symfony-v0.4.0) (2026-11-18)


### Features

* **symfony,ignore,liquibase:** make New & commands create the file ([797a5b2](https://github.com/arnaud-ritti/openidea-plugins/commit/797a5b2208f68b17560909f10cfd76dd268dda5f))
* **symfony:** Components and Events tabs ([33ab65a](https://github.com/arnaud-ritti/openidea-plugins/commit/33ab65aff2595ea94b57bd65d9f9645334e45e04))
* **symfony:** Console and Generators tabs and console MCP tools ([653df7b](https://github.com/arnaud-ritti/openidea-plugins/commit/653df7b46e321fb364a2bc27f51a8d7660babe40))
* **symfony:** create missing classes, unused templates, run command from class and new project ([3c3f262](https://github.com/arnaud-ritti/openidea-plugins/commit/3c3f26216a912c776591301b6db50aef818b5248))
* **symfony:** Database tab with Doctrine schema, compare, rows and migrations ([ceda891](https://github.com/arnaud-ritti/openidea-plugins/commit/ceda89170aea90a2742ac7f9caf0b71010b9a82d))
* **symfony:** Docker target, Send Request and route code lenses ([70384ff](https://github.com/arnaud-ritti/openidea-plugins/commit/70384ff4530c9195925252d6621b81fa89c74511))
* **symfony:** functional test, Twig template, translation key and injection quick fixes ([f94877f](https://github.com/arnaud-ritti/openidea-plugins/commit/f94877f0b78ea8b66581ea2f31fdfcf69a37227c))
* **symfony:** link hovers to the Symfony docs ([c5d206b](https://github.com/arnaud-ritti/openidea-plugins/commit/c5d206b7fb12b4e005d89284a44d1fd234a4e7e3))
* **symfony:** open the Symfony tool window at the bottom by default ([4ccf3d5](https://github.com/arnaud-ritti/openidea-plugins/commit/4ccf3d54eabd257962a0a9d65b803b6c060e3359))
* **symfony:** Related files gutter icon for controllers, entities and templates ([f7e5bdd](https://github.com/arnaud-ritti/openidea-plugins/commit/f7e5bddbc8d0d7653287bba587ec1d79228a5fe9))
* **symfony:** rename Twig templates, translation keys and service ids ([b0fd3d6](https://github.com/arnaud-ritti/openidea-plugins/commit/b0fd3d68c430ee6bc7cf40e325bf76066fbc5c0b))
* **symfony:** run configurations for serve, tests and workers ([08ec811](https://github.com/arnaud-ritti/openidea-plugins/commit/08ec811e1637b21f9e750bc3cdeb51a7242d4222))
* **symfony:** Runtime tab with app log, error stack frames and Messenger failed messages ([2164450](https://github.com/arnaud-ritti/openidea-plugins/commit/2164450eda8d0c7199950d43bc52e6c5c02768aa))
* **symfony:** Services, Templates, Entities and Translations as searchable tables ([4bd83ed](https://github.com/arnaud-ritti/openidea-plugins/commit/4bd83edd6ecf39a9c99db411a040ac5aa1434b37))
* **symfony:** validation constraints and request DTOs from code ([2b352a4](https://github.com/arnaud-ritti/openidea-plugins/commit/2b352a40ba17834abbc3d5c5208c1b1b6f9c6c2c))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* brand logos for the related-files gutter icons ([10c28d6](https://github.com/arnaud-ritti/openidea-plugins/commit/10c28d68d95fbff873034c530bbc0a29401d9590))
* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* explain what to select when a row command runs from Search Everywhere ([47728bd](https://github.com/arnaud-ritti/openidea-plugins/commit/47728bdd035532879ddc1b6ea60357ca7b769c84))
* **laravel,symfony:** show each tool window only in its own kind of project ([42a2797](https://github.com/arnaud-ritti/openidea-plugins/commit/42a2797267dcf49b73155cef3c4b853fca0d95f0))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* precise permission mapping and command fallbacks ([a46fee8](https://github.com/arnaud-ritti/openidea-plugins/commit/a46fee83335d5cbd91a15ee1739d0f1517daef63))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* standard permission copy everywhere ([93aa4d7](https://github.com/arnaud-ritti/openidea-plugins/commit/93aa4d74846487a98a4d1c30a2494a9cf96a9b43))
* standard permission copy for the HTTP request file fallback ([1c7c3bf](https://github.com/arnaud-ritti/openidea-plugins/commit/1c7c3bf8a2cf93ba179a01feaff81972d79bb137))
* **symfony:** confirm and redact agent console runs, keep manual Docker out of run configurations ([0de5f9f](https://github.com/arnaud-ritti/openidea-plugins/commit/0de5f9f8f69f82ab26f257bf1ce2e2bdb793234c))
* **symfony:** confirm database and request writes against the running app ([6518cf3](https://github.com/arnaud-ritti/openidea-plugins/commit/6518cf3ad56d6f58f97f65819b538a42f7ab151e))
* **symfony:** fix crashes on non-ASCII text and unclosed Twig tags ([1046564](https://github.com/arnaud-ritti/openidea-plugins/commit/1046564ef66d7df1a7799ab14c9933d3f9715141))
* **symfony:** fix template_blocks crash on unclosed Twig tags ([84afec3](https://github.com/arnaud-ritti/openidea-plugins/commit/84afec3048349e6151c64676579ee979946d0f25))
* **symfony:** incremental index updates and budget-safe completion ([091d8f5](https://github.com/arnaud-ritti/openidea-plugins/commit/091d8f56bc12a4101ae03365a14d74510c935863))
* **symfony:** plain Profiler and check messages, scoped Twig commands ([426764a](https://github.com/arnaud-ritti/openidea-plugins/commit/426764a5540c7b2fd93050dd49a26a18bbcb62d3))
* **symfony:** plain wording in confirm previews and checks ([7da6974](https://github.com/arnaud-ritti/openidea-plugins/commit/7da6974b00d63f12d0f5114322180f5cfa23d855))
* **symfony:** publish exact marker definitions, track Twig lexer files per file ([1f5a299](https://github.com/arnaud-ritti/openidea-plugins/commit/1f5a2996a065731255d1ba6e716d73ad4685e3d5))
* **symfony:** publish only rename-closed route anchors ([6830401](https://github.com/arnaud-ritti/openidea-plugins/commit/68304017a32c6f99b503a0c0d0496d68f788acc8))
* **symfony:** publish route usages only when Rename reaches an exact indexed declaration ([a82741b](https://github.com/arnaud-ritti/openidea-plugins/commit/a82741bb2e432cd684ca133d027c33463acc163a))
* **symfony:** route picker from the palette, hide commands outside Symfony projects ([ab5016b](https://github.com/arnaud-ritti/openidea-plugins/commit/ab5016b098259de4bb66ad76b339c36c173bec28))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* **symfony:** scope rename scans, share PHP parses and preview translation extracts ([8936b72](https://github.com/arnaud-ritti/openidea-plugins/commit/8936b7205ffee13bebc4961fef09c3d260fb82fe))
* **symfony:** show the docker exec line in the Processes tab when a container is chosen ([352c4d5](https://github.com/arnaud-ritti/openidea-plugins/commit/352c4d54b370f4ea3e62c87631397b7d25731bac))
* **symfony:** stop overdue Send Request and database probes, close the rename racy-mtime gap ([43898a2](https://github.com/arnaud-ritti/openidea-plugins/commit/43898a225be5e7af0f58fbb3eadc4cfedf674e12))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))
</details>

<details><summary>tasks: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/tasks-v0.3.0...tasks-v0.4.0) (2026-11-18)


### Features

* commands and menu entries for toolbar-only actions ([940dcf2](https://github.com/arnaud-ritti/openidea-plugins/commit/940dcf2ae8ebff44ec7b104af38b7559eb86bff9))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* **tasks:** clearer close and empty states, plain caret command titles ([d7814a6](https://github.com/arnaud-ritti/openidea-plugins/commit/d7814a6a0154f4b7d190dd564cd55af133da9b12))
* **tasks:** keep issues of failed trackers; unique GitHub ids ([ddc6168](https://github.com/arnaud-ritti/openidea-plugins/commit/ddc616845b97c256c50f84b68676f15badda150e))
* **tasks:** keep the commit template outside .git ([967a70d](https://github.com/arnaud-ritti/openidea-plugins/commit/967a70d01f74055793ad6c4e6e2719d86d26819d))
* **tasks:** let git write the commit template into the git dir; plugin dir only without commits ([c0a0a6c](https://github.com/arnaud-ritti/openidea-plugins/commit/c0a0a6c571fb2803a3595e606e80d753aa427ad9))
* **tasks:** plain confirmations, shorter settings and a compact toolbar ([8fb2d1e](https://github.com/arnaud-ritti/openidea-plugins/commit/8fb2d1ea60ba55c20c21266907a15b3c36e3e68e))
* **tasks:** reveal the Tasks window when Open Task needs its form ([96a292a](https://github.com/arnaud-ritti/openidea-plugins/commit/96a292ad7a739a01fd0d5aa144ac2cac98c1279b))
* **tasks:** send tracker credentials only to validated hosts ([d386b5a](https://github.com/arnaud-ritti/openidea-plugins/commit/d386b5abe622e668cc58cc7d432a3c38dd05586c))
* **tasks:** validate GitHub repositories and surface host errors on close ([92ed52e](https://github.com/arnaud-ritti/openidea-plugins/commit/92ed52e803c47c778f00a8f7ffdc2d0a68d1b7aa))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
</details>

<details><summary>twig: 0.4.0</summary>

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/twig-v0.3.0...twig-v0.4.0) (2026-11-18)


### Features

* **twig:** quick fix to insert the missing end tag ([4014bf8](https://github.com/arnaud-ritti/openidea-plugins/commit/4014bf8f3eb0b6bf4515cc9de9a9ad9055ace6b2))
* **twig:** related-template gutter icons for extends, includes and block overrides ([38156eb](https://github.com/arnaud-ritti/openidea-plugins/commit/38156eb7283e8e28ccbbfe82a6d3d2c931f194f7))


### Bug Fixes

* brand logos for the related-files gutter icons ([10c28d6](https://github.com/arnaud-ritti/openidea-plugins/commit/10c28d68d95fbff873034c530bbc0a29401d9590))
* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* short setting titles ([3a12e66](https://github.com/arnaud-ritti/openidea-plugins/commit/3a12e6689794c36e9966536ea31838ec6a671018))
* tell users which permission to allow (part 2) ([ff1288d](https://github.com/arnaud-ritti/openidea-plugins/commit/ff1288d90688ec61691439f98db08e6a8d8b7a0c))
* **twig:** drop the empty document indexer ([2ff9a17](https://github.com/arnaud-ritti/openidea-plugins/commit/2ff9a1763067b0db6707559b4b8c98f3105d8d14))
* **twig:** keep project scans out of editor providers ([32fbc39](https://github.com/arnaud-ritti/openidea-plugins/commit/32fbc39644861ce3493979c265d98f8d93d4f4f6))
* **twig:** plain rename and occurrence messages, Twig-only commands ([2877a47](https://github.com/arnaud-ritti/openidea-plugins/commit/2877a47eb0cfa541075a32b489d6eda287e13186))
* **twig:** plain setting titles and descriptions, correct occurrence lens wording ([357e908](https://github.com/arnaud-ritti/openidea-plugins/commit/357e908ac5c0347eac921888db5779841b174ed2))
* **twig:** update Lexer facts per saved file, requeue instead of clearing on event overflow, stop tags at an unquoted &lt; ([95a6144](https://github.com/arnaud-ritti/openidea-plugins/commit/95a614453bd5945a9b9ee7a2e58f811549ed74ed))
</details>

---
This PR was generated with [Release Please](https://github.com/googleapis/release-please). See [documentation](https://github.com/googleapis/release-please#release-please).